# ADR-0006: Microphone and Accessibility, plus System Audio for transcription

- **Status**: accepted
- **Date**: 2026-11-05, updated 2026-11-12 for transcription

## Context

Every system permission is a trust cost and a setup step. Dictation needs the microphone, and typing into other apps and hearing a global shortcut need Accessibility.

## Decision

Dito asks for Microphone and Accessibility and nothing else. The global shortcut uses an active event tap, which needs only Accessibility and can also swallow Esc and shortcut keys; it recognizes lone modifiers such as right ⌘ and treats a quick following key as typing.

Only the permission buttons in onboarding and Settings request access. Startup checks Accessibility silently and starts the global shortcut once access is granted. Recording checks Microphone access before opening an input stream.

Transcription adds System Audio Recording, the only other permission. It is asked for by macOS itself the first time the user starts a transcription session from the app (Core Audio process tap, `NSAudioCaptureUsageDescription`), never at startup or in onboarding, and dictation does not need it. Screen Recording is not used: ScreenCaptureKit would capture audio too, but it asks for far more trust than the feature needs. A session records only the sources the user picked for it (desktop audio, the microphone or both), and the system audio permission is asked for only when desktop audio is one of them.

## Alternatives Considered

### Listen-only event tap

- Pros: Passive
- Cons: Needs Input Monitoring and can't swallow Esc or combos
- Why not chosen: Rejected

### GPUI key handling

- Pros: No permission
- Cons: Can't see lone modifiers or keys while other apps are focused
- Why not chosen: Rejected

## Consequences

Features that would need another permission (screen recording, Input Monitoring, contacts) need an explicit product decision and an update to this ADR. macOS offers no public check for System Audio Recording: a denied tap records silence, so the Transcription page says up front that macOS will ask.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding an Input Monitoring step or switching to a listen-only tap.
- Adding a feature that silently depends on a new permission.
- Capturing system audio through ScreenCaptureKit, which needs Screen Recording.
- Opening a system audio tap anywhere but a transcription session the user started.

## Review Triggers

A feature is proposed that can't work within these two permissions.

## Code

`crates/dito-platform/src/permissions.rs`, `crates/dito-platform/src/hotkey/`, `crates/dito-audio/src/live.rs`, the `NSAudioCaptureUsageDescription` entries in `scripts/bundle.sh` and `scripts/dev-runner.sh`.
