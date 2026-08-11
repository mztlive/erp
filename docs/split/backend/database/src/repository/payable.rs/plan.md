# `backend/database/src/repository/payable.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/payable.rs` |
| 扫描行数 | 1046 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议参照 database/src/repository/purchase_order/ 的域目录模式，将 payable.rs 改为 payable/mod.rs 模块根，并按应付子账、分录与抵销、供应商付款、分配查询、共享工具拆分。PayableRepository 继续留在 mod.rs 承载跨集合事务写入，两个关联类型 Filter 从模块根 re-export，以保持 PayableExt 和 services 调用方式不变。拆分后最大文件预计约 500～580 行，均低于约 800 行目标。
- 拆分建议：
  - **backend/database/src/repository/payable/mod.rs**：放置 payable 域模块文档、account/allocation/common/entry/payment 子模块声明，执行 `pub use account::PayableAccountFilter` 与 `pub use payment::SupplierPaymentFilter`；保留常量 PAYABLE_ENTRIES、公开类型 PayableRepository<'a>、impl PayableRepository<'a> 及方法 new、create_payable_with_entry。
    - 依赖/注意：必须从模块根 re-export PayableAccountFilter 和 SupplierPaymentFilter，以保持 repository/extensions/payable.rs 中 `super::super::payable::{...}` 导入不变。PayableRepository 保持在模块根，无需 re-export。这里继续使用 `super::extensions::PayableExt` 获取集合关联常量；子模块不要再依赖 PayableExt，以限制现有模块交叉引用。迁移时必须删除原 payable.rs，避免同时存在 payable.rs 与 payable/mod.rs 导致模块文件冲突。
  - **backend/database/src/repository/payable/account.rs**：放置 PayableAccountRow、PayableAccountFilter、其 QueryFilter/Pagination 实现；放置 `impl Repository<PayableAccount>` 的 search_payable_accounts、apply_settlement、revert_settlement、apply_invoicing、revert_invoicing、私有 conditional_update；放置私有 helper amount_bson、progress_pipeline、payable_account_projection，以及对应过滤、Decimal128 和进度管道测试。
    - 依赖/注意：account.rs 通过 `super::common::sort_doc` 使用域内排序 helper；sort_doc 只需 pub(super)。amount_bson、progress_pipeline 和 conditional_update 仅服务应付子账进度更新，应继续保持文件私有并与调用方同文件，避免扩大可见性。原 `super::{PageResult, Pagination, QueryFilter, Repository}` 在新层级下需改成 `crate::repository::{PageResult, Pagination, QueryFilter}` 与 `crate::Repository` 等明确路径。
  - **backend/database/src/repository/payable/entry.rs**：放置 `impl Repository<PayableEntry>` 与 `impl Repository<PayableEntryOffset>`，包含 find_entries_by_accounts、find_entries_by_account、find_offsets_by_decrease、find_offsets_by_increase。
    - 依赖/注意：两个 impl 都围绕应付账本分录及分录间抵销关系，合并在一个文件内比按实体再拆两个小文件更内聚。该文件不依赖 common；应直接从 crate::executor、crate::{Repository, Result} 和 mongodb::bson 导入依赖。空 ID 集合提前返回逻辑保持不变。
  - **backend/database/src/repository/payable/payment.rs**：放置 SupplierPaymentRow、SupplierPaymentFilter、其 QueryFilter/Pagination 实现；放置 `impl Repository<SupplierPayment>` 的 search_supplier_payments、find_by_payment_no；放置私有 supplier_payment_projection，以及付款筛选测试 payment_filter_escapes_regex_literals。
    - 依赖/注意：通过 `super::common::sort_doc` 复用排序白名单处理；字面量正则 helper 的路径需改为 `crate::repository::regex_filter::insert_literal_regex_filter`，不能保留原来的 `super::regex_filter`。SupplierPaymentFilter 必须由 mod.rs re-export，以维持 PayableExt 关联类型实现；SupplierPaymentRow 可按 purchase_order 子模块模式留在本文件，不必额外暴露到 repository 根。
  - **backend/database/src/repository/payable/allocation.rs**：放置 `impl Repository<PaymentAllocation>` 和 `impl Repository<PurchaseInvoiceAllocation>`；包含 find_allocations_by_payments、find_allocations_by_entries、find_allocations_by_invoices、find_allocations_by_accounts。
    - 依赖/注意：两类实体都是应付域的不可变分配事实，并共享 `$in` 批量查询和空集合守卫模式，放在同一 allocation.rs 内可避免产生过小文件。需要显式导入 SupplierPaymentId、InvoiceId、PayableEntryId、PayableAccountId；也可保留部分全限定 ID 路径，但建议统一局部 use。该文件不应引用 account.rs 或 payment.rs，以免形成不必要的子模块耦合。
  - **backend/database/src/repository/payable/common.rs**：放置 payable 域内共享排序 helper `pub(super) fn sort_doc`，以及测试 sort_doc_maps_whitelisted_fields_and_falls_back。
    - 依赖/注意：sort_doc 同时被 account.rs 和 payment.rs 使用，因此从文件私有调整为 pub(super)，仅在 payable 域目录内可见。不要提升为 pub(crate) 或放入 repository 通用 base，因为允许字段白名单由各调用方传入且目前只有 payable 子模块共享；这样也与 purchase_order/common.rs 的组织方式一致。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
