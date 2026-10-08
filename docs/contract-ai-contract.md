# 合同 AI 提取接入合同

## 1. 所有权与协议

1. `erp-contract::ports::recognition::ContractExtractor` 保持供应商无关。SDK 依赖仅归组合层 `erp-processes`，不得进入领域 crate。
2. `contract_import::openai::OpenAiContractExtractor` 使用锁定的 `rig-core 0.43.0` OpenAI provider 编解码，发送一次非流式 `POST <base_url>/responses`，设置 `store=false`，不创建或续接服务端会话。不得回退到 Chat Completions、启用工具、自动重试或自动降低输出约束。
3. 提取模型须支持 Responses API 的 `text.format.type=json_schema` 与 `text.format.strict=true` 结构化输出，且上下文容量足以容纳合同全文、固定提示词、Schema 与输出。仅声明 OpenAI 兼容不代表满足此要求。服务不支持时任务失败，不得退回自由文本或人工输入。
4. 请求 Schema 仅使用对象、数组、字符串、整数、枚举、必填项及禁止额外属性的基础约束。不得发送微调模型不支持的 `minLength/maxLength/minimum/maximum/minItems/maxItems`；字段数量、字节长度、页号范围和非空校验必须在本地执行，超限仍须失败。

## 2. 配置与启用

配置须写入现有 SafeConfig 文件或 Nacos，禁止读取独立环境变量或提交真实 API key。

```toml
[contract_ai]
provider_id = "contract-ai-primary"
base_url = "https://your-provider.example/v1"
api_key = "<API key>"
model = "<provider model ID>"
timeout_seconds = 90
max_output_tokens = 8192
```

1. `base_url` 填服务 API 前缀，禁止包含 `/responses` 或 `/chat/completions` 后缀、用户名、密码、查询串或 fragment。远程服务须使用 HTTPS；`localhost`、`127.0.0.1`、`[::1]` 可使用 HTTP。请求不得跟随重定向。DeepSeek 使用 `https://api.deepseek.com`；其它供应商按其 Responses API 前缀填写。
2. `api_key` 必填且不得含空白或控制字符。`model` 必填、使用供应商模型 ID，不设置默认模型。缺少必填配置或配置非法时配置加载失败。
3. 超时默认 90 秒，允许 1 至 120 秒；`max_output_tokens` 默认 8192，允许 256 至 32768，包含供应商计入的推理 token 与正文 token。OCR 与 AI 仍共用 180 秒总预算，以先到达的期限为准。连接超时 5 秒，成功响应正文最多 1 MiB，提取 JSON 与领域结果分别最多 256,000 字节。
4. 整节缺省时使用 `UnconfiguredRecognition`，以 `AI_NOT_CONFIGURED` 拒绝提取。新任务使用当前配置快照，运行中的任务保留原快照。启用完整导入还须配置 `[aliyun_ocr]` 及 Poppler。
5. `provider_id` 必填，长度为 1 至 64 个 ASCII 字符，仅允许字母、数字、`-`、`_`、`.`。使用非敏感、稳定且能区分供应商/网关连接的标识，禁止填写凭据。切换供应商或网关时必须分配新标识；仅轮换同一服务的 API key 时保留标识。已启用 AI 的配置须补齐此项，否则配置加载失败。

## 3. 数据与失败处理

1. 必须在数据库事务外执行模型调用。发送已验证的全部 OCR 页面数组，包括空白页和原页号，不截断、摘要、分批取舍或修补原文。
2. 固定系统提示词通过 `instructions` 传入，只声明提取规则；合同全文通过 `input` 的用户消息传入，作为不可信数据。禁止传入 ERP 主键、主数据候选、凭据或可执行工具。
3. 输出仅接收 `fields` 列表与 `conflicts`。每个字段含 `field/value/page/quote`，仅允许合同领域定义的 14 种字段，重复字段、未知字段和额外属性一律失败。缺失原文须省略字段；不得猜测、补默认值、修正文字符号或从复合条款中截取可匹配片段。
4. 响应顶层 `status` 必须为 `completed`，且 `error` 与 `incomplete_details` 为空。`output` 必须包含唯一且 `status=completed` 的 assistant 消息，只合并其 `output_text` 内容；允许附带已完成或未声明状态的 reasoning 项，但其内容不得进入字段或证据。输出截断、拒答、工具调用、未知输出项、未完成消息、多个正文消息、Markdown 包裹、非法 JSON、越界页号或超限值须失败，不自动修复。原文引文、必需字段、冲突、条款、日期与主数据匹配继续执行[识别导入合同](contract-recognition-contract.md)。通过 JSON Schema 不等于业务准入成功。
5. 提取证据的 `provider` 必须取自任务配置快照中的 `provider_id`，随原有任务与合同修订保存，不发送给模型。`version` 由适配器生成，包含 `protocol=responses`、配置请求模型、响应报告模型及提示词版本 `contract-v1`；模型未报告版本时标为 `unreported`。供应商响应不得覆盖配置的供应商标识，生成内容不得自报供应商或版本。服务切换不得改写历史证据；历史 `openai-compatible` 供应商与 `protocol=openai` 版本记录保留原值，不得推断或补填供应商。
6. 失败保留原任务和已完成的有界阶段结果。没有自动重试；用户手动重试重新执行 OCR 与 AI，可能再次计费。超时取消本地等待，不保证供应商已取消已接收请求或免计费。
7. 日志内容不设限制，允许记录完整请求、响应、OCR、模型输出、原始错误、配置、凭据及调用上下文。本合同不要求日志脱敏、截断、字段白名单或内容过滤。适配器开启 SDK 内容追踪，SDK 日志须关联当前任务上下文。
8. 后台任务须建立 `contract_import` 日志上下文，包含 `task_id`、`account`、`request_id`。任务开始记录 `contract_import_started`，持久化后的业务失败须以 WARN 记录 `contract_import_finished`、`error_code`、`elapsed_ms`；持久化异常或提交结果未知须以 ERROR 记录，不得仅因 HTTP 返回 200 判定识别成功。
9. OCR 开始与完成须记录 `contract_ocr_started` / `contract_ocr_finished`、页数和耗时。识别阶段失败须记录 `contract_recognition_failed`、`stage`（`ocr` / `ai` / `validation`）、错误码、总耗时与当前阶段耗时；`RECOGNITION_TIMEOUT` 表示 180 秒总预算耗尽。
10. AI 调用须记录 `contract_ai_started` / `contract_ai_finished`，通过 `contract_ai` 上下文关联 `provider_id`、`model`、`base_url`、`protocol=responses`、`response_format`、`strict`、页数、超时秒数和输出 token 上限。结束日志须包含结果、耗时以及已取得的 HTTP 状态和供应商请求 ID；失败须以 WARN 记录错误码。`timeout_source` 区分 `application_deadline`（应用等待截止）、`connect`（连接超时）、`response_headers`（发送请求或等待响应头超时）、`response_body`（读取响应超时）、`upstream_http`（上游 408/504）。总预算取消 AI 时，以识别阶段的 `RECOGNITION_TIMEOUT` 为准，不得伪报供应商超时。
11. HTTP 失败须完整读取错误文本，在 `contract_ai_finished` 记录 `provider_error_body`、全部 `provider_response_headers` 及可用的 `provider_sdk_error`，不另设日志正文大小限制、不截断。JSON 错误须同时展开 `provider_error_type`、`provider_error_code`、`provider_error_param`、`provider_error_message`；保留供应商原值，不使用固定摘要替换。非 JSON 或 JSON 解析失败仍保留原始文本。供应商请求 ID 按 `x-request-id`、`x-dashscope-request-id`、`x-acs-request-id` 顺序提取，不限制值的长度或字符；其他头仍完整记录。
12. `provider_error_body_state` 标记正文已收到（`received`）、读取失败（`read_failed`）或调用期限到达前尚未读完（`pending`）；读取失败须记录原始 `provider_error_read_error`。诊断正文读取沿用既有请求超时与总预算，不额外发送请求或自动重试。已经取得非成功 HTTP 状态时，正文读取失败或 AI 调用期限到达均按该状态分类，不得把已知的 HTTP 400/401 等改报成等待响应超时。

| 错误码 | 行为 |
| --- | --- |
| `AI_NOT_CONFIGURED` | 补齐配置后手动重试 |
| `AI_UNAUTHORIZED` | 检查 API key、账户和模型授权 |
| `AI_THROTTLED` | 等待限流恢复后手动重试 |
| `AI_TIMEOUT` / `RECOGNITION_TIMEOUT` | 检查模型耗时、合同体积及总预算 |
| `AI_UNAVAILABLE` | 检查连接及供应商可用性 |
| `AI_REJECTED` | 检查 Responses API、模型、上下文容量及 JSON Schema 支持 |
| `AI_INVALID_OUTPUT` | 输出不完整或不满足协议，禁止归档 |

## 4. 验证与回退

1. 库单元测试须通过真实 Rig provider 与内存 HTTP 替身执行协议编解码，覆盖 `/responses` 路由、`instructions/input/text.format/max_output_tokens/store` 请求字段、完整页面、实际请求 Schema 基础子集、本地范围限制、配置供应商标识的证据序列化、结构化输出、reasoning 与正文隔离、完成状态、拒答、额外输出项、证据校验、冲突、缺失、重复、异常响应、无重试、超时取消及错误正文完整记录。不得使用真实计费接口作为单元测试。
2. 运行 `cargo check --workspace --locked`、受影响 crate 的库测试与 Clippy，以及领域依赖边界检查。不得执行真实 MongoDB/S3 集成测试。
3. 实际供应商上线前须验证 Responses API 路由、严格结构化输出兼容性、真实合同识别质量及跨页冲突检测。静态门禁和协议替身测试不得替代此项验收。
4. 删除 `[contract_ai]` 配置即可停用后续任务的 AI 提取。回退不得删除历史合同修订、原文件、任务或识别证据。

供应商协议依据：[DeepSeek Responses API](https://api-docs.deepseek.com/zh-cn/api/create-response/)、[Responses 兼容性](https://api-docs.deepseek.com/guides/responses_api/)。

SDK 依据：[rig-core 0.43.0](https://docs.rs/rig-core/0.43.0/rig_core/)、[OpenAI provider](https://docs.rs/rig-core/0.43.0/rig_core/providers/openai/index.html)。
