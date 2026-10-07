# aliyun-ocr 接入合同

本 crate 只负责阿里云 `RecognizeDocumentStructure` 图片识别、ACS3 请求签名、HTTPS 传输和结果解码。不得依赖 ERP 业务领域、数据库或配置中心。PDF 转图、原页码和合同准入由 `erp-processes::contract_import::aliyun::AliyunContractOcr` 实现，消费方仍依赖 `erp-contract::ports::recognition::ContractOcr`。

## 接口与传输

1. 使用 `ocr-api.cn-hangzhou.aliyuncs.com`、`POST /`、版本 `2021-07-07`。调用前必须开通该服务，授予 `ocr:RecognizeDocumentStructure` 权限。
2. `Client::new(Credentials)` 构造客户端，`recognize_image(&[u8])` 接收不超过 10 MB 的图片二进制。不得传 PDF、base64 或公开文件 URL。`Credentials` 支持 AK/SK 以及可选 STS token，不执行自动续期。
3. 所有 `x-acs-*`、`host`、`content-type` 和二进制正文摘要参与 ACS3-HMAC-SHA256 签名。每次请求生成新的 UTC 时间和 nonce。只允许 HTTPS，拒绝重定向，禁止自动重试计费请求。
4. 固定启用自动旋转、阅读顺序、段落、成行和表格输出，保留签章，使用旧版输出格式。`Data` 是 JSON 字符串，必须二次解析；`content` 全文原样交给消费方，不从部分表格或部分文字块拼出不完整页面。
5. 连接限时 5 秒，单次调用限时 30 秒；响应最多 8 MB，单页文本最多 100,000 字节。错误只返回稳定分类，不返回原始供应商 Message、HTTP 错误、响应或凭据。`Page.version` 同时保存 API、算法和 prism 版本；供应商未提供算法版本时对应段留空，不伪造版本。

## ERP 部署与使用

1. 运行环境必须提供 Poppler `pdftoppm` 和中文字体。仓库运行镜像安装 `poppler-utils`、`fonts-noto-cjk`；本地 macOS 可安装 Poppler 后使用 PATH 中的 `pdftoppm`，或配置绝对路径。
2. 配置只从现有 `SafeConfig` 文件或 Nacos 提供。整个节缺省时 OCR 保持未配置；出现不完整配置时配置加载失败。示例见 `backend/config.toml.example`：

```toml
[aliyun_ocr]
pdftoppm_path = "pdftoppm"

[aliyun_ocr.credentials]
access_key_id = "<RAM AccessKey ID>"
access_key_secret = "<RAM AccessKey Secret>"
# security_token = "<STS SecurityToken>"
```

3. 不得提交真实凭据。调试配置时 AK、SK、STS token 均须脱敏。新任务读取当前配置快照；已开始任务使用原快照，STS 到期后须更新配置，再手动重试原任务。
4. 合同适配器独立核验原 PDF 页数，逐页转成 PNG，长边 3200 像素，不裁切、不隐藏批注。每页渲染限时 20 秒，图片最多 10 MB。临时 PDF 位于独占临时目录，任务结束或取消后清理；渲染子进程取消时终止。
5. 完全不透明纯白像素页由本地确认空白，保留原页号并跳过外部调用。其他页面即使 OCR 返回零文字也必须标为不可读，禁止当作空白跳过。任一页面失败须停止整个 OCR，禁止缺页成功或自动重试。不同页面的供应商版本不一致时须重试整个任务。
6. 沿用合同识别流程的 200 页、总文本 2 MB、OCR 加 AI 总计 180 秒上限。较长或耗时文档可能超时；当前不缓存逐页计费结果，手动重试会重新识别非空白页。不得在超时后偷偷后台继续提交 OCR。
7. AI 提取端通过 `[contract_ai]` 配置 Rig OpenAI 兼容服务，执行 [AI 提取接入合同](../../../docs/contract-ai-contract.md)。仅配置 OCR 时可保存 OCR 阶段结果；缺少 AI 配置时任务仍须失败，不得归档。

## 验收

1. 必须通过签名官方固定向量、二进制请求构造、STS 签名、响应二次解析、错误脱敏、大小限制与逐页失败停止的库单元测试。
2. 不得将单元测试等同于阿里云账号授权、真实图片识别、Poppler 实际渲染、容器镜像构建或数据库/S3 验收。本次不在开发测试中调用真实计费接口。

依据：[接口文档](https://help.aliyun.com/zh/ocr/developer-reference/api-ocr-api-2021-07-07-recognizedocumentstructure)、[ACS3 签名规范](https://help.aliyun.com/zh/sdk/product-overview/v3-request-structure-and-signature)、[Poppler 参数](https://manpages.debian.org/bookworm/poppler-utils/pdftoppm.1.en.html)。
