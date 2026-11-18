# erp-contract-recognition：合同识别供应商适配

## 职责

本 crate 实现 [合同领域定义的识别 Port](../../domains/erp-contract/src/ports/recognition.rs)：`ContractOcr::recognize(pdf, page_count)` 接收原始 PDF 和独立校验的页数，返回保留原页号的 `OcrDocument`；`ContractExtractor::extract(document)` 接收逐页 OCR 全文，返回带页码、原文引文和冲突说明的 `ContractExtraction`。Port 由消费方定义，适配器负责把合同输入转成供应商请求，再把结果或失败转回领域类型。

识别只为人工核对提供候选值，不决定客户身份或创建正式合同。任务状态、权限重验、主数据匹配、人工确认及归档事务由 [`erp-data-import::contract_import`](../../orchestration/erp-data-import/src/contract_import/mod.rs) 编排；本 crate 不读对象存储或数据库，不修改任务、业务单据、客户主数据、附件关系或命令回执。

## 目录结构

### 代码入口

| 入口 | 职责 |
| --- | --- |
| [Cargo.toml](Cargo.toml)、[src/lib.rs](src/lib.rs) | 声明合同领域和技术依赖；只公开 `AliyunContractOcr`、`OpenAiContractExtractor` 两个实现。 |
| [src/aliyun/mod.rs](src/aliyun/mod.rs) | 校验 PDF 页数，按原顺序收集逐页 OCR，映射阿里云客户端错误。 |
| [src/aliyun/render.rs](src/aliyun/render.rs)、[src/aliyun/image.rs](src/aliyun/image.rs) | 管理私有临时文件和有界的 Poppler 渲染，完整解码 PNG 并判定空白页。 |
| [src/openai/mod.rs](src/openai/mod.rs)、[src/openai/prompt.txt](src/openai/prompt.txt)、[src/openai/output.rs](src/openai/output.rs) | 装配 Rig Extractor，约束模型输入与 `submit` 输出，校验并转换领域结果。 |
| [src/openai/transport.rs](src/openai/transport.rs)、[src/openai/diagnostics.rs](src/openai/diagnostics.rs)、[src/openai/rejection.rs](src/openai/rejection.rs) | 限制 HTTP 请求时限和成功响应大小，保留传输错误分类与供应商诊断。 |

## 架构设计

### 装配合同

[服务端 `Application::contract_import_process`](../../../apps/erp-server/src/wiring/application/imports.rs) 在构造流程时读取一次 `SafeConfig` 快照。`[aliyun_ocr]` 提供签名凭据和 `pdftoppm` 路径，`[contract_ai]` 提供供应商标识、API 前缀、密钥、模型、超时和输出 token 上限；应用分别以 `Arc<dyn ContractOcr>`、`Arc<dyn ContractExtractor>` 注入 [`RecognitionProviders`](../../orchestration/erp-data-import/src/contract_import/pipeline.rs)。构造不发请求，已启动的识别沿用该快照。缺少哪一节，就保留对应的未配置实现；执行到该阶段时返回 `OCR_NOT_CONFIGURED` 或 `AI_NOT_CONFIGURED`，不返回模拟结果，也不回退到其他供应商。

配置来源见[配置入口](../../../config/README.md)，键名和默认值见[配置示例](../../../config.toml.example)。运行环境须提供 Poppler 的 `pdftoppm` 和中文字体；可使用默认的 `pdftoppm` 或配置可执行文件路径，不能由上传请求指定。AI 网关须兼容 OpenAI Responses 的严格工具调用，`base_url` 填 API 前缀，不含 `/responses` 或 `/chat/completions`。

业务依赖只指向 `erp-contract` 的识别类型和 Port，其余为技术依赖。本 crate 不得依赖 Process、ReadModel、其他业务领域、数据库、对象存储或应用配置；领域和 Process 不得反向依赖具体适配器，由应用负责接线。

### 执行合同

1. **从原 PDF 到逐页 OCR。** [HTTP `run`](../../../apps/erp-server/src/http/handler/contract/imports.rs) 只等待领取结果；[服务端后台任务](../../../apps/erp-server/src/http/handler/contract/import_execution.rs) 读取原文件，请求断开不会取消已启动的工作。Process 核对原文件摘要后，由 `RecognitionProviders::run` 调用 `ocr.recognize(pdf, page_count)`。适配器限制 PDF 为 20 MiB，独立解析并核对 1 至 200 页，将文件写入私有临时目录。Poppler 不经 shell，按相同起止页码逐页渲染 PNG，不裁切、不隐藏批注；长边限制为 3200 像素，单页限时 20 秒，输出最多 10 MiB。渲染超时或取消时终止子进程，临时目录随渲染上下文释放而清理。
2. **空白页保留原页号，其他页才调用阿里云。** 例如一份三页合同依次为正文、空白页和带签章的附表，只有第 2 页完整解码后所有像素均为不透明白色，才可保留为空白页而不发 OCR 请求。第 1、3 页的原始 PNG 分别交给 [`aliyun_ocr::Client::recognize_image`](../../platform/aliyun-ocr/src/client.rs)。客户端负责 `RecognizeDocumentStructure` 的 ACS3 签名、HTTPS 请求及响应解码，只接收图片、返回全文和供应商版本，不解析 PDF 或决定合同页号。适配器按 `1, 2, 3` 返回全页结果；非空白页未识别到文字、任一页失败、文本超限（单页 100,000 字节、全文 2,000,000 字节）或供应商版本不一致时，立即停止，不得跳页、重排或继续提取。
3. **从逐页全文到待核对字段。** OCR 通过覆盖校验后，管线调用 `extractor.extract(document)`。`OpenAiContractExtractor` 将含原页号的完整 OCR 页面序列化为 JSON，由 Rig Extractor 发送给 OpenAI Responses，不发送 PDF 或页图。合同文字只作为数据，不执行其中的指令。只接受完整响应中的一次严格 `submit`：例如第 3 页写「签订日期：2026 年 11 月 18 日」，可返回 `signed_at` 值 `2026-11-18`，并附第 3 页页码和这段连续原文。第 1 页正文和第 3 页附表的付款约定冲突时报告冲突，不替用户选择。缺失字段不猜测，模型没有业务工具，不能查主数据、返回 ERP 主键或执行归档。适配器校验响应及字段结构，领域在[生成核对草稿](../../domains/erp-contract/src/entity/recognition/draft.rs)时再核对引文是否出现在指定页；缺失、冲突或无可靠证据的字段留空。
4. **运行与落库分开。** 页面渲染、供应商 HTTP 和识别均在数据库事务外执行。OCR 与提取共用管线的 180 秒总预算，AI 另受配置的期限和输出上限约束；两端均不隐式重试。超时、授权失败、限流、供应商拒绝与无效输出保留原有 `ImportFailure` 分类，不把供应商原始响应放进任务失败结果。OCR 不记录原 PDF、供应商响应或凭据；AI 失败诊断可能含供应商响应正文，应按敏感日志处理。
5. **识别成功不等于归档。** Process 单独保存进度和结果：识别失败记为 `failed`，保留原任务和文件；失败或过期处理可从原任务重新发起，已完成识别不重复调用供应商。成功只进入 `review`，只有人工 `confirm` 才在事务内重验权限、匹配主数据并归档。完整状态及归档规则见[合同识别导入执行合同](../../domains/erp-contract/README.md#合同识别导入执行合同)。

## 修改与验证要求

保留成功、失败、取消及错误分类语义。仅改文档时，在 `backend/` 对该文件执行 `git diff --check -- crates/adapters/erp-contract-recognition/README.md`。业务代码改动在 `backend/` 执行 `env -u ERP_TEST_MONGO_URI cargo test -p erp-contract-recognition --lib --locked`；单元测试只使用纯数据、替身页面或内存 HTTP 客户端，不访问真实 OCR、AI、MongoDB 或 S3。跨 crate 变更按[统一质量门禁](../../README.md#质量门禁)执行相应检查。
