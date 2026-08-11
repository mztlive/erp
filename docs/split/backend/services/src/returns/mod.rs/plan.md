# `backend/services/src/returns/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/returns/mod.rs` |
| 扫描行数 | 1625 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议保留 mod.rs 作为 returns 模块根，仅负责领域文档、子模块声明、DTO re-export 和 ReturnsService re-export；将服务主体、退货单据、客户/供应商退款、回款/付款冲正以及共享反向核销规划分别拆出。当前代码存在五个边界清晰的内聚簇，拆分后最大文件预计约 630–700 行，所有文件均可控制在约 800 行以内，同时保持 services::returns::ReturnsService 和现有 DTO 的外部访问路径不变。
- 拆分建议：
  - **backend/services/src/returns/service.rs**：放置公开服务类型 ReturnsService，以及仅包含构造函数 new 的基础 impl ReturnsService。ReturnsService 包含 db: Database 字段。
    - 依赖/注意：db 字段应声明为 pub(super) db: Database，使 return_documents、refunds、reversals 等兄弟模块中的 impl ReturnsService 能访问数据库；mod.rs 使用 pub use self::service::ReturnsService 保持 services::returns::ReturnsService 外部路径不变。该文件只依赖 mongodb::Database，不依赖其他业务子模块。
  - **backend/services/src/returns/return_documents.rs**：放置退货业务单据编排：类型别名 SalesReturnCaseFilter、PurchaseReturnOrderFilter；公开方法 sales_return_case_list、sales_return_case_detail、create_sales_return_case、purchase_return_order_list、purchase_return_order_detail、create_purchase_return_order；私有视图装配方法 sales_return_case_view、purchase_return_order_view。
    - 依赖/注意：通过 super::ReturnsService 扩展同一服务类型；从 super::dto 导入 SortDir、请求和视图类型。两个视图 helper 与调用方留在同一文件后可继续保持私有 async fn，无需扩大可见性。需要按本文件实际调用导入 ReturnsExt、AccessControlExt、NoTransaction、Transactional。视图行类型应改用 super::dto::SalesReturnLineView 和 super::dto::PurchaseReturnLineView，避免继续使用较长的 crate::returns::dto 路径。
  - **backend/services/src/returns/refunds.rs**：放置客户/供应商退款镜像流程：类型别名 CustomerRefundFilter；公开方法 customer_refund_list、customer_refund_detail、create_customer_refund、post_customer_refund、supplier_refund_detail、create_supplier_refund、post_supplier_refund；私有方法 customer_refund_view、supplier_refund_view。
    - 依赖/注意：通过 super::ReturnsService 扩展服务；依赖 super::reverse_plan::{plan_receipt_reverse, plan_payment_reverse, zero_amount}。需要导入应收与应付两侧实体、ID、Repository 扩展 trait、Transactional 和审计类型。两个视图 helper 与对应详情/创建方法同文件，可保持模块私有。该文件预计约 630–700 行，是拆分后最大文件但仍低于约 800 行。不要改变退款过账事务内的查询、冲减、分录、分配、状态迁移和审计顺序。
  - **backend/services/src/returns/reversals.rs**：放置回款/付款冲正镜像流程：公开方法 create_receipt_reversal、post_receipt_reversal、create_payment_reversal、post_payment_reversal；私有方法 receipt_reversal_view、payment_reversal_view。
    - 依赖/注意：通过 super::ReturnsService 扩展服务；依赖 super::reverse_plan::{plan_receipt_reverse, plan_payment_reverse, zero_amount}。需要同时导入 ReceivableExt、PayableExt、ReturnsExt、AccessControlExt、Transactional 及两侧分配实体。视图 helper 留在本文件并保持私有。与 refunds.rs 只共享 reverse_plan，不相互引用，避免形成循环依赖。
  - **backend/services/src/returns/reverse_plan.rs**：放置共享的纯计算规划类型与函数：ReceiptReversePlanRow、ReceiptReverseChunk、PaymentReversePlanRow、PaymentReverseChunk、plan_receipt_reverse、plan_payment_reverse、zero_amount。
    - 依赖/注意：该模块不应引用 ReturnsService 或数据库，只依赖 ReceiptAllocation、PaymentAllocation、相关 ID、Amount、FromStr 和 crate::errors::{Error, Result}，保持依赖单向。由于 refunds.rs 和 reversals.rs 需要调用规划函数并读取返回结构字段，函数、四个结构及其 original_id、amount、entry_id、increase_entry_id 字段均需使用 pub(super)；否则会出现兄弟模块不可访问或私有类型泄漏问题。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
