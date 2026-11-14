//! What the screenshot harness reads and steers on the tools window.

use gpui_kit::prelude::*;
use gpui_kit::*;

use super::Tools;

impl Tools {
    pub fn assume_transcription_ready(&mut self, ready: bool, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.assume_ready(ready, cx));
    }
    /// Opens the newest transcript as if it were still being recorded.
    pub fn preview_live_transcript(&mut self, tail: [&str; 3], window: &mut Window, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.preview_live(tail, window, cx));
    }

    /// The transcription page follows this session instead of the app's own.
    pub fn follow_transcription(
        &mut self,
        handle: dito_core::transcription::TranscriptionHandle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transcription.update(cx, |t, cx| t.follow(handle, window, cx));
    }

    /// Opens the newest transcript, as a click on its row does.
    pub fn preview_transcript(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.open_newest(window, cx));
    }
}
