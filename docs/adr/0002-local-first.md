# ADR-0002: Local first; the cloud is optional per stage

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Dictation handles whatever the user says, often at work. Requiring a cloud account makes the app useless offline, adds cost, and sends every word to a third party. Cloud models are still better for some users and languages.

## Decision

Speech recognition and polishing each run either on the Mac or through a provider, chosen separately. Local is the default and the complete flow works without any account.

- Local runtimes are llama.cpp (Metal) and sherpa-onnx, linked into one binary behind `RuntimeLoader`.
- When a local engine is unavailable, the app says so and shows red; it never returns placeholder text.
- A stop request reports `Stopping` while the runtime is released outside the UI thread, then `Stopped`. A load that finishes after a stop request cannot restore the stopped runtime.
- Nothing downloads until the user asks; models in use start at launch, and a model the user stopped stays stopped.

## Alternatives Considered

### Cloud only

- Pros: Smaller app, simpler code
- Cons: Offline use, cost and privacy
- Why not chosen: Rejected

### whisper.cpp for local ASR

- Pros: Popular
- Cons: Ships its own ggml, which clashes with llama.cpp in one binary
- Why not chosen: Rejected

## Consequences

App builds need cmake and the Metal toolchain, and model management is part of the product. Cloud features must never become a prerequisite for the basic flow.

## Pitfalls

Moves that look reasonable but go against this decision:

- Making a provider account required during onboarding or for any core feature.
- Adding a runtime that bundles its own ggml next to llama.cpp.
- Returning fake or placeholder results when a runtime or model is missing.

## Review Triggers

Local models stop being competitive, or the app targets machines that can't run them.

## Code

`crates/dito-local`, `crates/dito-runtimes`, `crates/dito-core/src/factory.rs`, `crates/dito-app/src/main.rs` (`autostart_local_models`).

## Model catalog extension

Catalog entries declare an inference layout and exact file roles, separately from the backend library. SenseVoice and NeMo TDT use sherpa-onnx; Qwen ASR and chat use llama.cpp. Another checkpoint of a supported layout adds a catalog entry with pinned files, sizes, hashes and display metadata. A new architecture adds a runtime adapter, without changing downloads or engine lifecycle management.

Hotword availability is read from the layout instead of model IDs. A language-focus label describes the intended use and never blocks other input languages. A decoder that does not detect language returns no language rather than inventing one. Haas-ja uses the community Apache-2.0 mixed INT4/INT8 ONNX conversion through the NeMo TDT adapter and stays an optional download.

The sherpa backend uses the official `sherpa-onnx-sys` Rust bindings, pinned with the native library at 2.0.1. This replaces the deprecated third-party binding and its ONNX Runtime 1.17.1 build, which cannot execute Haas's quantized convolutions on Apple Silicon. The shared adapter owns native recognizers and streams, serializes inference, preserves SenseVoice language detection from the result JSON and returns ordinary load errors. No model rewriting or process working-directory changes are needed.
