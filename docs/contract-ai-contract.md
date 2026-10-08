# 合同 AI 提取接入合同

## 1. 所有权与协议

1. `erp-contract::ports::recognition::ContractExtractor` 保持供应商无关。SDK 依赖仅归组合层 `erp-processes`，不得进入领域 crate。
2. `contract_import::openai::OpenAiContractExtractor` 使用锁定的 `rig-agent 0.43.0` 标准 `ExtractorBuilder<Output>` 与 `extract`，由 `rig-core 0.43.0` OpenAI provider 编解码。每次提取仅发送一次非流式 `POST <base_url>/responses`，设置 `store=false`、`parallel_tool_calls=false`、`retries(0)`，不创建或续接服务端会话，不自动重试或回退到 Chat Completions。
3. 提取模型须支持 Responses API 的严格函数调用及 `tool_choice=required`。唯一允许的函数为 Rig 内置输出函数 `submit`，仅承载提取数据，不注册业务工具、不读写主数据、不执行外部动作。模型上下文容量须容纳合同全文、固定提示词、函数 Schema 与输出。服务不支持时任务失败，不得退回自由文本或人工输入。
4. 提取 DTO 派生 `Serialize`、`Deserialize`、`JsonSchema`，由 Rig 生成 `submit.parameters`，通过 OpenAI provider 的 `with_strict_tools` 适配严格 Schema。禁止另行手写 Schema 或自行解析 `submit.arguments`；字段数量、字节长度、页号范围和非空校验在本地执行，超限仍须失败。SDK 依赖不得进入领域 crate。
5. 不发送供应商专用思考参数，推理行为使用配置模型的默认值。`max_output_tokens` 同时覆盖供应商计入的推理和结果 token；额度耗尽且输出未完成时失败，不自动扩额或重试。

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

1. `base_url` 填服务 API 前缀，禁止包含 `/responses` 或 `/chat/completions` 后缀、用户名、密码、查询串或 fragment。远程服务须使用 HTTPS；`localhost`、`127.0.0.1`、`[::1]` 可使用 HTTP。请求不得跟随重定向。OpenAI 官方服务使用 `https://api.openai.com/v1`；其它服务按实际 Responses API 前缀填写。
2. `api_key` 必填且不得含空白或控制字符。`model` 必填、使用接入端接受的模型 ID，按配置原样传给 Rig，不设置默认模型，不添加或移除模型名前缀。缺少必填配置或配置非法时配置加载失败。
3. 超时默认 90 秒，允许 1 至 120 秒；`max_output_tokens` 默认 8192，允许 256 至 32768，包含供应商计入的推理 token 与正文 token。OCR 与 AI 仍共用 180 秒总预算，以先到达的期限为准。连接超时 5 秒，成功响应正文最多 1 MiB，提取 JSON 与领域结果分别最多 256,000 字节。
4. 整节缺省时使用 `UnconfiguredRecognition`，以 `AI_NOT_CONFIGURED` 拒绝提取。新任务使用当前配置快照，运行中的任务保留原快照。启用完整导入还须配置 `[aliyun_ocr]` 及 Poppler。
5. `provider_id` 必填，长度为 1 至 64 个 ASCII 字符，仅允许字母、数字、`-`、`_`、`.`。使用非敏感、稳定且能区分供应商/网关连接的标识，禁止填写凭据。切换供应商或网关时必须分配新标识；仅轮换同一服务的 API key 时保留标识。已启用 AI 的配置须补齐此项，否则配置加载失败。

## 3. 数据与失败处理

1. 必须在数据库事务外执行模型调用。发送已验证的全部 OCR 页面数组，包括空白页和原页号，不截断、摘要、分批取舍或修补原文。
2. 合同提示词通过 `append_preamble` 追加到 Rig 标准提取提示词，由 provider 写入 `instructions`。必须通过 `submit` 提交 `fields/conflicts`，禁止重复字段、空字符串或占位值；原文缺失或无法确定的字段从列表省略。同页或跨页条款存在无法确定关系的不同付款约定时，省略该字段并记录冲突。合同全文通过 `input` 的用户消息传入，作为不可信数据。禁止传入 ERP 主键、主数据候选、凭据或业务工具。
3. 输出仅接收 `fields` 列表与 `conflicts`。每个字段含 `field/value/page/quote`，仅允许合同领域定义的 14 种字段，重复字段、未知字段和额外属性一律失败。缺失或无法确定时省略字段。quote 保留逐字原文，value 允许有依据的语义等价转换、日期规范化和业务范围概括，不要求是 quote 的子串。不得猜测或补默认值；不得将发票后付款仅按相同天数转换成货到付款。
4. 通过 Rig `AgentHook` 检查响应完成状态：顶层 `status` 必须为 `completed`，且 `error` 与 `incomplete_details` 为空；`output` 必须包含唯一 `submit` 函数调用，调用状态为空或 `completed`。允许附带已完成或未声明状态的 reasoning 项，其内容不得进入证据。Rig 负责将提交参数反序列化为 DTO。输出截断、拒答、其他函数、未知输出项、未完成调用、多次提交、自由文本 JSON、Markdown、非法参数、越界页号或超限值须失败，不自动修复或选择第一份提交。识别完成后进入待确认，按[识别导入合同](contract-recognition-contract.md)返回部分预填结果；缺失、冲突、不可验证引文或无法映射的单字段返回 null，不阻断其他字段。必填、条款、日期和主数据校验在用户确认归档时执行。
5. 提取证据的 `provider` 必须取自任务配置快照中的 `provider_id`，随原有任务与合同修订保存，不发送给模型。`version` 由适配器生成，包含 `protocol=responses`、配置请求模型、响应报告模型及提示词版本 `contract-v4`；模型未报告版本时标为 `unreported`。供应商响应不得覆盖配置的供应商标识，生成内容不得自报供应商或版本。服务或提示词变更不得改写历史证据；历史 `openai-compatible` 供应商、`protocol=openai` 及 `prompt=contract-v1`、`prompt=contract-v2`、`prompt=contract-v3` 版本记录保留原值，不得推断或补填。
6. 失败保留原任务和已完成的有界阶段结果。没有自动重试；用户手动重试重新执行 OCR 与 AI，可能再次计费。超时取消本地等待，不保证供应商已取消已接收请求或免计费。
7. 日志内容不设限制，允许记录完整请求、响应、OCR、模型输出、原始错误、配置、凭据及调用上下文。本合同不要求日志脱敏、截断、字段白名单或内容过滤。适配器开启 SDK 内容追踪，SDK 日志须关联当前任务上下文。
8. 后台任务须建立 `contract_import` 日志上下文，包含 `task_id`、`account`、`request_id`。任务开始记录 `contract_import_started`，持久化后的业务失败须以 WARN 记录 `contract_import_finished`、`error_code`、`elapsed_ms`；持久化异常或提交结果未知须以 ERROR 记录，不得仅因 HTTP 返回 200 判定识别成功。
9. OCR 开始与完成须记录 `contract_ocr_started` / `contract_ocr_finished`、页数和耗时。识别阶段失败须记录 `contract_recognition_failed`、`stage`（`ocr` / `ai` / `validation`）、错误码、总耗时与当前阶段耗时；`RECOGNITION_TIMEOUT` 表示 180 秒总预算耗尽。
10. AI 调用须记录 `contract_ai_started` / `contract_ai_finished`，通过 `contract_ai` 上下文关联 `provider_id`、`model`、`base_url`、`protocol=responses`、`extraction_method=rig_extractor`、`output_tool=submit`、`prompt_version`、页数、超时秒数和输出 token 上限。结束日志须包含结果、耗时以及已取得的 HTTP 状态和供应商请求 ID；失败须以 WARN 记录错误码。`timeout_source` 区分 `application_deadline`（应用等待截止）、`connect`（连接超时）、`response_headers`（发送请求或等待响应头超时）、`response_body`（读取响应超时）、`upstream_http`（上游 408/504）。总预算取消 AI 时，以识别阶段的 `RECOGNITION_TIMEOUT` 为准，不得伪报供应商超时。
11. HTTP 失败须完整读取错误文本，在 `contract_ai_finished` 记录 `provider_error_body`、全部 `provider_response_headers` 及可用的 `provider_sdk_error`，不另设日志正文大小限制、不截断。JSON 错误须同时展开 `provider_error_type`、`provider_error_code`、`provider_error_param`、`provider_error_message`；保留供应商原值，不使用固定摘要替换。非 JSON 或 JSON 解析失败仍保留原始文本。供应商请求 ID 按 `x-request-id`、`x-dashscope-request-id`、`x-acs-request-id`、`x-ds-trace-id` 顺序提取，不限制值的长度或字符；其他头仍完整记录。
12. `provider_error_body_state` 标记正文已收到（`received`）、读取失败（`read_failed`）或调用期限到达前尚未读完（`pending`）；读取失败须记录原始 `provider_error_read_error`。诊断正文读取沿用既有请求超时与总预算，不额外发送请求或自动重试。已经取得非成功 HTTP 状态时，正文读取失败或 AI 调用期限到达均按该状态分类，不得把已知的 HTTP 400/401 等改报成等待响应超时。
13. HTTP 成功响应须在 SDK 解码前保留完整已接收正文，沿用 1 MiB 传输上限，不另设日志截断。SDK 解码或本地输出校验失败时，`contract_ai_finished` 须记录 `provider_response_body`。SDK 已解码时，同时记录 `provider_decoded_response`、`provider_finish_reason`、`provider_response_status`、`provider_incomplete_details`、`provider_usage`，区分供应商原始正文与 SDK 解码结果。
14. 本地输出拒绝须记录 `output_validation_error`，明确完成原因、响应状态、输出项类型或状态、Rig 参数反序列化错误、重复字段、非法字段或超限项。JSON 错误保留 SDK 解析器提供的信息，字段错误保留字段名，超限错误保留实际值与上限。对外仍返回 `AI_INVALID_OUTPUT`，协议格式与资源限制保持不变；业务缺失和冲突按待确认草稿规则处理。

| 错误码 | 行为 |
| --- | --- |
| `AI_NOT_CONFIGURED` | 补齐配置后手动重试 |
| `AI_UNAUTHORIZED` | 检查 API key、账户和模型授权 |
| `AI_THROTTLED` | 等待限流恢复后手动重试 |
| `AI_TIMEOUT` / `RECOGNITION_TIMEOUT` | 检查模型耗时、合同体积及总预算 |
| `AI_UNAVAILABLE` | 检查连接及供应商可用性 |
| `AI_REJECTED` | 检查 Responses API、模型、上下文容量及严格函数调用支持 |
| `AI_INVALID_OUTPUT` | 输出不完整或不满足协议，禁止归档 |

## 4. 验证与回退

1. 库单元测试须通过真实 Rig Extractor、OpenAI provider 与内存 HTTP 替身执行提取，覆盖 `/responses` 路由、`submit` 严格 Schema、必选且单次函数调用、模型 ID 原样传输、全部 OCR 页面、类型反序列化、本地范围限制、供应商证据、reasoning 隔离、推理耗尽输出额度、完成状态、拒答、多次提交、其他工具、自由文本、证据校验、冲突、缺失、重复、异常响应、无重试、超时取消及失败诊断。不得使用真实计费接口作为单元测试。
2. 运行 `cargo check --workspace --locked`、受影响 crate 的库测试与 Clippy，以及领域依赖边界检查。不得执行真实 MongoDB/S3 集成测试。
3. 实际供应商上线前须验证 Responses API 路由、严格函数调用兼容性、真实合同识别质量及跨页冲突检测。静态门禁和协议替身测试不得替代此项验收。
4. 删除 `[contract_ai]` 配置即可停用后续任务的 AI 提取。回退不得删除历史合同修订、原文件、任务或识别证据。

SDK 依据：[Rig Extractor](https://docs.rs/rig-agent/0.43.0/rig_agent/extractor/index.html)、[rig-core 0.43.0 OpenAI provider](https://docs.rs/rig-core/0.43.0/rig_core/providers/openai/index.html)。
