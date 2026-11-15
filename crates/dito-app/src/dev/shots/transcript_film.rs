//! A transcription session played in real time through the real session actor and page, filmed four times a
//! second, to watch how text arrives, settles, closes into rows and gets polished. Opt-in:
//! `DITO_SHOTS_ONLY=transcriptfilm`. Speech is scripted tones (`ToneTranscriber`), so no model is needed.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use dito_core::config::Appearance;
use dito_core::controller::RecorderError;
use dito_core::factory::FixedEngines;
use dito_core::provider::mock::{MockTextModel, ToneTranscriber, tone};
use dito_core::transcription::{
    self, AudioSource, ChunkSink, EnergyDetectors, LiveAudio, LiveInput, SessionState, Sources, TranscriptionDeps,
};
use gpui_kit::*;

use super::capture::{render_tools_window, resize_tools_window, save};
use super::input::on_tools_window;
use crate::services;
use crate::state::AppState;
use crate::tools;

const FRAME: Duration = Duration::from_millis(250);
/// The dock morphs are filmed at display rate: the start, the stop press and the stop itself.
const DENSE_FRAME: Duration = Duration::from_millis(16);
const DENSE_FOR: Duration = Duration::from_millis(2_500);

/// Plays the script like a system audio tap: 100 ms blocks in real time, and nothing at all once it is over.
struct Talk(Vec<f32>);

struct Playing(Arc<AtomicBool>);
impl LiveInput for Playing {}
impl Drop for Playing {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

impl LiveAudio for Talk {
    fn open(&self, source: AudioSource, mut on_chunk: ChunkSink) -> Result<Box<dyn LiveInput>, RecorderError> {
        if source != AudioSource::System {
            return Err(RecorderError::NoInputDevice);
        }
        let (samples, stop) = (self.0.clone(), Arc::new(AtomicBool::new(false)));
        let stopped = stop.clone();
        std::thread::spawn(move || {
            let start = Instant::now();
            for (i, chunk) in samples.chunks(1_600).enumerate() {
                let due = Duration::from_millis(100 * (i as u64 + 1));
                std::thread::sleep(due.saturating_sub(start.elapsed()));
                if stopped.load(Ordering::Relaxed) {
                    break;
                }
                on_chunk(chunk);
            }
        });
        Ok(Box::new(Playing(stop)))
    }
}

pub async fn run(st: &Arc<AppState>, dir: &Path, cx: &mut AsyncApp) {
    // A hidden app is napped, which throttles its foreground tasks far below the frame rate being filmed.
    let _nap = dito_platform::system::NapGuard::new("transcript-film");
    services::save_config(st, |c| c.general.appearance = Appearance::Light);
    cx.update(|cx| {
        tools::open(st.clone(), cx);
        resize_tools_window(cx, 1120., 720.);
    });
    let mut audio = vec![0.0; 8_000];
    let mut lines = Vec::new();
    for (i, (text, secs, pause)) in crate::dev::seed::FILM_SCRIPT.iter().enumerate() {
        audio.extend(tone(i, *secs));
        audio.extend(vec![0.0; (pause * 16_000.) as usize]);
        lines.push((text.to_string(), *secs));
    }
    let length = Duration::from_secs_f32(audio.len() as f32 / 16_000.);
    let asr = ToneTranscriber::new(lines, Duration::from_millis(250), Duration::from_millis(60));
    let handle = {
        let _rt = st.rt.enter();
        transcription::spawn(TranscriptionDeps {
            audio: Arc::new(Talk(audio)),
            detectors: Arc::new(EnergyDetectors),
            engines: Arc::new(FixedEngines::new(
                Arc::new(asr),
                Arc::new(MockTextModel::default().with_delay(Duration::from_millis(2_500))),
            )),
            config: st.cfg.clone(),
            store: st.store.clone(),
        })
    };
    on_tools_window(cx, |w, cx, view| {
        let handle = handle.clone();
        view.update(cx, |t, cx| t.follow_transcription(handle, w, cx));
    });
    on_tools_window(cx, |_, app, view| view.update(app, |t, cx| t.pose_transcription_start_pressed(cx)));
    handle.start(Sources { system: true, microphone: false });

    // Encoding on this thread would stall the page between frames.
    let (tx, rx) = std::sync::mpsc::channel::<(String, image::RgbaImage)>();
    let out = dir.to_path_buf();
    let saver = std::thread::spawn(move || {
        for (name, image) in rx {
            save(&out, &name, image);
        }
    });
    let start = Instant::now();
    let mut stopped = false;
    let (pause_at, resume_at) = (length.mul_f32(0.3), length.mul_f32(0.5));
    let (arm_at, stop_at) = (length.mul_f32(0.7), length.mul_f32(0.85));
    let mut events = [(pause_at, "pause"), (resume_at, "resume"), (arm_at, "arm"), (stop_at, "stop")];
    let mut due = Duration::ZERO;
    loop {
        let wait = due.saturating_sub(start.elapsed());
        if !wait.is_zero() {
            cx.background_executor().timer(wait).await;
        }
        let now = start.elapsed();
        let dense = [Duration::ZERO, arm_at, stop_at].iter().any(|at| now >= *at && now < *at + DENSE_FOR);
        due = now + if dense { DENSE_FRAME } else { FRAME };
        for (at, what) in events.iter_mut() {
            if matches!(*what, "pause" | "resume") && start.elapsed() >= *at {
                handle.toggle_pause();
                *what = "";
            } else if *what == "arm" && start.elapsed() >= *at {
                on_tools_window(cx, |_, app, view| view.update(app, |t, cx| t.pose_transcription_stop_armed(cx)));
                *what = "";
            } else if *what == "stop" && start.elapsed() >= *at {
                handle.stop();
                stopped = true;
                *what = "";
            }
        }
        // Dense frames leave the foreground no gap to deliver the session in, so the page takes it here, as its
        // subscription would.
        let status = handle.status();
        on_tools_window(cx, |w, app, view| view.update(app, |t, cx| t.sync_transcription(status, w, cx)));
        if let Some(Ok(image)) = render_tools_window(cx) {
            let status = handle.status();
            let state = match status.state {
                SessionState::Recording { segment_start: None, .. } => "paused",
                SessionState::Recording { .. } => "rec",
                SessionState::Finishing { .. } => "fin",
                SessionState::Idle => "idle",
                SessionState::Starting => "start",
            };
            let _ = tx.send((format!("film-{:06}-{state}.png", start.elapsed().as_millis()), image));
        }
        if stopped && handle.status().state == SessionState::Idle {
            break;
        }
    }
    drop(tx);
    let _ = saver.join();
}
