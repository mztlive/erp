# erp-file-assets

## 职责

本库把已上传、已核验的文件整理为 `PendingFileAssets`，供业务命令把临时引用换成正式资产 ID，并将文件资产元数据和登记审计写入原业务事务。本库不接收文件字节，不操作对象存储，不创建单据附件关系，也不执行商品、付款或履约命令，不为调用方另开事务。

## 目录结构

| 文件 | 职责 |
| --- | --- |
| [Cargo.toml](Cargo.toml) | 声明对 `erp-support`、`erp-catalog`、`erp-audit` 和持久化合同等的依赖；不依赖对象存储客户端。 |
| [src/lib.rs](src/lib.rs) | 公开 `PendingFileAssets`、`Error` 和 `Result`。 |
| [src/pending.rs](src/pending.rs) | 构造批次、准备登记审计和内容清单，实现 `erp-support` 的附件端口及有序持久化。 |
| [src/catalog.rs](src/catalog.rs) | 为同一批次实现 `erp-catalog` 的附件端口，委托原批次读写并转换错误。 |
| [src/errors.rs](src/errors.rs) | 定义编排错误，保留字段校验细节、瞬态事务冲突和未知提交等分类。 |

## 架构设计

### 事务前：核验字节，准备批次

调用方先核验实际内容并写入对象存储，再将 `PendingFileAssetRequest` 交给批次。例如，[HTTP 附件入口](../../../apps/erp-server/src/http/handler/file_asset/mod.rs) 校验 multipart 文件的大小、扩展名、MIME 和真实文件头，计算内容 HMAC，通过 [storage](../../platform/storage/README.md) 保存字节。`prepare` 不读取对象，只校验登记元数据、解析 `pending-file:` 临时引用、分配正式 `FileAssetId`，并用操作人 `AuditActor` 为每个资产准备一条 `file_asset.register` 普通审计。批次拒绝重复的临时引用或正式资产 ID。

对已在服务端完成解码或格式核验的文件，[供应目录来源图片](../erp-supplier-catalog/src/source_media/mod.rs) 使用 `prepare_content_checked` 标记 `ContentChecked`；这只表示基础内容已检查，不表示恶意内容扫描通过。

### 命令内：消费引用，复用原事务

`shared()` 将批次交给 [`erp-support` 附件端口](../../domains/erp-support/src/ports/pending.rs)，`into_catalog_batch()` 将同一批次交给 [`erp-catalog` 附件端口](../../domains/erp-catalog/src/ports/pending.rs)；转换均不写数据库。业务命令解析本次上传的临时引用时，必须拒绝未知引用和重复消费；既有正式 ID 保持原样，仍由业务命令按各自规则核验。解析完所有相关字段后，调用方用 `ensure_all_used` 拒绝漏用上传；`persist` 不替调用方执行此检查。

`persist` 在调用方传入的 `Executor` 上先用 [`erp-support` 仓储](../../domains/erp-support/src/repository/file_asset.rs) 有序写入 `file_assets`，再由 [`erp-audit::execution::persist_logs`](../../domains/erp-audit/src/execution/command.rs) 校验整批普通审计并按原顺序写入。首错即停止：资产写入失败不再写审计；审计失败也必须返回错误，由原命令事务整体回滚。有序批量写入本身不保证原子性。文件登记审计不替代业务命令的成功审计，详见[审计执行合同](../../../docs/audit-logging.md#统一执行入口)。

- **创建商品**：[主数据编排](../erp-master-data/src/attachments/catalog.rs) 准备商品附件批次；[商品服务](../../domains/erp-catalog/src/service/catalog/product_create.rs) 在事务前解析轮播图、详情图和 SKU 主图引用，在原事务中核验业务事实，写入资产及登记审计，再保存商品、修订、媒体、SKU 与商品审计。商品媒体关系由商品域维护，不由批次创建。
- **登记进项发票**：[HTTP 入口](../../../apps/erp-server/src/http/handler/payable/invoice_files.rs) 上传文件后把批次交给[发票登记 Process](../erp-finance-posting/src/finance_posting/payable/invoice.rs)。首次登记时，发票及分配、文件资产及登记审计、附件关系、财务回执和发票业务事件复用同一个 `Executor`，随原事务提交。[发票附件步骤](../erp-finance-posting/src/finance_posting/receivable/invoice_attachments.rs) 在资产登记后核验文件归属和发票证据规则，再创建 `erp-support` 的 [`DocumentAttachment`](../../domains/erp-support/src/entity/file_asset/document_attachment.rs)，以正式资产 ID 关联发票。资产记录描述文件本身，附件关系描述哪个单据以何种用途引用它。

### 事务后：按命令结果处理对象

文件字节不参与 MongoDB 事务，回滚不会删除对象。以进项发票为例，业务请求与按临时引用排序的内容清单共同构成幂等指纹；清单含文件名、MIME、字节数和内容 HMAC，不含每次上传新分配的资产 ID 或对象键。命中原财务回执时不重写资产和审计。由 [HTTP 补偿入口](../../../apps/erp-server/src/http/handler/file_asset/compensation.rs) 清理本次确定未提交或幂等回放未消费的对象；提交结果未知时保留对象，已确认提交但详情读取失败也不删除。批次本身不负责补偿或命令重试。
