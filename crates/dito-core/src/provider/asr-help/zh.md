# Dito 语音识别模型配置规范 v1

本资料说明 Dito 接受的语音识别连接 JSON 格式及约束。附带的模板是参考示例，连接地址、鉴权方式、请求和响应格式取决于服务商的 API 规范。模型 ID 是由使用者指定的必填字符串，没有固定候选或默认值。

## 结构

- `version`：`1`。
- `connection`：`base_url` 和 `model`。基础 URL 使用 HTTP 或 HTTPS，可包含中转站路径前缀和端口，不包含最终请求路径、查询参数或片段。
- `language_map`：将 Dito 的语言代码映射为目标服务要求的值。
- `request`：`method`、`path`、`headers`、`encoding`，以及按需配置的 `query`、`body` 或 `fields`。
- `response`：提取识别文字的 `text_pointer`，以及可选的 `language_pointer`。

## 语言映射

语言来自通用设置中的全局语音优先语言，与界面显示语言相互独立。连接配置中没有独立的语言选择。

### Dito 当前实际传入的语言代码

| 全局语音优先语言设置 | 程序内部的值 | `language_map` 对应的 key |
| --- | --- | --- |
| 日语 | `ja` | `ja` |
| 英语 | `en` | `en` |
| 中文 | `zh` | `zh` |
| 自动识别 | 未指定（`null`） | 保留项 `auto` |

当前语音优先语言设置只会传入以上三个语言代码或“自动识别”状态。“自动识别”不会传入字符串 `auto`，而是让语言值保持未指定，再由映射层查找保留项 `auto`。`other` 是未知语言的兜底项，不是可选语言。

`ja-JP`、`en-US`、`zh-CN`、`JP`、`EN`、`Japanese` 等都不是当前设置会传入的内部代码。如果目标接口需要这些值，应放在映射右侧，例如 `"ja": "JP"`。配置格式也允许地区语言标签作为输入 key，但当前设置不会生成地区代码。中文的内部代码是 `zh`，不是 `cn`。

key 使用 `ja`、`en`、`zh`、`ja-JP` 等语言标签。`JP` 是国家代码，日语的输入 key 应使用 `ja` 或 `ja-JP`。value 按第三方要求填写，例如 `ja`、`JA`、`JP` 或 `Japanese`，输出值保留大小写。

匹配顺序为“完整语言标签 → 主语言 → `other`”。key 不区分大小写，不能存在冲突项。`other` 是必填的非空字符串，位于最后，用于未知语言。其他映射项允许使用 `null`。

保留状态 `auto` 与未知语言不同。未配置 `auto` 或配置为 `"auto": null` 时，省略语言字段；显式配置字符串时发送该值。接口没有语言参数时，无需语言字段。

## 请求

`method` 为 `POST` 或 `PUT`。`path` 是以 `/` 开头的相对 API 路径，不得填写完整 URL、查询参数或片段。将其追加到基础 URL，保留中转站路径前缀。例如 `https://relay.example.com/vendor/v1` 加 `/chat/completions` 得到 `https://relay.example.com/vendor/v1/chat/completions`。

`encoding` 为 `json` 时使用 `body`，为 `multipart` 时使用 `fields`，不能混用。JSON 请求体支持嵌套对象与数组。`query` 和 `fields` 的值可以是字符串、数值、布尔值或 `null`，请求头的值必须是字符串。Dito 自动设置 Content-Type 和 multipart boundary，并处理查询参数的 URL 编码与 JSON 字符串转义。

此格式仅支持同步 HTTP，不描述 WebSocket、任务轮询或多步上传。

## 变量

- `{{model}}`：`connection.model`。
- `{{language}}`：映射后的语言值。
- `{{api_key}}`：当前平台单独保存的 API 密钥。
- `{{audio.base64}}`：WAV 音频的原始 Base64，不含前缀。
- `{{audio.data_uri}}`：包含 `data:audio/wav;base64,` 前缀的音频字符串。
- `{{audio.file}}`：multipart 二进制 WAV，只能作为 `fields` 中的完整字段值。

配置中不包含真实 API 密钥或录音，鉴权使用 `{{api_key}}`。变量仅作固定替换，不执行表达式或脚本。未知变量应报错。

当对象字段的完整值为 `"{{language}}"`，且映射结果为 `null` 时，省略该字段。它不能用作字符串的一部分或数组元素。显式填写的 JSON `null` 保持原义。

## 响应

`text_pointer` 与可选的 `language_pointer` 使用 JSON Pointer。例如 `/text`、`/output/text`、`/choices/0/message/content`。数组索引用数字；`~` 转义为 `~0`，`/` 转义为 `~1`。识别结果必须是字符串，并去除两端空白。服务返回的语言不覆盖 Dito 的全局语言设置。

## 当前选择的参考模板
