# asr-lab

开源 ASR 模型本地验证环境（Apple Silicon / MLX + sherpa-onnx）。录音、保存、横向对比中文 / 日语 / 英语 / 混说场景的识别效果与本机速度。

## 快速开始

```bash
cd asr-lab
uv sync                      # 首次安装依赖
uv run jupyter lab notebooks # 打开 playground
```

- `00_录音台.ipynb`：麦克风录音（16 kHz 单声道），自动存 `recordings/`（WAV + JSON 元数据）
- `01_模型对比.ipynb`：跑合成测试集 + 自己的录音，输出准确率 / 耗时 / 实时倍率对比表
- 命令行冒烟（不进 notebook）：`uv run python -m asr_lab.bench`

首次运行会从 Hugging Face 下载约 6.5 GB 模型权重到 `models/`；录音与结果目录均被 git 忽略。

## 纳入对比的模型

| key | 模型 | 运行时 | 说明 |
|---|---|---|---|
| `sensevoice` | SenseVoice-Small (int8) | sherpa-onnx CPU | dito 现役引擎，zh/en/ja/ko/yue |
| `whisper-turbo` | Whisper large-v3-turbo | MLX GPU | 多语种基线，99 语种 |
| `whisper-large-v3` | Whisper large-v3 | MLX GPU | 精度更高的基线（可选） |
| `qwen3-asr-0.6b` | Qwen3-ASR-0.6B | MLX GPU | 快速档，30 语种 + 22 方言 |
| `qwen3-asr-1.7b` | Qwen3-ASR-1.7B | MLX GPU | 精度档，开源 SOTA 水平 |
| `r2t2` | Confucius4-R2T2 | MLX GPU | 网易有道的 Qwen3-ASR-1.7B 流式微调（需手动下载权重，见下） |

## R2T2（可选）

Confucius4-R2T2 权重就是标准 `qwen3_asr` 架构（官方也直接跑在 qwen-asr 运行时上），MLX 直接加载，实测与 `qwen3-asr-1.7b` 质量相当、同速，`context` 热词同样有效：

```bash
uv run hf download netease-youdao/Confucius4-R2T2 --local-dir models/r2t2
```

注意事项：

- 许可为[网易有道模型协议](https://github.com/netease-youdao/Confucius4-R2T2/blob/master/MODEL_LICENSE)：个人与中小规模商用免费；≥1 亿 MAU 或 ≥10 亿元营收需另行授权；不得用它改进其他 AI 模型；分发需保留协议与声明
- 语言检测字段经常误报 `English`（微调副作用，转写文本不受影响，需要语言判断时以 dito 侧逻辑或 qwen3-asr 原版为准）
- 日语句尾标点偶尔丢失
- 它独有的低延迟稳定前缀流式（80ms–2s chunk、追加式输出）需要官方 vLLM/CUDA 栈；MLX 下走的是 Qwen 的窗口重解码流式配方，实测可用但非其最佳形态

## 加一个新模型

在 `asr_lab/models.py` 里加一个类（实现 `transcribe(audio) -> Transcription`），注册进 `MODELS`，notebook 里 `run_benchmark(models=["your-key"])` 即可。

## 润色模型（Polisher）

`asr_lab/polish.py` 用 **dito 的真实润色提示词**（`crates/dito-core/src/prompts.rs`）+ llama.cpp（Metal，与 dito 运行时同源）对比 GGUF 候选；notebook `02_润色对比.ipynb` 可复跑。权重下载：

```bash
for spec in "unsloth/Qwen3-1.7B-GGUF qwen3-1.7b" "unsloth/Qwen3-4B-Instruct-2507-GGUF qwen3-4b" "unsloth/Qwen3.5-2B-GGUF qwen3.5-2b" "unsloth/Qwen3.5-4B-GGUF qwen3.5-4b"; do
  set -- $spec; uv run hf download "$1" --include "*Q4_K_M*" --local-dir "models/polish/$2"
done
```

M4 Pro 实测（Q4_K_M，6 条真实脏文本：SenseVoice 空格日语 / 口语改口 / 片假名混说 / Whisper 半角中文 / 中英混说 / 英语口语）：

| 模型 | 可用性 | 延迟 | 评价 |
|---|---|---|---|
| qwen3-1.7b（dito 目录最小档） | ❌ | ~2.3s 截断 | hybrid thinking 模型，输出全是 `<think>` 推理文本；**dito 切到它会把推理垃圾发给用户**，应移除或禁用 thinking |
| qwen3-4b-2507（dito 现默认） | ✅ | 0.32–0.49s | 稳；但中英混说会把英文词归化成中文（quick prototype→快速原型） |
| **qwen3.5-2b**（2026-03） | ✅ | **0.25–0.32s** | 质量与前代 4B 相当、最快；**保留英文术语**；英语口语偶尔轻微加戏 |
| **qwen3.5-4b**（2026-03） | ✅ | 0.48–0.63s | 质量最稳；**词典注入完美**（片假名按词表全转回拉丁词，与 ASR 热词形成闭环） |

选型建议：默认档 → `qwen3.5-4b`（质量 + 速度）；快速档 → `qwen3.5-2b`；`qwen3-1.7b` 从候选中移除。归化 vs 保留英文术语是产品口味，可在 dito 的 prompt 里加一句指令控制，不必换模型。

### 第二轮：Qwen3.5-9B / Qwen3.8-27B / MLX 运行时 / 低比特（2026-11-01 补充）

扩展矩阵（`MlxPolisher`，mlx-lm 4bit，同提示词同脏文本）：

| 配置 | 运行时 | 均延迟 | 词典注入 | 备注 |
|---|---|---|---|---|
| qwen3.5-4b-4bit (2.9GB) | MLX | **0.46s** | ✗（不转） | 质量与 GGUF 版一致 |
| qwen3.5-9b Q4_K_M (5.3GB) | llama.cpp | 0.96s | ✓ 完美 | |
| qwen3.5-9b Q3_K_M (4.4GB) | llama.cpp | 1.06s | ✓ 完美 | 质量几乎无损，但**并不更快**——Q3 反量化开销抵消了带宽收益 |
| qwen3.5-9b-4bit (5.6GB) | MLX | **0.81s** | ✓ 完美 | 比 GGUF 快 ~15%；ja 敬体最佳 |
| qwen3.8-27b-4bit (15GB) | MLX | 2.70s（6–7 t/s） | ✓ 完美 | **质量没有超过 9B 的证据**（改口句语气反而退步），听写场景出局 |

结论：

- **甜点是 9B@4bit**：相对 4B 的真实增益 = 词典注入稳定生效 + ja 敬体更自然；代价是延迟 ×2（MLX 0.8s / GGUF 0.96s）。**要「ASR 热词→润色词典」闭环，最小档就是 9B**——4B 的词典注入不稳定（GGUF 轮全转、MLX 轮未转，同权重不同量化/模板即翻车），9B 在两个运行时都稳定全转
- **「参数大比特小」实测为负收益**：9B Q3 不省时间；27B 4bit 无质量优势且 2.7s/条——参数量收益在 9B 已饱和，低于 4bit 的量化对 ≤12B 模型退化明显（与量化研究共识一致）
- **MLX vs llama.cpp**：同权重 MLX 全程更快（9B 快 15%，27B 差距更大）；但 dito 生产运行时是 llama.cpp（Rust），MLX 路线需换绑定，属于后续架构决策
- **集成坑**：Qwen3.5 的 chat 模板**默认开 thinking**（mlx-lm 必须显式 `enable_thinking=False`）；unsloth GGUF 内嵌模板默认非 thinking，两个运行时行为不同源
- **Qwen3.7 不存在开源版**（仅 API）；Qwen3.8 开源的是 27B dense 与 Max 2.4T MoE，小杯仍是 Qwen3.5
- **专用润色小模型**：HF 上没有能做「去口头语+改口+风格重写」三语通用的专用模型，只有窄任务件（标点恢复 BERT、ITN、填充词检测），至多做 LLM 前的廉价预处理，替代不了小 LLM

### 模型权重下载（第二轮）

```bash
# GGUF 9B（Q4 与 Q3）
uv run hf download unsloth/Qwen3.5-9B-GGUF --include "*Q4_K_M*" --local-dir models/polish/qwen3.5-9b
uv run hf download unsloth/Qwen3.5-9B-GGUF --include "*Q3_K_M*" --local-dir models/polish/qwen3.5-9b-q3
# MLX 4bit
for spec in "mlx-community/Qwen3.5-4B-MLX-4bit qwen3.5-4b-4bit" "mlx-community/Qwen3.5-9B-MLX-4bit qwen3.5-9b-4bit" "mlx-community/Qwen3.8-27B-4bit qwen3.8-27b-4bit"; do
  set -- $spec; uv run hf download "$1" --local-dir "models/polish-mlx/$2"
done
```

## 热词（专业词表）

Qwen3-ASR 原生支持**解码级**热词偏置：`context` 传空格分隔的词表，注入 system prompt（等价官方 API 的 hotword 机制，流式/批量同样支持）。Whisper 只能用 `initial_prompt` 软偏置；SenseVoice 无此机制（sherpa-onnx 的 `hr_dict` 是同音替换后处理，不是解码偏置）。

```python
qwen = load_model("qwen3-asr-1.7b")
qwen.transcribe(wav, context="桫椤 蕲春 岐伯 美卓乐 芪苈强心胶囊")
```

实测（生僻词句「岐伯山的桫椤要移栽到蕲春，顺便核对美卓乐和芪苈强心胶囊的处方。」，五个专业词全部踩同音陷阱）：

| 模型 | 无热词 | 有热词 |
|---|---|---|
| qwen3-asr-1.7b | 齐博山/梭罗/齐春/齐力强心（4 处错） | **全对** |
| qwen3-asr-0.6b | 同样 4 处错 | 岐伯山/桫椤/蕲春 全对，药名仍差 1 处 |
| whisper-turbo + initial_prompt | 4 处错 | 仅改善 1 处（芪苣≈芪苈） |
| sensevoice | 4 处错 | 不支持 |

## Qwen3-ASR 进 dito：llama.cpp 进程内通路（2026-11-01 定稿）

**结论：已完成 dito 生产集成**（`crates/dito-runtimes/src/llama.rs::Asr` + catalog 条目 `qwen3-asr-0.6b` / `qwen3-asr-1.7b`）。无需 llama-server 子进程、无新增二进制分发：`llama-cpp-sys-2` 0.1.160 自带的 vendored llama.cpp 已含 qwen3_asr 音频前端（`mtmd_audio_preprocessor_qwen3a`），给 `llama-cpp-2` 开 `mtmd` feature 即可用全 safe API（`MtmdBitmap::from_audio_data` → `tokenize` → `eval_chunks`）。Whisper large-v3-turbo 已从 dito 目录移除（runtime 从未编入，属死选项；目录现为 SenseVoice / 0.6B / 1.7B 三档听写阵容）。

权重：官方 `ggml-org/Qwen3-ASR-{0.6B,1.7B}-GGUF`（只有 Q8_0 与 bf16，无 Q4）——0.6B 804MB+214MB ≈ 1GB、1.7B 2.16GB+356MB ≈ 2.5GB，sha256 均已从 HF tree API 实拉入 catalog 并与下载件核对一致。

M4 Pro 实测（16 kHz 单声道，三条路线对拍）：

| 路线 | 单条延迟 | 备注 |
|---|---|---|
| llama-server 常驻（HTTP，实验用） | 0.14–0.24s | `/v1/chat/completions` + `input_audio` |
| **dito 进程内 0.6B（生产路径，每条新 context）** | **0.16–0.25s** | e2e `local_asr_e2e`，zh/ja/en/混说全对 |
| **dito 进程内 1.7B（高精度档）** | **0.32–0.49s**（首条 ~1s 为 MTL 冷分配） | 5/5 全对；官方仅 Q8_0（2.16GB+356MB） |
| 25.9s 长音频（进程内 0.6B） | 0.56s | 4/4 句完整 |

关键细节：
- prompt 形态 `<|im_start|>user\n<__media__>Transcribe the audio.<|im_end|>\n<|im_start|>assistant\n`，热词走 `<|im_start|>system\nContext: …` 注入（dito 的 `hotwords` 参数直通词典）
- 输出为 `language Chinese<asr_text>正文`，runtime 剥壳并把语言名映射为 `zh/en/ja`
- **长音频**：llama.cpp #21847（>60s 早停/空输出）已于 2026-05-15 上游修复（commit `72e60f5`，PR #23073，qwen3a 预处理 30s 分块）；llama-cpp-sys-2 0.1.160 vendored 的树已含修复。2026-11-03 复测（dito 进程内、336.7s/5.6min 真实语速合成音频，1394 字）：1.7B 单趟完整、CER 0.002、16.4s；0.6B CER 0.003、9.6s。dito 侧 45s 截断已移除（上限改为对齐录音侧 10 分钟）；音频 token 密度实测 ~13.5 tokens/s（25.9s → 353 tokens），n_ctx 按此动态计算
- **SenseVoice 长音频**：整段直喂超长音频会丢标点、丢音节（336.7s 实测 CER 0.188）；dito 运行时按静音帧切成 ≤28s 段转写后拼接，同音频 CER 降至 0.026、耗时反而更短（11.2s vs 31.2s）

复跑：`cargo run -p dito-runtimes --example local_asr_e2e -- models/qwen3-asr-0.6b audio/zh_01.wav "热词1,热词2"`

## 目录

```
asr_lab/        录音、测试集合成、模型适配、基准
notebooks/      playground
recordings/     麦克风录音（git 忽略）
audio/          say 合成测试集 + manifest（wav 忽略，manifest 入库）
models/         下载的权重（git 忽略）
results/        基准结果 csv（git 忽略）
```
