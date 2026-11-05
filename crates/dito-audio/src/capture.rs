use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Device, Sample, SampleFormat};
use dito_core::audio::{AudioClip, SAMPLE_RATE};
use dito_core::controller::Recorder;

use crate::resample::Resampler;
use crate::transport;

/// Hard cap so a forgotten recording cannot eat memory (10 minutes).
const MAX_SAMPLES: usize = SAMPLE_RATE as usize * 60 * 10;

/// How long automatic device picking listens to each candidate.
const PROBE_WINDOW: Duration = Duration::from_millis(700);
/// A candidate must beat the current device by this factor to take over mid-recording.
pub const SWITCH_THRESHOLD: f32 = 3.0;
/// Below this peak level a probe counts as silence, so noise floors never win.
const SILENCE_FLOOR: f32 = 0.004;

fn device_name(d: &Device) -> Option<String> {
    d.description().ok().map(|x| x.name().to_string())
}

/// An available input device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputDevice {
    pub name: String,
    /// Whether automatic picking may open this device by default. `false` for Bluetooth, iPhone
    /// (Continuity) and other non-wired devices: opening them has side effects, so unless the
    /// user opts in they are only used when selected (or made the default input by the system).
    pub auto_default: bool,
}

/// Per-device user choices (by name) overriding [`InputDevice::auto_default`].
pub type AutoOverrides = BTreeMap<String, bool>;

impl InputDevice {
    /// Whether automatic picking may open this device, given the user's choices.
    pub fn auto(&self, overrides: &AutoOverrides) -> bool {
        overrides.get(&self.name).copied().unwrap_or(self.auto_default)
    }
}

/// Names of the available input devices. Reads device properties only; opens nothing.
fn input_names() -> Vec<String> {
    let host = cpal::default_host();
    host.input_devices().map(|it| it.filter_map(|d| device_name(&d)).collect()).unwrap_or_default()
}

/// The available input devices, with whether automatic picking may use them.
pub fn list_input_devices() -> Vec<InputDevice> {
    let host = cpal::default_host();
    let Ok(devices) = host.input_devices() else { return Vec::new() };
    devices
        .filter_map(|d| {
            let name = device_name(&d)?;
            let transport = d.id().ok().and_then(|id| transport::transport_of(id.id()));
            Some(InputDevice { name, auto_default: transport::allows_auto_probe(transport) })
        })
        .collect()
}

/// Whether a device name looks like an internal microphone (no bubble worth showing).
pub fn is_builtin_name(name: &str) -> bool {
    let n = name.to_lowercase();
    ["macbook", "imac", "built-in", "builtin", "built in", "内蔵", "内置"].iter().any(|k| n.contains(k))
}

fn find_device(name: &str) -> Result<(Device, String), String> {
    let host = cpal::default_host();
    if name.trim().is_empty() {
        let d = host.default_input_device().ok_or("no input device")?;
        let n = device_name(&d).unwrap_or_default();
        return Ok((d, n));
    }
    let mut it = host.input_devices().map_err(|e| e.to_string())?;
    let found = it.find(|d| device_name(d).as_deref() == Some(name));
    match found {
        Some(d) => Ok((d, name.to_string())),
        None => {
            let d = host.default_input_device().ok_or("no input device")?;
            let n = device_name(&d).unwrap_or_default();
            Ok((d, n))
        }
    }
}

/// Which probe result clearly beats the current device's level, if any. Pure.
pub fn pick_switch(current: f32, probes: &[(String, f32)]) -> Option<&str> {
    let audible = |l: f32| l > SILENCE_FLOOR;
    let best = probes.iter().filter(|(_, l)| audible(*l)).max_by(|a, b| a.1.total_cmp(&b.1))?;
    if !audible(current) {
        // The current device hears nothing; any audible device is an upgrade.
        return Some(best.0.as_str());
    }
    (best.1 > current * SWITCH_THRESHOLD).then_some(best.0.as_str())
}

/// Peak chunk-RMS of one device over `window`, measured on this thread.
fn probe_one(name: String, window: Duration) -> (String, f32) {
    let peak = Arc::new(Mutex::new(0f32));
    let p2 = peak.clone();
    let build = || -> Result<cpal::Stream, String> {
        let (device, _) = find_device(&name)?;
        let stream = open_mono_stream(&device, move |mono| {
            if mono.is_empty() {
                return;
            }
            let rms = (mono.iter().map(|s| s * s).sum::<f32>() / mono.len() as f32).sqrt();
            let mut p = p2.lock().unwrap();
            if rms > *p {
                *p = rms;
            }
        })?;
        stream.play().map_err(|e| e.to_string())?;
        Ok(stream)
    };
    if let Ok(stream) = build() {
        std::thread::sleep(window);
        drop(stream);
    }
    (name, *peak.lock().unwrap())
}

/// Peak chunk-RMS of every auto-eligible input device except `skip`, each measured in parallel.
fn probe_levels(skip: &str, window: Duration, overrides: &AutoOverrides) -> Vec<(String, f32)> {
    let names: Vec<String> =
        list_input_devices().into_iter().filter(|d| d.auto(overrides) && d.name != skip).map(|d| d.name).collect();
    let handles: Vec<JoinHandle<_>> = names
        .into_iter()
        .map(|n| {
            std::thread::Builder::new()
                .name("dito-probe".into())
                .spawn(move || probe_one(n, window))
                .expect("spawn probe thread")
        })
        .collect();
    handles.into_iter().filter_map(|h| h.join().ok()).collect()
}

/// Opens a mono f32 input stream for `device`, resampled by the caller's callback concerns.
/// The callback receives one downmixed chunk per host callback; the stream is not started.
fn open_mono_stream<F>(device: &Device, on_mono: F) -> Result<cpal::Stream, String>
where
    F: FnMut(&[f32]) + Send + 'static,
{
    let config = device.default_input_config().map_err(|e| e.to_string())?;
    let channels = config.channels() as usize;
    let err_fn = |e: cpal::Error| tracing::warn!("audio stream error: {e}");
    let mut cb = Some(on_mono);
    let mut mono = Vec::new();
    match config.sample_format() {
        SampleFormat::F32 => {
            let mut cb = cb.take().unwrap();
            device.build_input_stream(
                config.into(),
                move |d: &[f32], _: &_| {
                    downmix_into(d, channels, &mut mono);
                    cb(&mono);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I16 => {
            let mut cb = cb.take().unwrap();
            device.build_input_stream(
                config.into(),
                move |d: &[i16], _: &_| {
                    downmix_into(d, channels, &mut mono);
                    cb(&mono);
                },
                err_fn,
                None,
            )
        }
        SampleFormat::I32 => {
            let mut cb = cb.take().unwrap();
            device.build_input_stream(
                config.into(),
                move |d: &[i32], _: &_| {
                    downmix_into(d, channels, &mut mono);
                    cb(&mono);
                },
                err_fn,
                None,
            )
        }
        other => return Err(format!("unsupported sample format {other}")),
    }
    .map_err(|e| e.to_string())
}

struct Running {
    stop: mpsc::Sender<()>,
    thread: JoinHandle<()>,
    samples: Arc<Mutex<Vec<f32>>>,
}

/// State shared between the recording stream, the auto-probe thread and the app.
struct Inner {
    /// User-configured device name; empty means automatic.
    device: Mutex<String>,
    /// Which devices automatic picking may open, as chosen by the user.
    auto_overrides: Mutex<AutoOverrides>,
    running: Mutex<Option<Running>>,
    level: Arc<AtomicU32>,
    /// Last device picked automatically; reused while it stays connected.
    sticky: Mutex<Option<String>>,
    /// Samples rescued from a recording whose stream could not be restarted after a switch.
    salvage: Mutex<Option<Arc<Mutex<Vec<f32>>>>>,
    /// (device, when) the user should be told which mic is live; read by the HUD each frame.
    notice: Mutex<Option<(String, Instant)>>,
}

impl Inner {
    fn set_notice_if_notable(&self, effective: &str) {
        let default_name = cpal::default_host().default_input_device().and_then(|d| device_name(&d));
        let notable = !is_builtin_name(effective) && default_name.as_deref() != Some(effective);
        if notable {
            *self.notice.lock().unwrap() = Some((effective.to_string(), Instant::now()));
        }
    }

    /// Spawns the capture thread for `selector` (empty = system default), appending into
    /// `samples`. Blocks until the stream is live and returns the resolved device name.
    fn spawn_stream(&self, selector: &str, samples: Arc<Mutex<Vec<f32>>>) -> Result<(Running, String), String> {
        self.level.store(0, Ordering::Relaxed);
        let (stop_tx, stop_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let (s2, l2) = (samples.clone(), self.level.clone());
        let sel = selector.to_string();
        let thread = std::thread::Builder::new()
            .name("dito-audio".into())
            .spawn(move || stream_thread(sel, s2, l2, ready_tx, stop_rx))
            .map_err(|e| e.to_string())?;
        match ready_rx.recv_timeout(Duration::from_secs(5)) {
            Ok(Ok(name)) => Ok((Running { stop: stop_tx, thread, samples }, name)),
            Ok(Err(e)) => {
                let _ = thread.join();
                Err(e)
            }
            Err(_) => Err("timed out opening the input device".into()),
        }
    }

    /// Records from one input device. The cpal stream lives on its own thread (streams are not `Send`).
    /// Returns the effective device when automatic device picking should run for it.
    fn begin(&self) -> Result<Option<String>, String> {
        let mut running = self.running.lock().unwrap();
        if running.is_some() {
            return Ok(None);
        }
        *self.salvage.lock().unwrap() = None;
        let configured = self.device.lock().unwrap().clone();
        let auto = configured.trim().is_empty();
        // A sticky pick survives only while the device stays connected.
        let live = input_names();
        let sticky = {
            let mut s = self.sticky.lock().unwrap();
            if s.as_ref().is_some_and(|n| !live.contains(n)) {
                *s = None;
            }
            s.clone()
        };
        let explicit = (!auto).then(|| configured.clone());
        let inferred = auto && sticky.is_some();
        let selector = sticky.clone().or(explicit).unwrap_or_default();
        let samples = Arc::new(Mutex::new(Vec::new()));
        let (r, effective) = self.spawn_stream(&selector, samples)?;
        if inferred {
            self.set_notice_if_notable(&effective);
        }
        *running = Some(r);
        Ok((auto && sticky.is_none()).then_some(effective))
    }

    /// While the default device records, measure it and every other device in parallel; if one
    /// is clearly louder, switch the live stream over without losing the samples so far.
    fn auto_probe(self: &Arc<Self>, current: String) {
        let level = self.level.clone();
        let probes = {
            let current = current.clone();
            let overrides = self.auto_overrides.lock().unwrap().clone();
            std::thread::spawn(move || probe_levels(&current, PROBE_WINDOW, &overrides))
        };
        let deadline = Instant::now() + PROBE_WINDOW;
        let mut current_peak = 0f32;
        while Instant::now() < deadline {
            current_peak = current_peak.max(f32::from_bits(level.load(Ordering::Relaxed)));
            std::thread::sleep(Duration::from_millis(40));
        }
        let probes = probes.join().unwrap_or_default();
        tracing::debug!("auto mic: current peak {current_peak:.4}, probes {probes:?}");
        let Some(winner) = pick_switch(current_peak, &probes) else { return };
        tracing::info!("auto mic: switching from {current} to {winner}");

        let mut running = self.running.lock().unwrap();
        if running.is_none() {
            return; // the recording ended while probing
        }
        // Strict switch: stop the old stream, then start the winner appending into the same
        // buffer, so the two sources never interleave.
        let old = running.take().unwrap();
        let recorded = old.samples.clone();
        let _ = old.stop.send(());
        let _ = old.thread.join();
        match self.spawn_stream(winner, old.samples) {
            Ok((r, effective)) => {
                tracing::info!("auto mic: live on {effective}");
                *self.sticky.lock().unwrap() = Some(effective.clone());
                self.set_notice_if_notable(&effective);
                *running = Some(r);
            }
            Err(_) => match self.spawn_stream(&current, recorded.clone()) {
                // The winner refused to open; fall back to re-opening the current device.
                Ok((r, _)) => *running = Some(r),
                Err(_) => *self.salvage.lock().unwrap() = Some(recorded),
            },
        }
    }

    fn stop(&self) -> AudioClip {
        let samples_arc = match self.running.lock().unwrap().take() {
            Some(r) => {
                let _ = r.stop.send(());
                let _ = r.thread.join();
                r.samples
            }
            None => match self.salvage.lock().unwrap().take() {
                Some(s) => s,
                None => return AudioClip::default(),
            },
        };
        self.level.store(0, Ordering::Relaxed);
        let samples = std::mem::take(&mut *samples_arc.lock().unwrap());
        AudioClip::new(samples)
    }
}

/// Records from one input device at a time, picked automatically when no device is configured.
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
        *self.0.device.lock().unwrap() = device.to_string();
    }

    /// Sets which devices automatic picking may open; used from the next recording on.
    pub fn set_auto_overrides(&self, overrides: &AutoOverrides) {
        *self.0.auto_overrides.lock().unwrap() = overrides.clone();
    }

    /// The device the user should be told about, and when it became live.
    pub fn device_notice(&self) -> Option<(String, Instant)> {
        self.0.notice.lock().unwrap().clone()
    }
}

impl Recorder for Capture {
    fn start(&self) -> Result<(), String> {
        if let Some(effective) = self.0.begin()? {
            let inner = self.0.clone();
            std::thread::Builder::new()
                .name("dito-auto-probe".into())
                .spawn(move || inner.auto_probe(effective))
                .map_err(|e| e.to_string())?;
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
    samples: Arc<Mutex<Vec<f32>>>,
    level: Arc<AtomicU32>,
    ready: mpsc::Sender<Result<String, String>>,
    stop: mpsc::Receiver<()>,
) {
    let build = || -> Result<(cpal::Stream, String), String> {
        let (device, resolved) = find_device(&device_name)?;
        let rate = device.default_input_config().map_err(|e| e.to_string())?.sample_rate();
        let mut resampler = Resampler::new(rate, SAMPLE_RATE);
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
            // Fast attack, slow release: reads well as a waveform height.
            let next = if rms > prev { rms } else { prev * 0.85 + rms * 0.15 };
            level.store(next.to_bits(), Ordering::Relaxed);
            let mut s = samples.lock().unwrap();
            if s.len() < MAX_SAMPLES {
                s.extend_from_slice(&out);
            }
        };
        let stream = open_mono_stream(&device, handle)?;
        stream.play().map_err(|e| e.to_string())?;
        Ok((stream, resolved))
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
    fn ties_pick_the_loudest() {
        let probes = [probe("A", 0.2), probe("B", 0.9)];
        assert_eq!(pick_switch(0.05, &probes), Some("B"));
    }

    #[test]
    fn builtin_detection_covers_the_known_spellings() {
        assert!(is_builtin_name("MacBook Pro Microphone"));
        assert!(is_builtin_name("Built-in Microphone"));
        assert!(is_builtin_name("内蔵マイク"));
        assert!(!is_builtin_name("C922 Pro Stream Webcam"));
        assert!(!is_builtin_name("AirPods 4"));
    }
}
