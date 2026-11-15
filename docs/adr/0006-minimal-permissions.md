# ADR-0006: Microphone, Accessibility and System Audio Recording

- **Status**: accepted
- **Date**: 2026-11-05, updated 2026-11-15 to add System Audio Recording

## Context

Every system permission is a trust cost and a setup step. Dictation needs the microphone, and typing into other apps and hearing a global shortcut need Accessibility. Meeting transcription also records the other side of a call, which macOS gates behind System Audio Recording.

## Decision

Dito asks for three permissions and nothing else: Microphone, Accessibility and System Audio Recording. They are requested up front, in one place each:

- Onboarding shows マイク, アクセシビリティ and システムオーディオ録音 side by side, each with its own Allow button or its granted chip. None of them gates Continue.
- Status lists System Audio Recording among the system checks, between マイク and 音声認識.
- The Tools window's readiness check names the permissions and the VAD model before a transcription session starts.

Only meeting transcription needs System Audio Recording, so it never gates dictation: the Status verdict, the menu bar health and the dictation flow ignore it, even while its row shows 未許可.

The global shortcut uses an active event tap, which needs only Accessibility and can also swallow Esc and shortcut keys; it recognizes lone modifiers such as right ⌘ and treats a quick following key as typing. Screen Recording is not used: ScreenCaptureKit would capture audio too, but it asks for far more trust than the feature needs. A transcription session records only the sources the user picked for it (desktop audio, the microphone or both).

### Reading and requesting System Audio Recording

macOS exposes no public API for this permission: neither `AVCaptureDevice` nor Core Audio reports whether the app may record other apps' output. Dito therefore uses TCC's private SPI, the way insidegui/AudioCap does:

- `TCCAccessPreflight(kTCCServiceAudioCapture, NULL)` answers granted (0), denied (1) or anything else, which reads as not determined.
- `TCCAccessRequest(kTCCServiceAudioCapture, NULL, block)` shows the recording prompt while the app is undecided.

Both symbols are resolved lazily with `dlopen`/`dlsym` from `/System/Library/PrivateFrameworks/TCC.framework/Versions/A/TCC`, so a macOS build that drops either one degrades instead of crashing: the status reads as not determined and the button opens the System Settings pane (`Privacy_AudioCapture`) instead of prompting. A permission the SPI cannot observe is never reported as granted.

The first real capture also registers the app in that pane: the Core Audio process tap carries `NSAudioCaptureUsageDescription`, so a build without the SPI still gets the system's own prompt when a session starts.

Only the permission buttons in onboarding, Status and Settings request access. Startup checks Accessibility silently and starts the global shortcut once access is granted. Recording checks Microphone access before opening an input stream.

## Alternatives Considered

### Listen-only event tap

- Pros: Passive
- Cons: Needs Input Monitoring and can't swallow Esc or combos
- Why not chosen: Rejected

### GPUI key handling

- Pros: No permission
- Cons: Can't see lone modifiers or keys while other apps are focused
- Why not chosen: Rejected

### Ask for System Audio Recording only when a session starts

- Pros: No setup step for users who never transcribe
- Cons: The session begins recording silence until the user answers, and the readiness check cannot say what is missing
- Why not chosen: Rejected; the Tools window states what is missing before a session starts.

## Consequences

The private SPI can change with any macOS update, and a TestFlight or App Store review may refuse it. If `TCCAccessPreflight` stops answering, Status and onboarding report not determined and the user is sent to Settings, so the feature still works once the switch is flipped by hand; only the automatic check is lost.

Without a public check, a denied permission shows up as silence rather than an error, so the Transcription page also says up front that macOS will ask.

Features that would need another permission (Screen Recording, Input Monitoring, contacts) need an explicit product decision and an update to this ADR.

## Pitfalls

Moves that look reasonable but go against this decision:

- Adding an Input Monitoring step or switching to a listen-only tap.
- Adding a feature that silently depends on a new permission.
- Letting System Audio Recording turn the dictation verdict red: dictation never records other apps' audio.
- Treating a TCC answer other than 0 or 1 as granted, or as denied instead of not determined.
- Adding a dependency to read or request this permission: the SPI costs one `dlopen` and two function pointers.
- Capturing system audio through ScreenCaptureKit, which needs Screen Recording.
- Opening a system audio tap anywhere but a transcription session the user started.

## Review Triggers

A feature is proposed that can't work within these three permissions, or a macOS update breaks the TCC SPI.

## Code

`crates/dito-platform/src/permissions.rs` (the `tcc` module holds the SPI), `crates/dito-platform/src/hotkey/`,
`crates/dito-app/src/settings/status/`, `crates/dito-app/src/onboarding/steps/permissions.rs`,
`crates/dito-audio/src/live.rs`, the `NSAudioCaptureUsageDescription` entries in `scripts/bundle.sh` and
`scripts/dev-runner.sh`.
