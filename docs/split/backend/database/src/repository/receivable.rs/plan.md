# `backend/database/src/repository/receivable.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/receivable.rs` |
| 扫描行数 | 1354 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议参考 database/src/repository/purchase_order/ 的目录模块模式，把 receivable.rs 转换为 receivable/ 目录。以应收子账、应收事实链、客户回款、发票四个持久化内聚簇拆分，mod.rs 仅保留模块根、必要 re-export 和跨集合事务仓储，common.rs 放跨列表查询共享的排序 helper。这样既保持 ReceivableExt 的现有公开关联类型和访问路径不变，也能让所有结果文件稳定控制在约 800 行以内；最大预计为 account.rs 的约 500–600 行。这里的逻辑均依赖 MongoDB 查询或原子更新，不适合下沉到 entities，也不需要改动 services 模块。
- 拆分建议：
  - **backend/database/src/repository/receivable/mod.rs**：作为 D18 仓储模块根：声明 mod account、mod common、mod entry、mod invoice、mod receipt；通过 pub use account::ReceivableAccountFilter、pub use receipt::CustomerReceiptFilter、pub use invoice::InvoiceFilter 保持 ReceivableExt 关联类型引用不变；保留 RECEIVABLE_ENTRIES、RECEIVABLE_FUNDS_REVIEWS 常量；定义 ReceivableRepository<'a> 及其 impl，包括 new、create_receivable_with_entry、append_funds_review。
    - 依赖/注意：必须显式 re-export 三个 Filter，否则 repository/extensions/receivable.rs 中 super::super::receivable::{CustomerReceiptFilter, InvoiceFilter, ReceivableAccountFilter, ReceivableRepository} 会失效。ReceivableRepository 继续留在模块根，因此扩展访问器路径无需修改。仅模块根引用 super::extensions::ReceivableExt 获取集合名，子模块不要反向依赖 extensions，以免扩大现有模块解析环。删除原 receivable.rs 后，repository/mod.rs 中现有的 mod receivable; 无需修改。
  - **backend/database/src/repository/receivable/common.rs**：放置多个列表查询共同使用的 pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document；保留测试 sort_doc_maps_whitelisted_fields_and_falls_back。
    - 依赖/注意：sort_doc 需要从原来的私有 fn 提升为 pub(super)，供 account.rs、receipt.rs、invoice.rs 使用，但仍不应暴露到 receivable 模块外。调用方使用 super::common::sort_doc。该文件只依赖 mongodb::bson，不应依赖任何领域子模块，从而避免循环依赖。
  - **backend/database/src/repository/receivable/account.rs**：放置 ReceivableAccountRow、ReceivableAccountFilter、impl QueryFilter for ReceivableAccountFilter、impl Pagination for ReceivableAccountFilter；放置 impl Repository<'a, ReceivableAccount>，具体方法为 search_receivable_accounts、apply_settlement、revert_settlement、apply_invoicing、revert_invoicing、私有 conditional_update；放置私有 helper amount_bson、progress_pipeline、receivable_account_projection；迁入测试 account_filter_applies_optional_fields_and_deleted_filter、apply_pipeline_guards_status_and_keeps_decimal_fidelity、revert_pipeline_reduces_progress_without_status_cond_misuse、revert_pipeline_derives_open_when_progress_reaches_zero。
    - 依赖/注意：amount_bson 与 progress_pipeline 只服务核销和开票原子更新，应继续保持文件私有；conditional_update 也必须与四个进度方法处于同一个 Repository<ReceivableAccount> impl 中或同文件的另一 impl 中。排序通过 super::common::sort_doc 引用。需要保留 SerializerOptions、to_bson_with_options、Bson、Document、FindOptions、NOT_DELETED_TIMESTAMP_BSON、mongo_ops 和 Executor 等导入。测试改为从当前模块访问私有 helper，不再通过原顶层 super::amount_bson。
  - **backend/database/src/repository/receivable/entry.rs**：集中应收子账事实链查询：impl Repository<'a, ReceivableEntry>，包含 find_entries_by_accounts、find_entries_by_account；impl Repository<'a, ReceivableEntryOffset>，包含 find_offsets_by_decrease、find_offsets_by_increase；impl Repository<'a, ReceivableFundsReview>，包含 find_reviews_by_account。
    - 依赖/注意：这些方法共享 ReceivableAccountId、ReceivableEntryId、doc!、Executor、Repository 和 Result，放在同一文件可保持“子账事实链”内聚。空 ID 集合的提前返回必须原样保留，避免生成无意义的 $in 查询。append_funds_review 不应放入此文件，因为它属于 ReceivableRepository 的跨集合事务写入口；这样也避免 entry.rs 依赖模块根的数据库句柄和集合常量。
  - **backend/database/src/repository/receivable/receipt.rs**：放置 CustomerReceiptRow、CustomerReceiptFilter、impl QueryFilter for CustomerReceiptFilter、impl Pagination for CustomerReceiptFilter；放置 impl Repository<'a, CustomerReceipt>，包含 search_customer_receipts、find_by_receipt_no；放置 impl Repository<'a, ReceiptAllocation>，包含 find_allocations_by_receipts、find_allocations_by_entries；放置 customer_receipt_projection；迁入测试 receipt_filter_escapes_regex_literals。
    - 依赖/注意：字面量模糊查询继续复用 crate::repository::regex_filter::insert_literal_regex_filter，不要在新文件内复制正则转义逻辑。排序通过 super::common::sort_doc 引用。CustomerReceiptFilter 必须由 mod.rs re-export 以维持 ReceivableExt 关联类型。ReceiptAllocation 的两个批量查询与回款过账场景强相关，留在 receipt.rs 可避免与 invoice.rs 形成交叉依赖。
  - **backend/database/src/repository/receivable/invoice.rs**：放置 InvoiceRow、InvoiceFilter、impl QueryFilter for InvoiceFilter、impl Pagination for InvoiceFilter；放置 impl Repository<'a, Invoice>，包含 search_invoices、find_by_direction_and_normalized_no、find_red_invoices_by_original；放置 impl Repository<'a, SalesInvoiceAllocation>，包含 find_allocations_by_invoices、find_allocations_by_accounts；放置 invoice_projection。
    - 依赖/注意：InvoiceFilter 必须由 mod.rs re-export，因为 D18 服务和 D19 复用路径通过 ReceivableExt::InvoiceFilter 到达该类型。发票号码模糊匹配继续引用 crate::repository::regex_filter::insert_literal_regex_filter；排序引用 super::common::sort_doc。Invoice 与 SalesInvoiceAllocation 放在同一文件可保持开票登记、红冲和分配查询内聚，且无需依赖 receipt.rs 或 entry.rs，不会引入新的循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
