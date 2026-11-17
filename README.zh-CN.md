<p align="center">
  <img src="assets/readme/header.png" alt="Dito" width="440">
</p>

<p align="center">
  <strong>一款本地模型优先的 macOS 语音助手</strong>
</p>

<p align="center">
  Language: <a href="README.md">English</a> / <a href="README.ja.md">日本語</a> / <strong>简体中文</strong> / <a href="README.ko.md">한국어</a>
</p>

语音输入、润色、改写、翻译、摘要、回复、提问和会议转写，都可以通过 Dito 完成。  
Dito 优先使用本地模型，所有历史、音频和模型都留在你的 Mac 上，不必担心数据外泄。  
你也可以按需接入云端模型、修改提示词，满足不同的自定义需求。

https://github.com/user-attachments/assets/07a4853a-fd79-4747-b3f1-251a711681ee

> [!NOTE]
> Dito 目前处于快速迭代的早期阶段，暂不提供预编译安装包，需从源码构建。构建和安装已简化为一条命令，按下方步骤运行脚本即可完成。

## 安装

需要配备 Apple 芯片的 Mac 和 macOS 13 或更高版本（录制系统音频需要 macOS 14.6 或更高版本）。

### 构建环境

> [!TIP]
> 如果尚未配置下述必要的开发工具链，安装 [Homebrew](https://brew.sh/) 后直接运行下方的安装脚本即可，脚本会协助安装缺少的工具。

1. [Rust](https://www.rust-lang.org/learn/get-started) 1.98 或更高版本。
2. CMake。
3. [Apple 命令行工具](https://developer.apple.com/xcode/features/)（如编译着色器时提示缺少 Metal 工具链，可运行 `bash scripts/setup-macos.sh` 安装）。

### 执行安装脚本

在代码仓库目录中运行：

```bash
bash scripts/install-local.sh
```

## 亮点功能

### 1. 语音输入

按下快捷键，说话，再按一次。Dito 会自动识别语音，并将润色后的文字输入到当前光标处。

你可以自定义语音识别模型、文本模型、单词本和提示词，也可以为每个应用单独设置润色规则。  
如果你手头有整理好的工作文档，Dito 还能用模型从中提取专有词加入单词本，让语音识别更准确。

<p align="center"><img src="assets/readme/dictation-zh.webp" alt="HUD 依次聆听、识别、润色，句子出现在光标处" width="720"></p>

### 2. 快速命令

在任何应用里选中文字，按下快捷键，说出你的指令——无论是润色、改写、翻译、摘要、回复还是提问，Dito 都能直接处理。

Dito 内置了针对本地小模型拟合的意图分路器，无需联网即可根据指令判断处理方式。  
如果不方便说话，把鼠标移到 HUD 上即可切换为手动输入指令。

<p align="center"><img src="assets/readme/command-zh.webp" alt="用语音改写选中的句子，并就地替换" width="720"></p>

### 3. 转写会议

在“工具”窗口中，你可以录制 Mac 的系统音频、麦克风，或两者同时录制，实时生成转写文本，并在结束后按上下文自动润色、生成会议标题。段落支持就地编辑和一键复制。

<p align="center"><img src="assets/readme/meeting-zh.webp" alt="在“工具”窗口里实时转写会议，停止后润色" width="720"></p>

### 4. 快速聊天与本地 API

聊天默认共用你选定的文本模型，支持附上文件或已结束的会议记录直接提问，流式输出 Markdown、代码和表格。使用本地模型时，对话全程留在你的 Mac 上。你还可以开启“本地 API”，通过兼容 OpenAI 的接口和 MCP 将本地模型共享给本机的其他应用。

<p align="center"><img src="assets/readme/chat-zh.webp" alt="询问下次会议前要做的事，本地模型以列表形式流式回答" width="720"></p>

### 其他功能

| | |
| --- | --- |
| **按应用润色** | 在每种语言的通用规则之上，可为任意应用单独叠加润色规则与写法示例 |
| **单词本** | 业务术语容易识别错？支持导入、导出热词，也能从粘贴的文字、文件或其他 AI 的回答中解析候选词 |
| **本地与云端自由组合** | 语音识别和文本处理可分别选择本地模型或云端配置，互不绑定 |
| **不丢内容，随时重算** | 润色失败时自动保底输入识别原文；每次结果与录音都保存在历史中，不满意可随时换模型重新识别或重新润色 |
| **统计与多语言界面** | 统计节省的时间、字数与语速；支持简繁中文、日语、英语、韩语、蒙古语和法语七种界面语言 |
| **默认私密** | 历史、音频和模型都留在你的 Mac 上；云端 API 密钥保存在 macOS 钥匙串中，绝不写入配置文件 |

## 模型

> [!NOTE]
> 所有本地模型均可在 Dito 内按需下载和管理，支持断点续传并在下载后自动校验完整性。

- 语音识别（ASR）
  - 本地模型
    - Qwen3-ASR 1.7B（默认）
    - Qwen3-ASR 0.6B
    - SenseVoice Small
    - Haas-ja v1.0
    - Confucius4-R2T2
    - Fun-ASR-MLT-Nano-2512
    - Whisper large-v3
    - Whisper large-v3-turbo
    - Nemotron 3.5 Transcribe 30B-A4B
  - 云端服务
    - FunASR
    - Qwen ASR
    - OpenRouter
    - 自定义请求（通过内置 JSON 编辑器配置）
- 文本处理（润色、命令、聊天）
  - 本地模型
    - Qwen3.5 Instruct 2B
    - Qwen3.5 Instruct 4B
    - Qwen3.5 Instruct 9B（默认）
    - Qwen3.8 27B
  - 云端服务
    - Azure OpenAI
    - Amazon Bedrock
    - 任意 OpenAI 兼容 API
- 发话检测（会议）
  - 本地模型
    - Silero VAD

## 开发

```bash
mise install                 # 按固定版本安装 Rust、Python、uv 和 ruff
mise run dev:mock            # 用模拟的云端、麦克风和数据运行整个应用
mise run dev                 # 使用真实的麦克风、模型和服务商
mise run check               # 格式检查和快速测试
mise run lint:rust           # clippy（也包含带 mock 和 screenshots 功能的构建）
mise run readme:assets       # 重新生成 README 里的动画（需先构建带 screenshots 功能的二进制）
```

Dito 是基于 [GPUI](https://www.gpui.rs/) 的 Rust 工作区。界面先在 `design/dito.figcraft` 里设计，代码再跟进。

| 目的 | 阅读 |
| --- | --- |
| 构建、用模拟环境运行、测试 | [docs/IMPLEMENTATION.md](docs/IMPLEMENTATION.md) |
| 本地运行、命令行参数、排查问题 | [docs/DEBUGGING.md](docs/DEBUGGING.md) |
| 架构与决策 | [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)、[ADR](docs/adr/README.md) |
| 术语以及各概念所在位置 | [docs/GLOSSARY.md](docs/GLOSSARY.md) |
| 应用做什么、为什么 | [design/features.md](design/features.md) |
| 界面与动效设计 | [design/README.md](design/README.md) |
| 与 AI 编程助手协作 | [AGENTS.md](AGENTS.md) |

## 许可证

[MIT](LICENSE)
