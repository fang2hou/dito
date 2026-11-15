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

    /// Arms the two-step stop as a first press does: the confirm pill widens without a pointer.
    #[cfg(feature = "screenshots")]
    pub fn pose_transcription_stop_armed(&mut self, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.pose_stop_armed(cx));
    }

    /// Hands the page the session's status at once, as its subscription would.
    #[cfg(feature = "screenshots")]
    pub fn sync_transcription(
        &mut self,
        status: dito_core::transcription::TranscriptionStatus,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transcription.update(cx, |t, cx| t.apply_status(status, window, cx));
    }

    /// Opens or closes the transcription settings dialog without a pointer.
    #[cfg(feature = "screenshots")]
    pub fn pose_transcription_settings(&mut self, open: bool, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.pose_settings(open, cx));
    }

    /// Focuses the first editable paragraph of the open transcript.
    #[cfg(feature = "screenshots")]
    pub fn pose_transcription_paragraph_focus(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, cx| t.pose_focus_paragraph(window, cx));
    }

    /// Marks New meeting as just pressed, so a session started next folds the button as a click does.
    #[cfg(feature = "screenshots")]
    pub fn pose_transcription_start_pressed(&mut self, cx: &mut Context<Self>) {
        self.transcription.update(cx, |t, _| t.pose_start_pressed());
    }
}
