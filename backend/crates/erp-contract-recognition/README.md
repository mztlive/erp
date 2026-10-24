# erp-contract-recognition：合同识别供应商适配

实现合同领域声明的 `ContractOcr` 和 `ContractExtractor`，负责 PDF 转图、逐页 OCR 和全文字段提取。任务状态、权限重验、客户匹配、用户确认与归档事务由 `erp-processes::contract_import` 执行。

## 装配合同

1. `web-api` 从 `SafeConfig` 快照构造 `AliyunContractOcr` 与 `OpenAiContractExtractor`，以合同领域 trait 对象交给 `RecognitionProviders`。配置键与默认值以 [配置入口](../../config/README.md) 和 [配置示例](../../config.toml.example) 为准。
2. 本 crate 只依赖合同领域的识别输入、结果和 Port，以及供应商技术库；不得依赖 Process、ReadModel、其他业务领域、数据库、对象存储或应用配置。
3. 领域与 Process 只消费合同领域 Port；不得引用本 crate 的具体实现。
4. 本 crate 不修改任务、业务单据、客户主数据、附件关系或命令回执。

## 执行合同

- `AliyunContractOcr` 独立核对 PDF 页数，使用私有临时目录和有界 Poppler 输出逐页转图。取消时终止子进程并清理临时文件。
- 只有完整解码且所有像素为不透明白色的页面可认定为空白。OCR 页面不得跳过、重排或在失败后继续执行。
- `OpenAiContractExtractor` 使用 Rig Extractor 与 OpenAI Responses provider，接收逐页 OCR 全文，仅接受标准 `submit` 输出函数返回的字段及证据。
- 模型不得调用业务工具、查询主数据或执行归档；超时、请求限流、供应商拒绝与无效输出必须保持既有 `ImportFailure` 分类。
- 外部 HTTP、页面渲染和识别必须在数据库事务外执行；禁止隐式重试。

## 代码入口

| 入口 | 职责 |
| --- | --- |
| [src/lib.rs](src/lib.rs) | 对应用暴露两种供应商适配器 |
| [src/aliyun/mod.rs](src/aliyun/mod.rs) | PDF 覆盖校验、逐页 OCR 与供应商错误映射 |
| [src/aliyun/render.rs](src/aliyun/render.rs) | 私有临时文件、转图超时与输出限制 |
| [src/openai/mod.rs](src/openai/mod.rs) | 字段提取协议、期限及结果转换 |
| [src/openai/transport.rs](src/openai/transport.rs) | HTTP 传输限制与错误分类 |

## 修改与验证要求

保留现有识别成功、失败、取消及错误分类语义。库单元测试必须使用纯数据、替身页面或内存 HTTP 客户端，不得访问真实 OCR、AI、MongoDB 或 S3。

在 `backend/` 执行：

```bash
env -u ERP_TEST_MONGO_URI cargo test -p erp-contract-recognition --lib --locked
```

跨 crate 变更执行[统一质量门禁](../README.md#质量门禁)。编译收益必须按相同工具链、构建配置和缓存条件单独测量；本 crate 边界不构成性能承诺。
