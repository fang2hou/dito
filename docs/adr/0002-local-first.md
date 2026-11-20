# ADR-0002: Local first; the cloud is optional per stage

- **Status**: accepted
- **Date**: 2026-11-05

## Context

Dictation handles whatever the user says, often at work. Requiring a cloud account makes the app useless offline, adds cost, and sends every word to a third party. Cloud models are still better for some users and languages.

## Decision

Speech recognition and polishing each run either on the Mac or through a provider, chosen separately. Local is the default and the complete flow works without any account.

- Local runtimes are llama.cpp (Metal), sherpa-onnx and transcribe.cpp (Metal), linked into one binary behind `RuntimeLoader`. Nemotron Transcribe uses a private NeMo-Speech.cpp worker behind the same interface because its ggml version is incompatible with the linked runtimes.
- llama.cpp and transcribe.cpp each vendor ggml, and a static link keeps one copy for both. This is allowed only while the two pins vendor the same ggml release (0.26.0 for `llama-cpp-sys-2` 0.1.161 and `transcribe-cpp` 0.4.2, with an identical `ggml.h`). transcribe.cpp's copy adds small patches and one registry function, so it is the copy that links.
- When a local engine is unavailable, the app says so and shows red; it never returns placeholder text.
- A stop request reports `Stopping` while the runtime is released outside the UI thread, then `Stopped`. A load that finishes after a stop request cannot restore the stopped runtime.
- Nothing downloads until the user asks; models in use start at launch, and a model the user stopped stays stopped.
- A process-backed model has the same download and engine states as an embedded model. Its pinned runtime archive downloads with its weights, is verified before extraction, and needs no manual installation, endpoint or provider configuration. The app owns the worker, waits for authenticated readiness, cancels startup when stopped, and closes the worker on unload or quit even while callers retain handles. A pipe guard cleans up after an abrupt app exit.
- All worktrees of the development build use the same development model directory unless the owner explicitly chooses another folder. A per-model file lock serializes downloads across app instances; a waiter reuses files completed by the first downloader.

## Alternatives Considered

### Cloud only

- Pros: Smaller app, simpler code
- Cons: Offline use, cost and privacy
- Why not chosen: Rejected

### whisper.cpp for local ASR

- Pros: Popular
- Cons: Ships its own ggml, which clashes with llama.cpp in one binary, and runs one model family
- Why not chosen: Rejected. transcribe.cpp (2026-11-13) runs Whisper and many other families from one adapter, on a ggml release that matches llama.cpp's

## Consequences

App builds need cmake and the Metal toolchain, and model management is part of the product. Cloud features must never become a prerequisite for the basic flow.

## Pitfalls

Moves that look reasonable but go against this decision:

- Making a provider account required during onboarding or for any core feature.
- Adding a runtime that bundles a different ggml release next to llama.cpp, or bumping `llama-cpp-sys-2` or `transcribe-cpp` alone. The link still succeeds and one library then runs on the other's ggml. Compare `ggml/CMakeLists.txt` and `ggml/include/ggml.h` of both crates, then run a Qwen ASR model and a transcribe.cpp model in one process.
- Returning fake or placeholder results when a runtime or model is missing.

## Review Triggers

Local models stop being competitive, or the app targets machines that can't run them.

## Code

`crates/dito-local`, `crates/dito-runtimes`, `crates/dito-core/src/factory.rs`, `crates/dito-app/src/main.rs` (`autostart_local_models`).

## Model catalog extension

Catalog entries declare an inference layout and exact file roles, separately from the backend library. SenseVoice and NeMo TDT use sherpa-onnx; Qwen ASR (including the Confucius4-R2T2 fine-tune) and chat use llama.cpp; a single-file speech GGUF of any family transcribe.cpp runs (Whisper, Fun-ASR Nano) uses the `TranscribeCpp` layout. `NemoSpeech` names the decoder, encoder and runtime archive for managed NeMo-Speech.cpp inference. Another checkpoint of a supported layout adds a catalog entry with pinned files, sizes, hashes and display metadata. A new architecture adds a runtime adapter, without changing the user-facing download or engine lifecycle.

Text model specifications also declare native context limits, any separately published output ceiling, output-length guidance and thinking capabilities. Thinking levels carry a protocol `key` and a display `name`, and only supported levels are listed. Unknown separate output limits remain unset rather than being inferred from examples. These facts do not change the current direct-answer runtime policy; future callers can use them to choose request settings independently.

Hotword availability is read from the layout instead of model IDs. A language-focus label describes the intended use and never blocks other input languages. A decoder that does not detect language returns no language rather than inventing one. Haas-ja uses the community Apache-2.0 mixed INT4/INT8 ONNX conversion through the NeMo TDT adapter and stays an optional download. The transcribe.cpp adapter reads languages, language detection and vocabulary support from the loaded file, drops a language hint the model was not trained on, and passes hotwords as the vocabulary, which the library formats per family.

The sherpa backend uses the official `sherpa-onnx-sys` Rust bindings, pinned with the native library at 2.0.1. This replaces the deprecated third-party binding and its ONNX Runtime 1.17.1 build, which cannot execute Haas's quantized convolutions on Apple Silicon. The shared adapter owns native recognizers and streams, serializes inference, preserves SenseVoice language detection from the result JSON and returns ordinary load errors. No model rewriting or process working-directory changes are needed.
