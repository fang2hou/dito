//! Silero VAD through sherpa-onnx: one detector per audio source of a transcription session.

use std::collections::VecDeque;
use std::ffi::CString;
use std::path::{Path, PathBuf};
use std::ptr::NonNull;

use dito_core::transcription::{Speech, VoiceDetector, VoiceDetectors};
use sherpa_onnx_sys::vad as ffi;

const RATE: i32 = 16_000;
/// Silero works on 512-sample windows at 16 kHz.
const WINDOW: usize = 512;

/// Creates detectors from `silero_vad.onnx`; the model is checked at every creation, so a deleted file fails the
/// session start instead of the app.
pub struct SileroDetectors {
    model: PathBuf,
}

impl SileroDetectors {
    pub fn new(model: &Path) -> Self {
        Self { model: model.to_path_buf() }
    }
}

impl VoiceDetectors for SileroDetectors {
    fn create(&self) -> Result<Box<dyn VoiceDetector>, String> {
        if !self.model.is_file() {
            return Err(format!("voice activity model missing: {}", self.model.display()));
        }
        let model = CString::new(self.model.to_string_lossy().as_bytes()).map_err(|e| e.to_string())?;
        Ok(Box::new(SileroVad::new(&model)?))
    }
}

struct SileroVad {
    detector: NonNull<ffi::VoiceActivityDetector>,
    pending: Vec<f32>,
    ready: VecDeque<Speech>,
}

// SAFETY: the native detector has no thread affinity; it is owned and used by one thread at a time.
unsafe impl Send for SileroVad {}

impl SileroVad {
    fn new(model: &CString) -> Result<Self, String> {
        let silero = ffi::SileroVadModelConfig {
            model: model.as_ptr(),
            threshold: 0.5,
            min_silence_duration: 0.5,
            min_speech_duration: 0.25,
            window_size: WINDOW as i32,
            max_speech_duration: 20.0,
        };
        let config = ffi::VadModelConfig {
            silero_vad: silero,
            sample_rate: RATE,
            num_threads: 1,
            provider: c"cpu".as_ptr(),
            debug: 0,
            ten_vad: ffi::TenVadModelConfig {
                model: c"".as_ptr(),
                threshold: 0.5,
                min_silence_duration: 0.5,
                min_speech_duration: 0.25,
                window_size: 256,
                max_speech_duration: 20.0,
            },
        };
        // SAFETY: every string in the config outlives this call, which copies the config.
        let ptr = unsafe { ffi::SherpaOnnxCreateVoiceActivityDetector(&config, 60.0) };
        let detector = NonNull::new(ptr.cast_mut()).ok_or("could not create the voice activity detector")?;
        Ok(Self { detector, pending: Vec::new(), ready: VecDeque::new() })
    }

    fn collect(&mut self) {
        let p = self.detector.as_ptr();
        // SAFETY: the detector is live; each front segment is copied out and freed before it is popped.
        unsafe {
            while ffi::SherpaOnnxVoiceActivityDetectorEmpty(p) == 0 {
                let segment = ffi::SherpaOnnxVoiceActivityDetectorFront(p);
                if let Some(s) = segment.as_ref() {
                    let samples = std::slice::from_raw_parts(s.samples, s.n.max(0) as usize).to_vec();
                    self.ready.push_back(Speech { start_sample: s.start.max(0) as u64, samples });
                    ffi::SherpaOnnxDestroySpeechSegment(segment);
                }
                ffi::SherpaOnnxVoiceActivityDetectorPop(p);
            }
        }
    }
}

impl VoiceDetector for SileroVad {
    fn accept(&mut self, samples: &[f32]) {
        self.pending.extend_from_slice(samples);
        let whole = self.pending.len() / WINDOW * WINDOW;
        for window in self.pending[..whole].chunks(WINDOW) {
            // SAFETY: the detector is live and copies the window before returning.
            unsafe {
                ffi::SherpaOnnxVoiceActivityDetectorAcceptWaveform(
                    self.detector.as_ptr(),
                    window.as_ptr(),
                    WINDOW as i32,
                )
            };
        }
        self.pending.drain(..whole);
        self.collect();
    }

    fn flush(&mut self) {
        // SAFETY: the detector is live.
        unsafe { ffi::SherpaOnnxVoiceActivityDetectorFlush(self.detector.as_ptr()) };
        self.collect();
    }

    fn pop(&mut self) -> Option<Speech> {
        self.ready.pop_front()
    }

    fn speaking(&self) -> bool {
        // SAFETY: the detector is live.
        unsafe { ffi::SherpaOnnxVoiceActivityDetectorDetected(self.detector.as_ptr()) != 0 }
    }
}

impl Drop for SileroVad {
    fn drop(&mut self) {
        // SAFETY: the detector is live and destroyed once.
        unsafe { ffi::SherpaOnnxDestroyVoiceActivityDetector(self.detector.as_ptr()) }
    }
}
