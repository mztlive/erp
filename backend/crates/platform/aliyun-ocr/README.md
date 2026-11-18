# aliyun-ocr 接入合同

## 职责

`aliyun-ocr` 是平台层的阿里云单图 OCR 客户端。它调用 `RecognizeDocumentStructure`，负责 ACS3 签名、HTTPS 传输、响应解码与错误脱敏，返回图片全文和版本信息。它不读取部署配置，不依赖 ERP 业务领域、数据库或配置中心，也不处理 PDF、合同页码、空白页判定或归档。

## 目录结构

| 文件 | 职责 |
| --- | --- |
| [`Cargo.toml`](Cargo.toml) | 声明 HTTP、签名、序列化等技术依赖，不依赖其他 ERP crate。 |
| [`src/lib.rs`](src/lib.rs) | 导出 `Client`、`Credentials`、`Page`、`Error`、`Result` 及端点、API 版本和大小上限。 |
| [`src/client.rs`](src/client.rs) | 构造并发送单图请求，设置传输策略，有界读取成功响应。 |
| [`src/credentials.rs`](src/credentials.rs) | 校验 AK/SK 和可选 STS 令牌的格式，脱敏调试输出。 |
| [`src/signing.rs`](src/signing.rs) | 生成 ACS3 授权头，标记授权头和 STS 头为敏感信息。 |
| [`src/response.rs`](src/response.rs) | 解码双层 JSON，组装全文和版本，将 HTTP 状态和供应商业务码映射为稳定的错误分类。 |

## 架构设计

### 接口与传输

1. **单图请求。** 用 `Client::new(Credentials)` 创建可复用客户端，再调用 `recognize_image(&[u8])`。输入是单张图片的原始字节，必须非空且不超过 10 MiB。客户端只检查长度，不解码图片；调用方不得传入 PDF、Base64 编码文本或文件 URL。
2. **固定请求协议。** 请求通过 HTTPS 发往杭州公网端点 `ocr-api.cn-hangzhou.aliyuncs.com`，方法和路径为 `POST /`，API 版本为 `2021-07-07`，正文类型为 `application/octet-stream`。端点和查询参数不能由上传请求控制。固定参数启用自动旋转、阅读顺序、段落、成行和表格输出，保留签章，使用旧版输出格式。
3. **签名和传输。** 每次请求读取当前 UTC 时间，并生成新的随机 nonce。ACS3-HMAC-SHA256 对规范请求签名，覆盖实际发送的查询串、`host`、`content-type`、全部 `x-acs-*` 头（含可选 STS 头）和原始图片的 SHA-256 摘要。连接限时 5 秒，单次 HTTP 调用限时 30 秒；不跟随重定向，不自动重试计费请求。

### 响应与错误

- HTTP 非成功状态直接按状态码分类，不读取错误正文：401/403 为授权失败，429 为限流，5xx 为服务不可用，其余为请求被拒绝。超时和其他传输失败分别返回 `Timeout` 和 `Transport`。
- HTTP 成功后，先检查声明长度和累计分块，正文最多 8 MiB。解析外层 JSON 后，若 `Code` 是 `"200"` 以外的字符串，则将已知业务码归为授权失败、限流或服务不可用，未知码归为请求被拒绝。若 `Code` 缺省、为 `null` 或为 `"200"`，则将 `Data` 字符串二次解析为 JSON。缺少必需字段或格式不符返回 `InvalidResponse`，长度超限返回 `ResponseSize`。
- `Page.text` 原样返回 `content`，最多 100,000 字节，不从表格或文字块拼接全文。`Page.version` 格式为 `2021-07-07;algo=…;prism=…`，算法和 Prism 版本各限 80 字节，供应商未提供的部分留空。空文本不代表页面空白。
- `Error` 不携带供应商原始错误码、错误消息、响应正文、底层 HTTP 错误或凭据。

### 合同适配边界

- [`erp-contract-recognition`](../../adapters/erp-contract-recognition/README.md) 的 `AliyunContractOcr` 实现合同领域的 [`ContractOcr`](../../domains/erp-contract/src/ports/recognition.rs)。适配器独立核验 PDF 页数，把不超过 20 MiB 的 PDF 放入私有临时目录，用 Poppler 按原页码逐页转为长边缩放至 3200 像素的 PNG，不裁切、不隐藏批注。单页渲染限时 20 秒、输出最多 10 MiB；取消时终止渲染子进程，识别结束或取消时清理临时目录。
- 只有完整解码后全部像素均为不透明白色的页面才在本地判为空白：保留原页码，跳过外部调用。其余页面即使 OCR 返回空文本或纯空白文本也判为不可读。任一页失败、不可读或非空白页版本不一致，均停止本次识别，不返回缺页结果，也不自动重试。
- 合同适配器限制 1～200 页、全文最多 2,000,000 字节。[合同导入流程](../../orchestration/erp-data-import/src/contract_import/pipeline.rs) 只依赖领域 Port，在数据库事务外执行 OCR 和 AI 提取，共用 180 秒预算。超时后不会继续提交后续页面；不缓存逐页计费结果，手动重试整个任务时会重新识别非空白页。字段提取、人工确认与归档不属于本 crate。

## ERP 部署与使用

1. 开通阿里云 `RecognizeDocumentStructure` 并授予 RAM 权限 `ocr:RecognizeDocumentStructure`。ERP PDF 识别环境另须提供 Poppler `pdftoppm` 和中文字体；[运行镜像](../../../Dockerfile)安装了 `poppler-utils` 和 `fonts-noto-cjk`。本地 macOS 安装 Poppler 后，可使用 PATH 中的 `pdftoppm`，也可配置可执行文件的绝对路径。
2. 服务端从 [`SafeConfig` 的本地 TOML 或 Nacos](../../../config/README.md) 取快照，按[配置示例](../../../config.toml.example)填写。`[aliyun_ocr]` 整节省略时不启用 OCR；配置该节时必须提供 `[aliyun_ocr.credentials]` 下的 `access_key_id`、`access_key_secret`，使用临时凭据时还需 `security_token`。`pdftoppm_path` 默认是 `pdftoppm`，显式配置时不能为空。缺少必填项或格式不合法时，配置校验失败；本地校验不会验证云端权限或令牌有效期。
3. 真实凭据只存放在受控配置中，不提交、不记录。新启动的识别任务使用当前有效快照，已启动的任务不切换快照。客户端不自动续期；STS 到期后应更新配置并在新快照生效后手动重试原任务。
4. AI 字段提取由 `[contract_ai]` 单独配置，使用 Rig 的 OpenAI Responses 适配器，见[适配器执行合同](../../adapters/erp-contract-recognition/README.md#执行合同)。仅配置 OCR 时，已完成的 OCR 结果可以保留，但缺少 AI 配置仍会使任务失败，不能归档。

## 验收

- 修改客户端实现时，用离线库单元测试验证阿里云公开签名固定向量、二进制请求与 STS 签名、响应二次解析、错误脱敏和大小限制。修改合同适配器时，另用本地 PNG 验证空白页判定，用替身页面验证原页码和任一页失败即停止。
- 单元测试不等同于云账号授权、真实图片识别、Poppler 实际渲染、容器镜像构建或数据库和 S3 验收；不在开发测试中调用真实计费接口。
- 仅修改本 README 时，按[后端开发规则](../../../AGENTS.md#验证)在 `backend/` 执行 `git diff --check -- crates/platform/aliyun-ocr/README.md`，不编译、不运行测试。
