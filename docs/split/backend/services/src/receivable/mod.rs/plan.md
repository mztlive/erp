# `backend/services/src/receivable/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/receivable/mod.rs` |
| 扫描行数 | 1353 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议把当前 1353 行的单体服务按应收子账、客户回款、客户回款过账、发票基础操作、发票过账与红冲六个职责拆分。mod.rs 仅保留模块根、DTO 与服务类型 re-export，以及跨子域共享的私有 zero_amount。拆分后最大文件预计约 450–500 行，所有文件均能控制在约 800 行以内，同时保持 services 层负责事务与 Repository 编排的边界。
- 拆分建议：
  - **backend/services/src/receivable/service.rs**：放置公开服务类型 ReceivableService 及构造方法 ReceivableService::new；ReceivableService 仍只持有 mongodb::Database。
    - 依赖/注意：mod.rs 使用 pub use self::service::ReceivableService 保持外部 API 不变。db 字段需使用 pub(super) 可见性，供 receivable_account、customer_receipt、customer_receipt_posting、invoice、invoice_posting 等兄弟模块中的 impl 块访问；不要扩大为 pub 或 pub(crate)。
  - **backend/services/src/receivable/receivable_account.rs**：放置 ReceivableAccountFilter 类型别名，以及 ReceivableService 的 receivable_account_list、receivable_account_detail、create_receivable_account、update_receivable_account_review、append_funds_review 方法和私有 receivable_account_view。append_funds_review 与该文件内聚，因为它在同一事务中追加复核链并更新 ReceivableAccount 复核缓存。
    - 依赖/注意：仅此文件需要 SalesOrderExt；同时需要 AccessControlExt、ReceivableExt、Transactional、NoTransaction。通过 super::dto 导入请求和视图类型，通过 super::zero_amount 使用共享零金额。FundsReviewView 和 ReceivableEntryView 应直接导入，替换原有 crate::receivable::dto 完整路径。不得把来源销售单校验或事务内账户、分录、审计写入下沉到 Repository。
  - **backend/services/src/receivable/customer_receipt.rs**：放置 CustomerReceiptFilter 类型别名，以及 ReceivableService 的 customer_receipt_list、customer_receipt_detail、create_customer_receipt 方法；同时放置私有 customer_receipt_view 和 allocation_view，集中处理回款单查询、草稿登记及回款分配视图装配。
    - 依赖/注意：使用 ReceivableExt、AccessControlExt 和 NoTransaction；通过 super::zero_amount 计算分配净额。allocation_view 是 DTO 装配 helper，应保持私有，不应从 receivable 模块导出。customer_receipt_detail 会被 posting 模块中的同类型 impl 调用，这不会形成 Rust 模块循环依赖。
  - **backend/services/src/receivable/customer_receipt_posting.rs**：放置客户回款事务过账流程 post_customer_receipt，以及只服务该流程的私有 req_allocated_total、net_receipt_allocated。该文件集中维护回款状态校验、既有净核销汇总、跨主体校验、开放余额校验、分配写入、子账结算进度更新和审计写入。
    - 依赖/注意：需要 ReceivableExt、AccessControlExt、Transactional，以及 ReceiptAllocation、AllocationAction、Amount 等类型。保持现有事务闭包中的查询和写入顺序，不要将余额、主体或状态校验移到事务外。完成事务后可继续调用 self.customer_receipt_detail；如后续按 30 行方法约定继续重构，应只在本文件内提取私有 posting helper，并谨慎处理 ClientSession 生命周期。
  - **backend/services/src/receivable/invoice.rs**：放置 InvoiceFilter 类型别名，以及 ReceivableService 的 invoice_list、invoice_detail、create_invoice 方法；同时放置私有 invoice_view 和 sales_allocation_view，集中处理发票查询、草稿创建及发票分配视图装配。
    - 依赖/注意：使用 ReceivableExt、AccessControlExt 和 NoTransaction；通过 super::zero_amount 设置舍入调整缺省值并计算净分配额。sales_allocation_view 保持私有。invoice_detail 会被 invoice_posting 模块中的同类型 impl 调用，不需要相互声明子模块，也不会形成循环依赖。
  - **backend/services/src/receivable/invoice_posting.rs**：放置发票资金事实事务流程 post_invoice 和 issue_red_invoice，集中维护蓝票登记、号码幂等校验、子账开票进度更新、红票创建、原分配反向引用、累计红冲限制和审计写入。
    - 依赖/注意：需要 ReceivableExt、AccessControlExt、Transactional、Invoice、InvoiceData、InvoiceKind、SalesInvoiceAllocation 及其 ID/Data 类型，并通过 super::zero_amount 构造金额初值。必须保持发票号码检查、分配校验、apply_invoicing 或 revert_invoicing、发票状态迁移和审计写入位于同一事务。两个方法仍较长，后续如提取私有 helper，应留在本文件并避免引入 Service 之间的依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
