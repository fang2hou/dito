//! The capsule's wait when the command shortcut is the dictation shortcut plus modifiers (`motion/chord-hold`).

use super::*;

/// Past the hold, with room for the actor's task to run.
async fn past_the_hold() {
    tokio::time::sleep(CHORD_HOLD + Duration::from_millis(60)).await;
}

async fn wait_for_listening(r: &Rig) -> HudState {
    wait_for(&r.h, |s| matches!(s, HudState::Listening { .. })).await
}

#[tokio::test]
async fn the_recording_starts_at_the_first_key_and_the_capsule_waits() {
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    let pressed = Instant::now();
    r.h.send(Command::KeyDownMayHandOver);
    settle().await;
    assert_eq!(counts(&r), (1, 0), "recording does not wait");
    let started = r.h.recording_started().expect("recording");
    assert!(started.duration_since(pressed) < CHORD_HOLD, "the clock started at the key, not after the wait");
    assert_eq!(*r.h.hud().borrow(), HudState::Hidden, "no capsule yet");

    wait_for_listening(&r).await;
    assert!(pressed.elapsed() >= CHORD_HOLD, "the capsule came after the wait");
    assert_eq!(r.h.run_kind(), RunKind::Dictation, "a plain dictation pops in as one");
    assert_eq!(r.h.recording_started(), Some(started), "the wait did not move the recording clock");
}

#[tokio::test]
async fn a_handover_inside_the_wait_never_shows_a_dictation_capsule() {
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    let mut hud = r.h.hud();
    r.h.send(Command::KeyDownMayHandOver);
    settle().await;
    assert_eq!(*hud.borrow_and_update(), HudState::Hidden);
    let started = r.h.recording_started();

    r.h.send(Command::KeyToCommand);
    wait_for_listening(&r).await;
    assert_eq!(r.h.run_kind(), RunKind::Command, "the first state shown already belongs to the command");
    assert_eq!(counts(&r), (1, 1), "one recording, the selection read at the handover");
    assert_eq!(r.h.recording_started(), started);

    // The wait's own timer fires later and changes nothing.
    hud.borrow_and_update();
    past_the_hold().await;
    assert!(!hud.has_changed().unwrap(), "no second state after the wait");
    assert_eq!(r.h.run_kind(), RunKind::Command);
}

#[tokio::test]
async fn a_handover_after_the_wait_converts_the_visible_capsule() {
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    r.h.send(Command::KeyDownMayHandOver);
    wait_for_listening(&r).await;
    assert_eq!(r.h.run_kind(), RunKind::Dictation);

    r.h.send(Command::KeyToCommand);
    wait_for_run_kind(&r, RunKind::Command).await;
    assert_eq!(*r.h.hud().borrow(), HudState::Listening { mode: RecordMode::Toggle });
    r.h.send(Command::CommandToKey);
    wait_for_run_kind(&r, RunKind::Dictation).await;
}

#[tokio::test]
async fn a_typing_chord_or_a_cancel_inside_the_wait_shows_nothing() {
    for abort in [Command::ChordAbort, Command::Cancel] {
        let r = simple_rig("x");
        let mut hud = r.h.hud();
        r.h.send(Command::KeyDownMayHandOver);
        settle().await;
        assert_eq!(counts(&r), (1, 0));
        r.h.send(abort);
        settle().await;
        assert!(r.h.recording_started().is_none(), "the recording is over");
        past_the_hold().await;
        assert_eq!(*hud.borrow_and_update(), HudState::Hidden);
        assert_eq!(r.asr.calls(), 0);
    }
}

#[tokio::test]
async fn a_second_press_inside_the_wait_still_ends_a_toggle_recording() {
    let r = simple_rig("x");
    r.h.send(Command::KeyDownMayHandOver);
    settle().await;
    r.h.send(Command::KeyDown);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["x."]);
    past_the_hold().await;
    assert_eq!(*r.h.hud().borrow(), HudState::Inserted, "the late reveal of the first run is ignored");
}

#[tokio::test]
async fn releasing_inside_the_wait_still_ends_a_hold_recording() {
    let r = rig(
        MockTranscriber::new("x"),
        MockPolisher::default(),
        RigOptions { mode: RecordMode::Hold, ..Default::default() },
    );
    r.h.send(Command::KeyDownMayHandOver);
    settle().await;
    r.h.send(Command::KeyUp);
    wait_for(&r.h, |s| *s == HudState::Inserted).await;
    assert_eq!(*r.sink.delivered.lock().unwrap(), ["x."]);
}

#[tokio::test]
async fn a_plain_press_shows_the_capsule_at_once() {
    let r = simple_rig("x");
    r.h.send(Command::KeyDown);
    wait_for_listening(&r).await;
    let r = command_rig_in(RecordMode::Toggle, Some("teh text"), "the text");
    let pressed = Instant::now();
    r.h.send(Command::CommandKeyDown);
    wait_for_listening(&r).await;
    assert!(pressed.elapsed() < CHORD_HOLD, "the command shortcut itself never waits");
    assert_eq!(r.h.run_kind(), RunKind::Command);
}
