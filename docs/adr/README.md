# Architecture Decision Records

ADRs record the judgments that should steer future work: the direction the product and code take, and the tempting moves that would take them somewhere worse. Design details live in `design/`, and one-time facts and engineering traps live in `docs/IMPLEMENTATION.md`.

Before changing something an ADR covers, read it. When a request conflicts with one, raise it, then update or supersede the ADR in the same change instead of working around it.

| ADR | Decision |
| --- | --- |
| [0001](0001-design-leads-code.md) | The design file leads the code |
| [0002](0002-local-first.md) | Local first; the cloud is optional per stage |
| [0003](0003-never-lose-speech.md) | Never lose what the user said |
| [0004](0004-hud-out-of-the-way.md) | The HUD never takes focus and never gets in the way |
| [0005](0005-reliable-delivery.md) | Reliable insertion beats simple insertion |
| [0006](0006-minimal-permissions.md) | Microphone, Accessibility and System Audio Recording |
| [0007](0007-declarative-cloud-asr.md) | Cloud speech vendors through a declarative config |
| [0008](0008-data-stays-local.md) | User data and secrets stay on the Mac |
| [0009](0009-portable-core.md) | The core is independent of UI and macOS; one actor owns the dictation |
| [0010](0010-one-home-per-setting.md) | Each setting has exactly one home |
| [0011](0011-japanese-first.md) | Japanese first, display languages compiled in |
| [0012](0012-structure-over-prompting.md) | Fix model behavior with structure, not longer prompts |
| [0013](0013-ui-state-ownership.md) | UI state belongs to its view and never blocks rendering |
| [0014](0014-silent-launch.md) | Launching Dito opens no window |
| [0015](0015-versioned-persistence.md) | Persisted formats are versioned and migrated before loading |
| [0016](0016-ui-kit-crate.md) | The shared UI kit becomes its own crate, in stages |
