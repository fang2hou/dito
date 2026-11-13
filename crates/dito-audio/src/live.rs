//! Streaming 16 kHz mono input for transcription: the Mac's output through a system audio tap, or a microphone.

use std::sync::mpsc;
use std::thread::JoinHandle;
use std::time::Duration;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use dito_core::audio::SAMPLE_RATE;
use dito_core::controller::RecorderError;
use dito_core::transcription::{AudioSource, ChunkSink, LiveAudio, LiveInput};

use crate::capture::{device_error, find_device, open_mono_stream_with};
use crate::resample::Resampler;

const OPEN_TIMEOUT: Duration = Duration::from_secs(5);
/// Creating the system audio tap waits while macOS asks for System Audio Recording access, so the user has time to
/// answer.
const TAP_OPEN_TIMEOUT: Duration = Duration::from_secs(120);

/// `microphone` names the input device to use, empty for the system default; it is read at every open.
pub struct LiveCapture {
    microphone: Box<dyn Fn() -> String + Send + Sync>,
}

impl LiveCapture {
    pub fn new(microphone: impl Fn() -> String + Send + Sync + 'static) -> Self {
        Self { microphone: Box::new(microphone) }
    }
}

/// The stream lives on its own thread; dropping this stops it and waits, so the chunk callback is gone by then.
struct LiveStream {
    stop: Option<mpsc::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl LiveInput for LiveStream {}

impl Drop for LiveStream {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl LiveAudio for LiveCapture {
    fn open(&self, source: AudioSource, mut on_chunk: ChunkSink) -> Result<Box<dyn LiveInput>, RecorderError> {
        let microphone = (self.microphone)();
        let (stop_tx, stop_rx) = mpsc::channel::<()>();
        let (ready_tx, ready_rx) = mpsc::channel::<Result<(), RecorderError>>();
        let thread = std::thread::Builder::new().name("dito-live-audio".into()).spawn(move || {
            let build = move || -> Result<cpal::Stream, RecorderError> {
                let (device, config) = match source {
                    AudioSource::System => {
                        // Loopback needs macOS 14.6; the tap is created when the stream is built.
                        let device =
                            cpal::default_host().default_output_device().ok_or(RecorderError::NoInputDevice)?;
                        let config = device.default_output_config().map_err(device_error)?;
                        (device, config)
                    }
                    AudioSource::Microphone => {
                        let (device, _) = find_device(&microphone)?;
                        let config = device.default_input_config().map_err(device_error)?;
                        (device, config)
                    }
                };
                let mut resampler = Resampler::new(config.sample_rate(), SAMPLE_RATE);
                let mut out = Vec::new();
                let stream = open_mono_stream_with(&device, config, move |mono| {
                    out.clear();
                    resampler.push(mono, &mut out);
                    if !out.is_empty() {
                        on_chunk(&out);
                    }
                })?;
                stream.play().map_err(device_error)?;
                Ok(stream)
            };
            match build() {
                Ok(stream) => {
                    let _ = ready_tx.send(Ok(()));
                    let _ = stop_rx.recv();
                    drop(stream);
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e));
                }
            }
        })?;
        let mut live = LiveStream { stop: Some(stop_tx), thread: Some(thread) };
        let timeout = if source == AudioSource::System { TAP_OPEN_TIMEOUT } else { OPEN_TIMEOUT };
        match ready_rx.recv_timeout(timeout) {
            Ok(Ok(())) => Ok(Box::new(live)),
            Ok(Err(e)) => Err(e),
            Err(_) => {
                // A stuck open is abandoned rather than joined.
                live.thread.take();
                Err(RecorderError::OpenTimeout)
            }
        }
    }
}
