# Nemotron 3.5 Transcribe feasibility

Measured on 2026-11-16, at Dito revision `f8a05607472713525bbb9dc099f24d2a6bcd9191`.

## Managed integration follow-up

The managed path is now implemented in this worktree. `NemoSpeech` is a normal local catalog layout. A download fetches the pinned runtime archive with the two weights, startup launches and authenticates a private Metal worker, and stop or quit closes it even while a transcriber handle is retained. Parent-death cleanup is tested separately. `LocalEngines` reports child-process resource usage and worker failures through its existing interface.

The research weights were moved, not copied, to the shared development directory at `~/Library/Application Support/com.fang2hou.Dito-dev/models/nemotron-3.5-transcribe`. The old research path links to that directory. The actual model manager reused both complete weights and downloaded only the runtime archive, then completed Japanese, Japanese/English, English, Chinese and Chinese/English transcription, stop, restart and exit cleanup. Startup cancellation was also exercised with the real loader. A 39.556-second Japanese clip containing distinct sentences completed the actual app dictation pipeline, preserved the final sentence and reached the normal success state with the log sink. The adapter sends complete recordings instead of using the short-utterance chunker. Both explicit startup cancellation and abandonment of the startup future release the worker and clear the model state. This changes the initial endpoint-only findings below into an integrated local-model path; the original measurements remain an initial feasibility record.

A separate 38.206-second fixture repeating the same sentence seven times triggered the upstream runtime's repetition guard and ended early. The 0.3.0 HTTP API reports `finish_reason=stop` even on this guard path, so it cannot be reliably distinguished from ordinary completion by the adapter. This is a recognition limitation of this runtime, not a successful long-repeat transcription test. Do not use repetitive synthetic fixtures to claim long-form coverage.

## Initial result

The public AtomicChat IQ4_NL conversion runs on an Apple M5 Pro with 48 GB unified memory through NVIDIA NeMo-Speech.cpp 0.3.0's Metal release. Dito's existing `ConfiguredAsr` can use its multipart transcription endpoint and JSON chat endpoint without a production-code change or an additional dependency. This is an endpoint experiment, not a running-app UI test or an embedded-runtime implementation.

Keep it an optional model experiment. The model files total 21.35 GB, loading took 6.1 to 24.2 seconds across four launches, and short-request latency varied substantially. Only synthetic clips of 4.5 to 8.3 seconds were tested, plus silence. Real microphone audio, long recordings, cancellation, concurrency and sustained memory pressure remain untested.

## Pinned artifacts

| Artifact | Revision or size |
| --- | --- |
| Runtime | NVIDIA/NeMo-Speech.cpp `v0.3.0`, source commit `20999cc921d2b14aa58708f61756a8b2e787ba20` |
| Runtime archive | `nemo-speech-0.3.0-macos-aarch64-metal.tar.gz`, SHA-256 `2b972ed8b5dba07492212acf8cb75cbf34ecb141198ffa1997febe84b7c4d9ed` |
| Model repository | `AtomicChat/NVIDIA-Nemotron-3.5-Transcribe-30B-A4B-GGUF` |
| Model revision | `dbd06c5cad7a2f33c8e06be2fb3ef814dbd9aa59` |
| Decoder | `NVIDIA-Nemotron-3.5-Transcribe-30B-A4B-AD-IQ4_NL.gguf`, 19,678,656,672 bytes |
| Required encoder | `NVIDIA-Nemotron-3.5-Transcribe-30B-A4B-Encoder-F16.gguf`, 1,670,410,080 bytes |
| Decoder SHA-256 | `0c47d4ab6c783e7606ec03714b354fab0b2052027a7ea1991e7e2d3009c345ad` |
| Encoder SHA-256 | `4221029c8a565c4a80c507ffa760c716e595acf663f9f4273ce69910050e7ba2` |

All three file hashes were checked. Downloads, binaries, copied build artifacts, request configurations and measurements stay outside the checkout.

## Measurements

The client was `configured_asr_probe`, which calls the existing Rust `ConfiguredAsr` with the synthetic WAVs defined in `bench/data/clips.json`. Latency includes WAV encoding, the loopback HTTP request and response parsing. The server stayed loaded between requests. No app settings, microphone or text insertion were used. macOS was 27.2.1.

| Case | Audio duration | Request latency | Observation |
| --- | ---: | ---: | --- |
| Japanese, default transcription, five repeats in run 2 | 4.958 s | 1.245 to 3.908 s, median 3.453 s | Same correct words in all five outputs |
| Japanese, script instruction without vocabulary, run 4 | 4.958 s | 1.192 s, then 0.590 s | Correct words in both outputs |
| Japanese with English, same instruction, run 4 | 6.799 s | 0.783 s | Preserved all five English terms in Latin script |
| English, default transcription, run 2 | 4.532 s | 2.857 s | Correct words |
| Chinese, default transcription, run 2 | 8.283 s | 4.122 s | Exact reference text |
| Silence, run 2 | 3.000 s | 0.003 s | Server returned empty text; Dito reported `NoSpeech` |

The highest sampled server RSS across the four launches was approximately 18.2 GiB. This is process RSS sampled every 0.5 seconds, not total system memory or a minimum-memory guarantee. Startup measurements include model initialization without explicit warmup. Cache state was not controlled. These small trials do not establish an accuracy ranking or a general latency distribution.

## Prompt behavior

- The default transcription prompt translated English terms in a Chinese mixed-language clip into Chinese.
- A Japanese/English instruction without a vocabulary list preserved the mixed-language reference and did not alter the plain Japanese control clip. The tested instruction was:

  > Transcribe verbatim in the original languages. Preserve hesitations and false starts. Write Japanese in Japanese script and English in Latin script. Return only the transcript with punctuation and capitalization.

- A Japanese-specific vocabulary list preserved the mixed-language terms in two trials, but inserted `issue` into an unrelated word in the plain Japanese control. A shared Chinese/English vocabulary also changed a Japanese clip into Chinese and inserted unrelated terms.
- Chinese vocabulary improved several rare names, but did not recover every reference character. Vocabulary is a generation bias, not a spelling guarantee.
- `ConfiguredAsr` currently does not substitute `AsrRequest.hotwords` into requests. A fixed instruction in a JSON configuration is supported; dynamic wordbook integration would require a separate change.

## Reproduce the endpoint

Keep the decoder and encoder in the same external directory. With the extracted runtime on `PATH`, start:

```bash
nemo-speech serve --asr-model "$MODEL" --device metal \
  --host 127.0.0.1 --port 18761 --threads 4 --no-ui --no-warmup \
  --asr.salm.max_sequences 1 --asr.salm.n_ctx_per_sequence 4096 \
  --asr.salm.max_context 4096 --asr.salm.max_new_tokens 512 \
  --asr.salm.n_batch 512 --asr.salm.encoder_max_batch 1 \
  --asr.salm.mtp_draft 0
```

Read `GET /v1/models` for the served model ID. Use it as `connection.model` in a Dito ASR configuration. The multipart route is `/v1/audio/transcriptions` with `file`, `language` and `response_format=json`, extracting `/text`. The instruction route is `/v1/chat/completions` with one user message containing `text` and `input_audio` parts, extracting `/choices/0/message/content`.

Do not send `max_tokens` or `max_completion_tokens`: this release refuses them with HTTP 400. The 0.3.0 CLI also refuses `--asr.salm.batched`; Metal automatically used the upstream llama.cpp path. The tested context and output settings are for short dictation, not the model's advertised one-hour capacity.

The initial `configured_asr_probe` helper was archived outside the checkout with the research artifacts after the managed path replaced manual endpoint setup. All four initial experimental server instances were stopped.

Exercise the integrated path with the shared development cache instead:

```bash
cargo run --locked -p dito-runtimes --example managed_asr_e2e -- \
  /tmp/dito-managed-asr "$AUDIO_DIR"
```

The managed example downloads only missing files, runs the existing local-engine lifecycle and checks stop and restart. The user-facing app reuses the existing controls in `design:settings/local-models`. Its status table includes the model worker in total CPU and memory rather than subtracting it from the main process alone.

## Embedded-runtime implications

The current `transcribe-cpp` 0.4.2 package has no implementation of this speech-augmented Nemotron architecture. Dito's Qwen-specific audio adapter also cannot load it by changing a catalog filename. NeMo-Speech.cpp's release ships ggml 0.25.1, whereas Dito's linked runtimes use 0.26.0. An embedded adapter needs deliberate ggml/link compatibility work under `docs/adr/0002-local-first.md`; the separate HTTP process avoids that conflict.

The initial endpoint experiment changed no production UI, catalog, dependencies or ADR decisions. The managed follow-up updates the catalog, runtime and lifecycle interface and ADR-0002, while reusing the existing model controls. Validation for the integrated version is recorded in the implementation hand-over.

## Sources

- [NVIDIA model announcement](https://research.nvidia.com/labs/nemotron/NVIDIA-Nemotron-3.5-Transcribe/)
- [Runtime release](https://github.com/NVIDIA/NeMo-Speech.cpp/releases/tag/v0.3.0)
- [Pinned HTTP contract](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.3.0/docs/api.md)
- [Pinned SALM instructions](https://github.com/NVIDIA/NeMo-Speech.cpp/blob/v0.3.0/docs/salm/README.md)
- [Pinned community conversion](https://huggingface.co/AtomicChat/NVIDIA-Nemotron-3.5-Transcribe-30B-A4B-GGUF/tree/dbd06c5cad7a2f33c8e06be2fb3ef814dbd9aa59)
