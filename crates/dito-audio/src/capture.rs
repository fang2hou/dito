use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SampleFormat};
use dito_core::audio::{AudioClip, SAMPLE_RATE};
use dito_core::controller::{Recorder, RecorderError};

use crate::resample::Resampler;
use crate::transport;

/// Hard cap so a forgotten recording cannot eat memory (10 minutes).
const MAX_SAMPLES: usize = SAMPLE_RATE as usize * 60 * 10;

const PROBE_WINDOW: Duration = Duration::from_millis(700);
/// A candidate must beat the current device by this factor to take over mid-recording.
const SWITCH_THRESHOLD: f32 = 3.0;
/// Below this peak level a probe counts as silence, so noise floors never win.
const SILENCE_FLOOR: f32 = 0.004;

fn device_name(d: &Device) -> Option<String> {
    d.description().ok().map(|x| x.name().to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    pub name: String,
    /// `false` for Bluetooth, iPhone (Continuity) and other non-wired devices: opening them has side effects, so
    /// unless the user opts in they are only used when selected, even while the system makes them the default input.
    pub auto_default: bool,
}

pub type AutoOverrides = BTreeMap<String, bool>;

impl InputDevice {
    pub fn auto(&self, overrides: &AutoOverrides) -> bool {
        overrides.get(&self.name).copied().unwrap_or(self.auto_default)
    }
}

/// One enumeration pass. Reads device properties only; opens nothing.
fn connected_inputs() -> Vec<(Device, String)> {
    let Ok(devices) = cpal::default_host().input_devices() else { return Vec::new() };
    devices.filter_map(|d| device_name(&d).map(|name| (d, name))).collect()
}

fn input_device(device: &Device, name: &str) -> InputDevice {
    let transport = device.id().ok().and_then(|id| transport::transport_of(id.id()));
    InputDevice { name: name.to_string(), auto_default: transport::allows_auto_probe(transport) }
}

fn take_named(inputs: Vec<(Device, String)>, name: &str) -> Option<Device> {
    inputs.into_iter().find(|(_, n)| n == name).map(|(device, _)| device)
}

pub fn list_input_devices() -> Vec<InputDevice> {
    connected_inputs().iter().map(|(device, name)| input_device(device, name)).collect()
}

/// An internal microphone needs no notice bubble.
fn is_builtin_name(name: &str) -> bool {
    let n = name.to_lowercase();
    ["macbook", "imac", "built-in", "builtin", "built in", "内蔵", "内置"].iter().any(|k| n.contains(k))
}

pub(crate) fn device_error(error: impl std::fmt::Display) -> RecorderError {
    RecorderError::Device(error.to_string())
}

fn find_device(name: &str) -> Result<Device, RecorderError> {
    let host = cpal::default_host();
    let mut it = host.input_devices().map_err(device_error)?;
    it.find(|d| device_name(d).as_deref() == Some(name)).ok_or(RecorderError::NoInputDevice)
}

fn default_input_name() -> Option<String> {
    cpal::default_host().default_input_device().and_then(|d| device_name(&d))
}

/// The device a recording opens. `configured` (empty means automatic) wins while it is connected; otherwise the
/// system default is used only when automatic picking allows it, then a built-in microphone, then any other allowed
/// device. Nothing is returned rather than open a device the user kept out of automatic picking.
fn pick_input(
    configured: &str,
    default: Option<&str>,
    devices: &[InputDevice],
    overrides: &AutoOverrides,
) -> Option<String> {
    if !configured.trim().is_empty() && devices.iter().any(|d| d.name == configured) {
        return Some(configured.to_string());
    }
    let allowed: Vec<&InputDevice> = devices.iter().filter(|d| d.auto(overrides)).collect();
    allowed
        .iter()
        .find(|d| Some(d.name.as_str()) == default)
        .or_else(|| allowed.iter().find(|d| is_builtin_name(&d.name)))
        .or_else(|| allowed.first())
        .map(|d| d.name.clone())
}

/// A sticky pick survives only while its device stays connected and automatic picking still allows it.
fn sticky_still_allowed(sticky: &str, devices: &[InputDevice], overrides: &AutoOverrides) -> bool {
    devices.iter().any(|d| d.name == sticky && d.auto(overrides))
}

/// Applies [`pick_input`] to one enumeration and hands back the picked device with its name, so the caller opens it
/// without enumerating again.
fn resolve_among(
    inputs: Vec<(Device, String)>,
    configured: &str,
    default: Option<&str>,
    overrides: &AutoOverrides,
) -> Result<(Device, String), RecorderError> {
    if inputs.is_empty() {
        return Err(RecorderError::NoInputDevice);
    }
    let devices: Vec<InputDevice> = inputs.iter().map(|(device, name)| input_device(device, name)).collect();
    let name = pick_input(configured, default, &devices, overrides).ok_or(RecorderError::NoAutoInputDevice)?;
    let device = take_named(inputs, &name).ok_or(RecorderError::NoInputDevice)?;
    Ok((device, name))
}

pub(crate) fn resolve_input(configured: &str, overrides: &AutoOverrides) -> Result<Device, RecorderError> {
    resolve_among(connected_inputs(), configured, default_input_name().as_deref(), overrides).map(|(device, _)| device)
}

fn pick_switch(current: f32, probes: &[(String, f32)]) -> Option<&str> {
    let audible = |l: f32| l > SILENCE_FLOOR;
    let best = probes.iter().filter(|(_, l)| audible(*l)).max_by(|a, b| a.1.total_cmp(&b.1))?;
    if !audible(current) {
        // The current device hears nothing; any audible device is an upgrade.
        return Some(best.0.as_str());
    }
    (best.1 > current * SWITCH_THRESHOLD).then_some(best.0.as_str())
}

/// One recording, shared with the probes it launched: they close with it and never act on a later recording.
#[derive(Default)]
struct Recording {
    over: Mutex<bool>,
    changed: Condvar,
}

impl Recording {
    fn end(&self) {
        *self.over.lock().expect("audio state lock") = true;
        self.changed.notify_all();
    }

    fn is_over(&self) -> bool {
        *self.over.lock().expect("audio state lock")
    }

    /// Waits for `timeout`, or less when the recording ends first; `true` once it has ended.
    fn ended_within(&self, timeout: Duration) -> bool {
        let over = self.over.lock().expect("audio state lock");
        *self.changed.wait_timeout_while(over, timeout, |over| !*over).expect("audio state lock").0
    }
}

fn probe_one(name: String, window: Duration, recording: &Recording) -> (String, f32) {
    if recording.is_over() {
        return (name, 0.0);
    }
    let peak = Arc::new(Mutex::new(0f32));
    let p2 = peak.clone();
    let build = || -> Result<cpal::Stream, RecorderError> {
        let device = find_device(&name)?;
        let stream = open_mono_stream(&device, move |mono| {
            if mono.is_empty() {
                return;
            }
            let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt();
            let mut p = p2.lock().expect("audio state lock");
            if rms > *p {
                *p = rms;
            }
        })?;
        stream.play().map_err(device_error)?;
        Ok(stream)
    };
    if let Ok(stream) = build() {
        recording.ended_within(window);
        drop(stream);
    }
    (name, *peak.lock().expect("audio state lock"))
}

fn probe_levels(
    skip: &str,
    window: Duration,
    overrides: &AutoOverrides,
    recording: &Arc<Recording>,
) -> Vec<(String, f32)> {
    let names: Vec<String> =
        list_input_devices().into_iter().filter(|d| d.auto(overrides) && d.name != skip).map(|d| d.name).collect();
    let handles: Vec<JoinHandle<_>> = names
        .into_iter()
        .map(|n| {
            let recording = recording.clone();
            std::thread::Builder::new()
                .name("dito-probe".into())
                .spawn(move || probe_one(n, window, &recording))
                .expect("spawn probe thread")
        })
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

/// The returned stream is not started.
fn open_mono_stream<F>(device: &Device, on_mono: F) -> Result<cpal::Stream, RecorderError>
where
    F: FnMut(&[f32]) + Send + 'static,
{
    let config = device.default_input_config().map_err(device_error)?;
    open_mono_stream_with(device, config, on_mono)
}

/// An input stream in `config`. For an output device, Core Audio records what it plays through a process tap. Stream
/// errors are only logged; [`open_mono_stream_reporting`] also hands them over.
pub(crate) fn open_mono_stream_with<F>(
    device: &Device,
    config: cpal::SupportedStreamConfig,
    on_mono: F,
) -> Result<cpal::Stream, RecorderError>
where
    F: FnMut(&[f32]) + Send + 'static,
{
    open_mono_stream_reporting(device, config, on_mono, |_| {})
}

pub(crate) fn open_mono_stream_reporting<F, E>(
    device: &Device,
    config: cpal::SupportedStreamConfig,
    on_mono: F,
    on_error: E,
) -> Result<cpal::Stream, RecorderError>
where
    F: FnMut(&[f32]) + Send + 'static,
    E: FnMut(cpal::Error) + Send + 'static,
{
    match config.sample_format() {
        SampleFormat::F32 => build_stream::<f32, F, E>(device, config, on_mono, on_error),
        SampleFormat::I16 => build_stream::<i16, F, E>(device, config, on_mono, on_error),
        SampleFormat::I32 => build_stream::<i32, F, E>(device, config, on_mono, on_error),
        other => Err(RecorderError::UnsupportedSampleFormat(other.to_string())),
    }
}

/// Whether the error ended the stream: Core Audio pauses it when its device goes away or changes, while an overload
/// only drops a buffer.
pub(crate) fn ends_stream(error: &cpal::Error) -> bool {
    matches!(
        error.kind(),
        cpal::ErrorKind::DeviceNotAvailable | cpal::ErrorKind::DeviceChanged | cpal::ErrorKind::StreamInvalidated
    )
}

fn build_stream<T, F, E>(
    device: &Device,
    config: cpal::SupportedStreamConfig,
    mut on_mono: F,
    mut on_error: E,
) -> Result<cpal::Stream, RecorderError>
where
    T: cpal::SizedSample + 'static,
    f32: cpal::FromSample<T>,
    F: FnMut(&[f32]) + Send + 'static,
    E: FnMut(cpal::Error) + Send + 'static,
{
    let channels = config.channels() as usize;
    let mut mono = Vec::new();
    device
        .build_input_stream(
            config.into(),
            move |data: &[T], _: &_| {
                downmix_into(data, channels, &mut mono);
                on_mono(&mono);
            },
            move |e: cpal::Error| {
                tracing::warn!("audio stream error: {e}");
                on_error(e);
            },
            None,
        )
        .map_err(device_error)
}

struct Running {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
    samples: Arc<Mutex<Vec<f32>>>,
    recording: Arc<Recording>,
}

/// Takes the stream only while it still belongs to `recording`: by the time a probe has decided, that recording may
/// have ended and the next one started.
fn take_recording(running: &mut Option<Running>, recording: &Arc<Recording>) -> Option<Running> {
    running.take_if(|r| Arc::ptr_eq(&r.recording, recording))
}

struct Inner {
    /// Empty means automatic.
    device: Mutex<String>,
    auto_overrides: Mutex<AutoOverrides>,
    running: Mutex<Option<Running>>,
    level: Arc<AtomicU32>,
    sticky: Mutex<Option<String>>,
    /// Samples rescued from a recording whose stream could not be restarted after a switch.
    salvage: Mutex<Option<Arc<Mutex<Vec<f32>>>>>,
    notice: Mutex<Option<(String, Instant)>>,
}

impl Inner {
    fn set_notice_if_notable(&self, effective: &str, default_name: Option<&str>) {
        let notable = !is_builtin_name(effective) && default_name != Some(effective);
        if notable {
            *self.notice.lock().expect("audio state lock") = Some((effective.to_string(), Instant::now()));
        }
    }

    /// `selector` names a connected device; `device` is that device when the caller already holds it, otherwise it is
    /// looked up by name on the stream thread. Blocks until the stream is live.
    fn spawn_stream(
        &self,
        selector: &str,
        device: Option<Device>,
        samples: Arc<Mutex<Vec<f32>>>,
        recording: Arc<Recording>,
    ) -> Result<(Running, String), RecorderError> {
        self.level.store(0, Ordering::Relaxed);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (s2, l2) = (samples.clone(), self.level.clone());
        let sel = selector.to_string();
        let thread = std::thread::Builder::new()
            .name("dito-audio".into())
            .spawn(move || stream_thread(sel, device, s2, l2, ready_tx, stop_rx))?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(name)) => Ok((Running { stop: stop_tx, thread, samples, recording }, name)),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err(RecorderError::OpenTimeout),
        }
    }

    /// Returns the effective device and its recording only when automatic picking should run for it.
    fn begin(&self) -> Result<Option<(String, Arc<Recording>)>, RecorderError> {
        let mut running = self.running.lock().expect("audio state lock");
        if running.is_some() {
            return Ok(None);
        }
        *self.salvage.lock().expect("audio state lock") = None;
        let configured = self.device.lock().expect("audio state lock").clone();
        let auto = configured.trim().is_empty();
        // The only enumeration of this start: every pass costs many Core Audio property reads while the shortcut
        // waits for the stream.
        let inputs = connected_inputs();
        let sticky = {
            let mut s = self.sticky.lock().expect("audio state lock");
            if let Some(n) = s.as_deref() {
                // Only the devices of that name are described: each description costs Core Audio property reads.
                let named: Vec<InputDevice> = inputs
                    .iter()
                    .filter(|(_, name)| name == n)
                    .map(|(device, name)| input_device(device, name))
                    .collect();
                let overrides = self.auto_overrides.lock().expect("audio state lock");
                if !sticky_still_allowed(n, &named, &overrides) {
                    *s = None;
                }
            }
            s.clone()
        };
        let default_name = default_input_name();
        let (device, selector) = match sticky.clone().filter(|_| auto) {
            Some(s) => (take_named(inputs, &s), s),
            None => {
                let overrides = self.auto_overrides.lock().expect("audio state lock").clone();
                let (device, name) = resolve_among(inputs, &configured, default_name.as_deref(), &overrides)?;
                (Some(device), name)
            }
        };
        let samples = Arc::new(Mutex::new(Vec::new()));
        let recording = Arc::new(Recording::default());
        let (r, effective) = self.spawn_stream(&selector, device, samples, recording.clone())?;
        if effective != configured {
            self.set_notice_if_notable(&effective, default_name.as_deref());
        }
        *running = Some(r);
        Ok((auto && sticky.is_none()).then_some((effective, recording)))
    }

    fn auto_probe(self: &Arc<Self>, current: String, recording: Arc<Recording>) {
        let level = self.level.clone();
        let probes = {
            let current = current.clone();
            let overrides = self.auto_overrides.lock().expect("audio state lock").clone();
            let recording = recording.clone();
            std::thread::spawn(move || probe_levels(&current, PROBE_WINDOW, &overrides, &recording))
        };
        let deadline = Instant::now() + PROBE_WINDOW;
        let mut current_peak = 0f32;
        while Instant::now() < deadline {
            current_peak = current_peak.max(f32::from_bits(level.load(Ordering::Relaxed)));
            if recording.ended_within(Duration::from_millis(40)) {
                return;
            }
        }
        let probes = probes.join().unwrap_or_default();
        tracing::debug!("auto mic: current peak {current_peak:.4}, probes {probes:?}");
        let Some(winner) = pick_switch(current_peak, &probes) else { return };

        let mut running = self.running.lock().expect("audio state lock");
        // Strict switch: stop the old stream, then start the winner appending into the same
        // buffer, so the two sources never interleave.
        let Some(old) = take_recording(&mut running, &recording) else { return };
        tracing::info!("auto mic: switching from {current} to {winner}");
        let recorded = old.samples.clone();
        let _ = old.stop.send(());
        let _ = old.thread.join();
        match self.spawn_stream(winner, None, old.samples, recording.clone()) {
            Ok((r, effective)) => {
                tracing::info!("auto mic: live on {effective}");
                *self.sticky.lock().expect("audio state lock") = Some(effective.clone());
                self.set_notice_if_notable(&effective, default_input_name().as_deref());
                *running = Some(r);
            }
            Err(_) => match self.spawn_stream(&current, None, recorded.clone(), recording) {
                // The winner refused to open; fall back to re-opening the current device.
                Ok((r, _)) => *running = Some(r),
                Err(_) => *self.salvage.lock().expect("audio state lock") = Some(recorded),
            },
        }
    }

    fn stop(&self) -> AudioClip {
        let samples_arc = match self.running.lock().expect("audio state lock").take() {
            Some(r) => {
                r.recording.end();
                let _ = r.stop.send(());
                let _ = r.thread.join();
                r.samples
            }
            None => match self.salvage.lock().expect("audio state lock").take() {
                Some(s) => s,
                None => return AudioClip::default(),
            },
        };
        self.level.store(0, Ordering::Relaxed);
        let samples = std::mem::take(&mut *samples_arc.lock().expect("audio state lock"));
        AudioClip::new(samples)
    }
}

/// The cpal stream lives on its own thread (streams are not `Send`).
pub struct Capture(Arc<Inner>);

impl Default for Capture {
    fn default() -> Self {
        Self::new("")
    }
}

impl Capture {
    /// `device` is a device name; empty selects the system default.
    pub fn new(device: &str) -> Self {
        Self(Arc::new(Inner {
            device: Mutex::new(device.to_string()),
            auto_overrides: Mutex::new(AutoOverrides::new()),
            running: Mutex::new(None),
            level: Arc::new(AtomicU32::new(0)),
            sticky: Mutex::new(None),
            salvage: Mutex::new(None),
            notice: Mutex::new(None),
        }))
    }

    pub fn set_device(&self, device: &str) {
        *self.0.device.lock().expect("audio state lock") = device.to_string();
    }

    /// Takes effect from the next recording.
    pub fn set_auto_overrides(&self, overrides: &AutoOverrides) {
        *self.0.auto_overrides.lock().expect("audio state lock") = overrides.clone();
    }

    pub fn device_notice(&self) -> Option<(String, Instant)> {
        self.0.notice.lock().expect("audio state lock").clone()
    }
}

impl Recorder for Capture {
    fn start(&self) -> Result<(), RecorderError> {
        if let Some((effective, recording)) = self.0.begin()? {
            let inner = self.0.clone();
            std::thread::Builder::new()
                .name("dito-auto-probe".into())
                .spawn(move || inner.auto_probe(effective, recording))?;
        }
        Ok(())
    }

    fn stop(&self) -> AudioClip {
        self.0.stop()
    }

    fn level(&self) -> f32 {
        f32::from_bits(self.0.level.load(Ordering::Relaxed))
    }
}

fn downmix_into<T: Sample>(data: &[T], channels: usize, mono: &mut Vec<f32>)
where
    f32: cpal::FromSample<T>,
{
    mono.clear();
    for frame in data.chunks(channels) {
        let sum: f32 = frame.iter().map(|s| s.to_sample::<f32>()).sum();
        mono.push(sum / frame.len() as f32);
    }
}

fn stream_thread(
    device_name: String,
    device: Option<Device>,
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    ready: mpsc::Sender<Result<String, RecorderError>>,
    stop: mpsc::Receiver<()>,
) {
    let build = || -> Result<(cpal::Stream, String), RecorderError> {
        let device = match device {
            Some(device) => device,
            None => find_device(&device_name)?,
        };
        let config = device.default_input_config().map_err(device_error)?;
        let mut resampler = Resampler::new(config.sample_rate(), SAMPLE_RATE);
        let mut out = Vec::new();
        let handle = move |mono: &[f32]| {
            out.clear();
            resampler.push(mono, &mut out);
            let rms = if mono.is_empty() {
                0.0
            } else {
                (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt()
            };
            let prev = f32::from_bits(level.load(Ordering::Relaxed));
            // Fast attack, slow release reads well as a waveform height.
            let next = if rms > prev { rms } else { prev * 0.85 + rms * 0.15 };
            level.store(next.to_bits(), Ordering::Relaxed);
            let mut s = samples.lock().expect("audio state lock");
            if s.len() < MAX_SAMPLES {
                s.extend_from_slice(&out);
            }
        };
        let stream = open_mono_stream_with(&device, config, handle)?;
        stream.play().map_err(device_error)?;
        Ok((stream, device_name.clone()))
    };

    match build() {
        Ok((stream, name)) => {
            let _ = ready.send(Ok(name));
            let _ = stop.recv();
            drop(stream);
        }
        Err(e) => {
            let _ = ready.send(Err(e));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe(name: &str, level: f32) -> (String, f32) {
        (name.to_string(), level)
    }

    fn device(name: &str, auto_default: bool) -> InputDevice {
        InputDevice { name: name.to_string(), auto_default }
    }

    #[test]
    fn a_default_kept_out_of_automatic_picking_is_skipped() {
        let devices = [device("AirPods", false), device("C922", true), device("MacBook Pro Microphone", true)];
        let none = AutoOverrides::new();
        assert_eq!(pick_input("", Some("C922"), &devices, &none).as_deref(), Some("C922"));
        assert_eq!(pick_input("", Some("AirPods"), &devices, &none).as_deref(), Some("MacBook Pro Microphone"));
        let opted_in = AutoOverrides::from([("AirPods".to_string(), true)]);
        assert_eq!(pick_input("", Some("AirPods"), &devices, &opted_in).as_deref(), Some("AirPods"));
    }

    #[test]
    fn only_devices_kept_out_of_automatic_picking_open_nothing() {
        let devices = [device("AirPods", false), device("iPhone", false)];
        assert_eq!(pick_input("", Some("AirPods"), &devices, &AutoOverrides::new()), None);
        assert_eq!(pick_input("C922", Some("AirPods"), &devices, &AutoOverrides::new()), None, "C922 is unplugged");
    }

    #[test]
    fn a_chosen_device_opens_even_when_kept_out_of_automatic_picking() {
        let devices = [device("AirPods", false), device("MacBook Pro Microphone", true)];
        assert_eq!(pick_input("AirPods", None, &devices, &AutoOverrides::new()).as_deref(), Some("AirPods"));
    }

    #[test]
    fn a_clearly_louder_device_takes_over() {
        let probes = [probe("C922", 0.3), probe("MacBook Pro Microphone", 0.02)];
        assert_eq!(pick_switch(0.05, &probes), Some("C922"));
    }

    #[test]
    fn a_merely_louder_device_does_not() {
        let probes = [probe("C922", 0.12)];
        assert_eq!(pick_switch(0.05, &probes), None, "0.12 < 3×0.05");
    }

    #[test]
    fn a_silent_current_yields_to_any_audible_device() {
        let probes = [probe("C922", 0.01)];
        assert_eq!(pick_switch(0.0, &probes), Some("C922"));
    }

    #[test]
    fn all_silent_stays_put() {
        let probes = [probe("C922", 0.001)];
        assert_eq!(pick_switch(0.0, &probes), None);
    }

    #[test]
    fn a_probe_of_an_ended_recording_leaves_the_next_one_alone() {
        let stream_of = |recording: &Arc<Recording>| Running {
            stop: mpsc::channel().0,
            thread: std::thread::spawn(|| {}),
            samples: Arc::default(),
            recording: recording.clone(),
        };
        let capture = Capture::new("");
        let slot = || capture.0.running.lock().expect("audio state lock");
        let first = Arc::new(Recording::default());
        *slot() = Some(stream_of(&first));
        assert!(!first.is_over());

        capture.stop();
        assert!(first.ended_within(Duration::from_secs(5)), "its probes wake at once instead of finishing the window");
        let second = Arc::new(Recording::default());
        *slot() = Some(stream_of(&second));

        assert!(take_recording(&mut slot(), &first).is_none());
        assert!(slot().is_some(), "the next recording keeps its stream");
        assert!(take_recording(&mut slot(), &second).is_some(), "a probe still switches its own recording");
    }

    #[test]
    fn user_choice_overrides_the_connection_default() {
        let wired = InputDevice { name: "C922".into(), auto_default: true };
        let headset = InputDevice { name: "AirPods".into(), auto_default: false };
        let none = AutoOverrides::new();
        assert!(wired.auto(&none));
        assert!(!headset.auto(&none));
        let chosen = AutoOverrides::from([("AirPods".to_string(), true), ("C922".to_string(), false)]);
        assert!(headset.auto(&chosen));
        assert!(!wired.auto(&chosen));
    }

    #[test]
    fn a_sticky_pick_ends_once_unplugged_or_kept_out_of_automatic_picking() {
        let devices = [device("C922", true), device("AirPods", false)];
        let none = AutoOverrides::new();
        assert!(sticky_still_allowed("C922", &devices, &none));
        assert!(!sticky_still_allowed("AirPods", &devices, &none), "its opt-in was withdrawn");
        assert!(!sticky_still_allowed("Yeti", &devices, &none), "unplugged");
        let chosen = AutoOverrides::from([("AirPods".to_string(), true), ("C922".to_string(), false)]);
        assert!(!sticky_still_allowed("C922", &devices, &chosen), "the user turned it off");
        assert!(sticky_still_allowed("AirPods", &devices, &chosen));
    }
}
