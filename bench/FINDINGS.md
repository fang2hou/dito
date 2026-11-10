# Findings

Conclusions from the model evaluations that shaped Dito's catalog and defaults.

These numbers were measured on an M4 Pro in November 2026, before the result store existed, so they are not backed by files in `results/`. To reproduce or extend them, run `mise run bench asr run` or `mise run bench polish run`; new measurements are then stored per chip and can be compared with the figures below.

## ASR models

| Key | Model | Runtime | Notes |
|---|---|---|---|
| `sensevoice` | SenseVoice-Small (int8) | sherpa-onnx CPU | Dito's shipping engine. zh, en, ja, ko, yue |
| `whisper-turbo` | Whisper large-v3-turbo | MLX GPU | Multilingual baseline, 99 languages |
| `whisper-large-v3` | Whisper large-v3 | MLX GPU | More accurate baseline (optional) |
| `qwen3-asr-0.6b` | Qwen3-ASR 0.6B | MLX GPU | Fast tier. 30 languages and 22 Chinese dialects |
| `qwen3-asr-1.7b` | Qwen3-ASR 1.7B | MLX GPU | Accuracy tier, state of the art among open models |
| `r2t2` | Confucius4-R2T2 | MLX GPU | NetEase Youdao's streaming fine-tune of Qwen3-ASR-1.7B (weights downloaded separately) |

### Confucius4-R2T2

The weights use the standard `qwen3_asr` architecture (the vendor also runs it on the qwen-asr runtime), so MLX loads it directly. Measured quality and speed match `qwen3-asr-1.7b`, and the `context` hotwords work the same way.

- License: [NetEase Youdao model license](https://github.com/netease-youdao/Confucius4-R2T2/blob/master/MODEL_LICENSE). Free for individuals and small to mid-size commercial use; separate authorization is required at 100 million MAU or 1 billion CNY revenue or more; the model must not be used to improve other AI models; redistribution must keep the license and notices.
- The language detection field often reports `English`, a side effect of the fine-tune. The transcript itself is unaffected. When the language matters, rely on Dito's own logic or on the original qwen3-asr.
- Japanese sentence-final punctuation is occasionally dropped.
- Its low-latency, stable-prefix streaming (80 ms to 2 s chunks, append-only output) needs the vendor's vLLM/CUDA stack. On MLX it follows Qwen's windowed re-decoding recipe, which works but is not its best form.

### Hotwords

Qwen3-ASR supports decoding-level hotword biasing: the `context` string is a space-separated word list injected into the system prompt, which is equivalent to the hotword mechanism of the official API and works for streaming and batch alike. Whisper only has a soft bias through `initial_prompt`. SenseVoice has no such mechanism (`hr_dict` in sherpa-onnx is a homophone replacement applied after decoding, not decoding bias).

Test sentence, with all five terms chosen to trigger homophone mistakes (the clip `zh_hotwords_01`):

| Model | Without hotwords | With hotwords |
|---|---|---|
| qwen3-asr-1.7b | 4 errors | All correct |
| qwen3-asr-0.6b | Same 4 errors | 3 of the terms correct, 1 medicine name still wrong |
| whisper-turbo with `initial_prompt` | 4 errors | Only 1 improved |
| sensevoice | 4 errors | Not supported |

### Qwen3-ASR in Dito: the in-process llama.cpp path (settled 2026-11-01)

**Production integration is done** (`crates/dito-runtimes/src/llama.rs::Asr`, catalog entries `qwen3-asr-0.6b` and `qwen3-asr-1.7b`). No llama-server subprocess and no extra binary to ship: the vendored llama.cpp in `llama-cpp-sys-2` 0.1.160 already contains the qwen3_asr audio front end (`mtmd_audio_preprocessor_qwen3a`), so enabling the `mtmd` feature of `llama-cpp-2` is enough to use the safe API (`MtmdBitmap::from_audio_data`, `tokenize`, `eval_chunks`). Whisper large-v3-turbo was removed from Dito's catalog because its runtime was never compiled in. The dictation lineup is now SenseVoice, 0.6B and 1.7B.

Weights are the official `ggml-org/Qwen3-ASR-{0.6B,1.7B}-GGUF`, which only come in Q8_0 and bf16 (no Q4). 0.6B is 804 MB plus 214 MB (about 1 GB); 1.7B is 2.16 GB plus 356 MB (about 2.5 GB). The sha256 values in the catalog were taken from the Hugging Face tree API and match the downloaded files.

Measured on an M4 Pro (16 kHz mono, three routes compared):

| Route | Latency per clip | Notes |
|---|---|---|
| llama-server resident (HTTP, experiment only) | 0.14 to 0.24 s | `/v1/chat/completions` with `input_audio` |
| **Dito in-process 0.6B (production path, new context per clip)** | **0.16 to 0.25 s** | e2e `local_asr_e2e`, zh, ja, en and mixed all correct |
| **Dito in-process 1.7B (high accuracy tier)** | **0.32 to 0.49 s** (first clip about 1 s from cold Metal allocation) | 5 of 5 correct; official Q8_0 only |
| 25.9 s long audio (in-process 0.6B) | 0.56 s | 4 of 4 sentences complete |

Details:

- Prompt shape: `<|im_start|>user\n<__media__>Transcribe the audio.<|im_end|>\n<|im_start|>assistant\n`. Hotwords are injected as `<|im_start|>system\nContext: ...` (Dito's `hotwords` parameter passes the wordbook straight through).
- Output looks like `language Chinese<asr_text>body`. The runtime strips the wrapper and maps the language name to `zh`, `en` or `ja`.
- **Long audio**: llama.cpp #21847 (early stop or empty output beyond 60 s) was fixed upstream on 2026-05-15 (commit `72e60f5`, PR #23073, the qwen3a preprocessor splits into 30 s chunks). The tree vendored by llama-cpp-sys-2 0.1.160 contains the fix. Retest on 2026-11-03 (Dito in-process, 336.7 s of real-pace synthesized audio, 1394 characters): 1.7B finished in a single pass, CER 0.002, 16.4 s; 0.6B CER 0.003, 9.6 s. Dito's 45 s truncation was removed (the limit now matches the 10 minute recording limit). Audio token density is about 13.5 tokens/s (25.9 s gave 353 tokens), and `n_ctx` is computed from it.
- **SenseVoice long audio**: feeding very long audio in one piece drops punctuation and syllables (CER 0.188 at 336.7 s). Dito's runtime splits at silent frames into segments of at most 28 s and joins the transcripts, which brings the same audio to CER 0.026 and makes it faster (11.2 s versus 31.2 s).

Rerun: `cargo run -p dito-runtimes --example local_asr_e2e -- models/qwen3-asr-0.6b audio/zh_01.wav "hotword1,hotword2"`

## Polish models

The polish harness uses Dito's real prompts (`crates/dito-core/src/prompts.rs`) with candidate models through llama.cpp (Metal, the same family as Dito's runtime) or mlx-lm. Weights are fetched with `mise run bench polish download <key>`.

### Round one: GGUF candidates

Q4_K_M on an M4 Pro, six dirty texts: SenseVoice Japanese with spaces, a spoken self-correction, katakana mixed speech, Whisper half-width Chinese, mixed Chinese and English, and spoken English.

| Model | Usable | Latency | Verdict |
|---|---|---|---|
| qwen3-1.7b (Dito's smallest tier at the time) | No | about 2.3 s, truncated | A hybrid thinking model: the output is all `<think>` reasoning text. **Switching Dito to it would send reasoning junk to the user**; remove it or disable thinking |
| qwen3-4b-2507 (Dito's default at the time) | Yes | 0.32 to 0.49 s | Stable, but it normalizes English words in mixed speech into Chinese (quick prototype becomes 快速原型) |
| **qwen3.5-2b** (2026-03) | Yes | **0.25 to 0.32 s** | Quality comparable to the previous 4B and the fastest; **keeps English terms**; occasionally embellishes spoken English slightly |
| **qwen3.5-4b** (2026-03) | Yes | 0.48 to 0.63 s | Most stable quality; **wordbook injection works perfectly** (katakana is converted back to Latin words from the word list, closing the loop with the ASR hotwords) |

Recommendation: default tier `qwen3.5-4b` (quality and speed), fast tier `qwen3.5-2b`, drop `qwen3-1.7b` from the candidates. Normalizing versus keeping English terms is a product taste; a sentence in Dito's prompt can control it without changing the model.

### Round two: Qwen3.5-9B, Qwen3.8-27B, MLX and low-bit quantization (2026-11-01)

Extended matrix (MLX with mlx-lm 4-bit, same prompts and dirty texts):

| Configuration | Runtime | Mean latency | Wordbook injection | Notes |
|---|---|---|---|---|
| qwen3.5-4b 4-bit (2.9 GB) | MLX | **0.46 s** | No (not converted) | Same quality as the GGUF version |
| qwen3.5-9b Q4_K_M (5.3 GB) | llama.cpp | 0.96 s | Yes, perfect | |
| qwen3.5-9b Q3_K_M (4.4 GB) | llama.cpp | 1.06 s | Yes, perfect | Almost no quality loss, but **not faster**: dequantizing Q3 cancels the bandwidth gain |
| qwen3.5-9b 4-bit (5.6 GB) | MLX | **0.81 s** | Yes, perfect | About 15% faster than GGUF; best Japanese polite style |
| qwen3.8-27b 4-bit (15 GB) | MLX | 2.70 s (6 to 7 tokens/s) | Yes, perfect | **No evidence of better quality than 9B** (self-correction sentences got worse in tone); out for dictation |

Conclusions:

- **The sweet spot is 9B at 4-bit.** Compared with 4B, the real gains are reliable wordbook injection and a more natural Japanese polite style, at twice the latency (MLX 0.8 s, GGUF 0.96 s). **The ASR hotwords to polish wordbook loop needs 9B at minimum**: 4B's injection is unreliable (all converted in the GGUF round, none converted in the MLX round, same weights with a different quantization or template), while 9B converts everything on both runtimes.
- **Bigger model with fewer bits measured as a net loss.** 9B Q3 saves no time, and 27B at 4-bit has no quality edge at 2.7 s per clip. The benefit of parameter count is saturated at 9B, and quantizing below 4-bit visibly degrades models of 12B or less, in line with the quantization literature.
- **MLX versus llama.cpp**: with the same weights MLX is faster throughout (15% for 9B, more for 27B). Dito's production runtime is llama.cpp (Rust), so an MLX route needs a different binding, which is a later architecture decision.
- **Integration traps**: the Qwen3.5 chat template **enables thinking by default** (mlx-lm needs an explicit `enable_thinking=False`), while the chat template embedded in the unsloth GGUF defaults to non-thinking, so the two runtimes behave differently out of the box.
- **Qwen3.7 has no open release** (API only). Qwen3.8 open-sources a 27B dense model and the Max 2.4T MoE; the small tier is still Qwen3.5.
- **Dedicated small polish models**: there is no open model on Hugging Face that handles filler removal, self-correction and style rewriting across the three languages. Only narrow tools exist (punctuation restoration BERT, ITN, filler detection), which at best serve as cheap preprocessing before the LLM and cannot replace a small LLM.
