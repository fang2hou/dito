# Haas-ja on Apple Silicon

Measured on 2026-11-06, Apple M5 Pro, 48 GiB unified memory, macOS 27.2, Python 3.15.2.

Haas-ja runs efficiently on this Mac. The community MLX BF16 port was fastest among the two runtimes measured here: 31 ms for 4.958 seconds of Japanese speech and 152 ms for a synthetic 30-second input after warmup. The mixed INT4/INT8 ONNX conversion also works with the existing sherpa-onnx dependency, without changing Dito's runtime architecture. These are feasibility and latency measurements, not evidence that Haas is more accurate than the existing engines.

## Local measurements

Seven timed repetitions after one warmup for each input, one model resident, one request at a time. CPU thread configurations ran in separate processes and sequentially. Both runtimes include feature extraction and decoding; CPU decode timing excludes WAV loading and `accept_waveform`, while MLX excludes WAV loading and waveform upload and synchronizes Metal before and after timing. CPU raw files also record wall time including WAV loading and `accept_waveform`.

| Runtime | Japanese 4.958 s, median | Japanese 15 s, median | Japanese 30 s, median | 30 s RTFx |
| --- | ---: | ---: | ---: | ---: |
| sherpa-onnx CPU, 1 thread | 378 ms | 1,094 ms | 2,230 ms | 13.5 |
| sherpa-onnx CPU, 2 threads | 201 ms | 591 ms | 1,199 ms | 25.0 |
| sherpa-onnx CPU, 4 threads | 115 ms | 346 ms | 704 ms | 42.6 |
| sherpa-onnx CPU, 8 threads | 91 ms | 246 ms | 512 ms | 58.6 |
| MLX BF16, Metal | 31 ms | 78 ms | 152 ms | 197.8 |

The 15- and 30-second inputs repeat and crop the same macOS `say` Japanese clip. They measure scaling, not recognition quality on natural long speech. RTFx means audio seconds divided by processing seconds. Results are descriptive of this machine and workload, not an extrapolation to every Mac.

- CPU 4-thread p90: 124 ms for 4.958 s, 768 ms for 30 s. Initialization with weights on disk: 826 ms. Peak process RSS across the full run: 1.38 GiB.
- MLX p90: 39 ms for 4.958 s, 153 ms for 30 s. First transcription in the fresh process: 2.28 s for the short Japanese clip; model and tokenizer initialization separately took 100 ms. Warm the model before the first user request. These are process-first timings with OS file caches potentially warm, not a cold boot experiment.
- MLX peak allocator memory: 1.70 GiB; peak process RSS: 1.25 GiB. The counters have different scopes and overlap on unified memory, so do not add them.
- SenseVoice on the same Japanese clip took 48 ms in the regular one-shot bench run, and 61 ms for the Japanese/English clip. This is a sanity-check baseline, not a seven-repeat comparison.

Raw measurements, all repetitions and transcripts are in [results/exploration/apple-m5-pro](results/exploration/apple-m5-pro/). Standard six-clip results are in [haas-ja.json](results/asr/apple-m5-pro/haas-ja.json) and [sensevoice.json](results/asr/apple-m5-pro/sensevoice.json).

## Available formats

All listed conversions are community work, not official macOS support. Sizes are decimal MB/GB unless GiB is stated. External figures below are the converters' measurements, not measurements made here, and their datasets and timing scopes differ.

| Source | Format and size | macOS route | Evidence and tradeoff |
| --- | --- | --- | --- |
| [Original](https://huggingface.co/third-intelligence/Haas-ja-v1.0) | 615.8M parameters | Official code requires Linux and NVIDIA CUDA | Japanese offline FastConformer encoder with KDA prediction network and TDT decoding. Apache-2.0. |
| [oboroge0 quantized ONNX](https://huggingface.co/oboroge0/Haas-ja-v1.0-onnx-quantized) | 481 MB total (459 MiB), mixed INT4 MatMul and INT8 pointwise convolutions, FP32 decoder/joiner | sherpa-onnx CPU, tested here | Lowest integration cost for the existing bench and Dito runtime. The frontend differs from NeMo, and KDA state uses the existing Parakeet state interface. |
| [sakasegawa GGUF](https://huggingface.co/sakasegawa/Haas-ja-v1.0-GGUF) | F16 1.23 GB, Q8_0 656 MB, Q6_K 509 MB, Q5_K 428 MB, Q4_K 352 MB | speech.cpp, Metal | Converter reports Q8_0 RTFx 205, median 42 ms on M5, FLEURS 650 clips. Not locally tested. Q4_K CER 4.42% versus F16 4.40% on that evaluation. |
| [Sanami ONNX](https://huggingface.co/Sanami/haas-ja-onnx) | FP32, FP16, INT8 and INT4 variants | ONNX Runtime, Rust ort or tract depending on variant | Author recommends `int8dyn_mm`, reports RTFx 29 on M1. Separate frontend and decoding code. Not locally tested. |
| [OneCompression ONNX](https://huggingface.co/kizuna-intelligence/Haas-ja-v1.0-OneCompression-ONNX) | INT4 GPTQ + QEP 380 MB, INT8 664 MB, FP32 2.47 GB | ONNX Runtime CPU | Author reports INT4 RTFx 30.15 on i7-12700F, not a Mac result. Uses ORT-specific MatMulNBits. Not locally tested. |
| [Sanami Core ML](https://huggingface.co/Sanami/haas-ja-coreml) | INT8 weight-only encoder, about 591 MB total | ANE with Swift runtime, macOS 14+ | Author reports total RTFx about 85 on M1. Fixed 15.04-second windows, overlap and trimming. Not locally tested. |
| [MLX port](https://huggingface.co/mrjnzzz/Haas-ja-v1.0-mlx) | BF16, about 1.23 GB, no quantized weights in this port | Custom [haas-mlx](https://github.com/matsuurajun/haas-mlx) code on Metal, tested here | Works using already installed MLX dependencies. Not compatible with mlx-audio loaders. No custom vocabulary or native streaming implementation. |
| [wkentaro Core ML](https://huggingface.co/wkentaro/Haas-ja-v1.0-coreml) | FP16 encoder and joint, FP32 frontend and KDA decoder | FluidAudio fork's `haas-ja` branch | Four-clip validation, no full benchmark in its card. Not locally tested. |

The Haas GGUF layout is specific to speech.cpp. Dito's existing llama.cpp loader cannot run it just because both use GGUF. A production Metal integration would require a separate runtime or port. No new project dependencies were added in this exploration.

## Accuracy and product fit

The regular bench intentionally ran all six clips to expose language limitations. The only pure Japanese reference is synthetic and short. Its 7.4% plain CER counts `三` versus `3` and missing punctuation; both Haas ONNX and SenseVoice produce the same spoken content here. Do not compare this number with the original model's normalized benchmark CER.

The Japanese/English clip has 54% plain CER with ONNX Haas, largely from English words rendered in Japanese, plus an error on `issue`. The MLX port also misrecognizes `issue`. Chinese and pure English fail badly. Keep this model opt-in and Japanese-specific; do not include it in the multilingual default set or infer language detection from the adapter's fixed `ja` result.

The official architecture is full-context, offline. Reprocessing a growing phrase can approximate captions, but this is not a native streaming model. Keep long dictation segmented, around 30 seconds, and validate joins. The tested ONNX and MLX adapters do not expose custom vocabulary; speech.cpp offers a separate vocabulary implementation.

For Dito, ONNX is the practical first integration candidate because sherpa-onnx already exists. MLX proves that Metal performance is promising, but the external port currently has no maintenance promise and is Python code, not an in-process Rust runtime. Before choosing a shipping engine, test natural Japanese dictation, business terms, silence, fillers, names and long-segment boundaries against the existing models using the same audio and normalization.

## Reproduce

From the repository root:

```sh
mise run bench asr run -m haas-ja
mise run bench asr report -m haas-ja --markdown
```

From `bench/`, run each thread count sequentially:

```sh
uv run python notebooks/haas_ja.py --threads 1
uv run python notebooks/haas_ja.py --threads 2
uv run python notebooks/haas_ja.py --threads 4
uv run python notebooks/haas_ja.py --threads 8
```

The MLX experiment loads external source explicitly without installing or vendoring it:

```sh
git clone https://github.com/matsuurajun/haas-mlx /tmp/haas-mlx
git -C /tmp/haas-mlx checkout cde96d263f56aad75c1de70282f71a91562092dd
uv run python notebooks/haas_ja_mlx.py --source /tmp/haas-mlx
```

Measured versions: sherpa-onnx 2.0.1, MLX 0.32.4, sentencepiece 0.2.3. Weight repository heads checked on the measurement date: ONNX `7f5ab0a4b1bf39db78afd6691eeecbc4f9d42e39`, MLX `15ef4f11618c6a1ee50e0eafaae13beded8ecdf9`. The bench fetches the repository's current head, so check these revisions when reproducing; output JSON also records file-size identity, environment and source audio hashes.

## Native application verification

Dito now uses the official `sherpa-onnx-sys` 2.0.1 static binding for both SenseVoice and Haas-ja. The former sherpa-rs package linked ONNX Runtime 1.17.1, whose Apple Silicon build cannot execute this model's quantized convolutions; that binding and the temporary compatibility guard have been removed. Models stay in their original ONNX format with their external data alongside them.

The native `asr_e2e` check passed for Haas-ja: verified files, real model loading, transcription with Japanese, English and Chinese hints, 35-second chunked audio, silence, stop and restart. The decoder reports no detected language. Language-focus metadata is descriptive; it never blocks input. SenseVoice was also checked with the new binding, preserving Japanese, English and Chinese language results. These are synthetic-input integration checks, not a new natural-speech accuracy evaluation.
