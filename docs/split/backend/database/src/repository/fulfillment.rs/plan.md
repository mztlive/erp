# `backend/database/src/repository/fulfillment.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/fulfillment.rs` |
| 扫描行数 | 1145 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | low |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 low）
- 摘要：建议参照 database/src/repository/purchase_order/ 的按集合拆分模式，把 fulfillment.rs 替换为 fulfillment/ 目录模块。五类列表查询分别形成采购入库、发货、电子交付、服务履约和客户验收五个集合级文件；mod.rs 保留 FulfillmentRepository、集合常量、跨集合批量查询与事务写入以及共享 helper。拆分后预计 mod.rs 约 400～480 行，其余文件约 120～180 行，所有文件均低于约 800 行，同时保持 FulfillmentExt 的关联类型和访问器接口不变。
- 拆分建议：
  - **backend/database/src/repository/fulfillment/mod.rs**：作为 D16 fulfillment 仓储模块根：声明 purchase_receipt、delivery、electronic_delivery、service_fulfillment、customer_acceptance 五个私有子模块；re-export PurchaseReceiptFilter、DeliveryFilter、ElectronicDeliveryFilter、ServiceFulfillmentFilter、CustomerAcceptanceFilter；保留集合常量 PURCHASE_RECEIPT_LINES、DELIVERY_LINES、CUSTOMER_ACCEPTANCE_LINES、ACCEPTANCE_FULFILLMENT_ALLOCATIONS；保留 FulfillmentRepository<'a> 及其完整 impl；保留共享 helper ids_to_strings、find_lines_in、sort_doc；保留 sort_doc 和 ids_to_strings 的单元测试。
    - 依赖/注意：原 backend/database/src/repository/fulfillment.rs 必须删除并由该目录模块替代，否则 Rust 会同时发现 fulfillment.rs 与 fulfillment/mod.rs。repository/mod.rs 中的 mod fulfillment; 无需修改。必须 pub use 五个 Filter，确保 extensions/fulfillment.rs 中 super::super::fulfillment::{...} 的现有导入继续编译。sort_doc 可保持模块根私有，子模块通过 super::sort_doc 使用；ids_to_strings 与 find_lines_in 仅供 FulfillmentRepository 使用，应继续保持私有。FulfillmentRepository 仍通过 super::extensions::FulfillmentExt 取得集合常量，避免复制集合名。模块根不要反向依赖子模块中的投影 helper，以免形成不必要的双向耦合。
  - **backend/database/src/repository/fulfillment/purchase_receipt.rs**：放置采购入库集合相关项目：PurchaseReceiptRow、PurchaseReceiptFilter、impl QueryFilter for PurchaseReceiptFilter、impl Pagination for PurchaseReceiptFilter、impl Repository<'a, PurchaseReceipt>，其中包含 search_purchase_receipts 和 find_by_receipt_no；放置私有 purchase_receipt_projection；放置 receipt_filter_applies_optional_fields_and_deleted_filter 测试。
    - 依赖/注意：通过 super::sort_doc 复用排序白名单 helper；Repository、PageResult、Pagination、QueryFilter 建议从 crate::repository 或 crate 根按 purchase_order 子文件的现有风格导入。PurchaseReceiptRow 保持 pub，因为 public 搜索方法返回 PageResult<PurchaseReceiptRow>；模块根只需 re-export PurchaseReceiptFilter，不必扩大 Row 的既有外部可见范围。采购入库表头和行的跨集合写入继续留在 mod.rs，符合 purchase_order/mod.rs 将事务聚合写入放在模块根的模式。
  - **backend/database/src/repository/fulfillment/delivery.rs**：放置发货集合相关项目：DeliveryRow、DeliveryFilter、impl QueryFilter for DeliveryFilter、impl Pagination for DeliveryFilter、impl Repository<'a, Delivery>，其中包含 search_deliveries 和 find_by_tracking_no；放置私有 delivery_projection。
    - 依赖/注意：通过 super::sort_doc 构建 created_at/shipped_at 白名单排序。delivery_projection 必须继续排除敏感履约地址字段。DeliveryRow 保持 pub，DeliveryFilter 由 mod.rs re-export。create_delivery_with_lines 和 delivery_lines_by_delivery_ids 继续留在模块根的 FulfillmentRepository impl 中，避免集合查询文件承担事务聚合职责。
  - **backend/database/src/repository/fulfillment/electronic_delivery.rs**：放置电子交付记录相关项目：ElectronicDeliveryRow、ElectronicDeliveryFilter、impl QueryFilter for ElectronicDeliveryFilter、impl Pagination for ElectronicDeliveryFilter、impl Repository<'a, ElectronicDelivery>，其中包含 search_electronic_deliveries；放置私有 electronic_delivery_projection。
    - 依赖/注意：通过 super::sort_doc 复用 occurred_at/recorded_at/created_at 排序逻辑。投影必须继续排除交付对象快照及其指纹。ElectronicDeliveryFilter 由 mod.rs re-export，ElectronicDeliveryRow 保持 pub 以满足 public 方法返回类型。该文件不应依赖 FulfillmentRepository，验收分配查询继续集中在模块根，避免电子交付与客户验收子模块互相引用。
  - **backend/database/src/repository/fulfillment/service_fulfillment.rs**：放置线下服务履约记录相关项目：ServiceFulfillmentRow、ServiceFulfillmentFilter、impl QueryFilter for ServiceFulfillmentFilter、impl Pagination for ServiceFulfillmentFilter、impl Repository<'a, ServiceFulfillment>，其中包含 search_service_fulfillments；放置私有 service_fulfillment_projection。
    - 依赖/注意：通过 super::sort_doc 复用 occurred_at/recorded_at/created_at 排序逻辑。投影必须继续排除交付对象快照、服务地点及其指纹。ServiceFulfillmentFilter 由 mod.rs re-export；不要直接引用 customer_acceptance 子模块，跨履约事实的分配读取仍由模块根 FulfillmentRepository 统一处理，以避免循环依赖。
  - **backend/database/src/repository/fulfillment/customer_acceptance.rs**：放置客户验收集合相关项目：CustomerAcceptanceRow、CustomerAcceptanceFilter、impl QueryFilter for CustomerAcceptanceFilter、impl Pagination for CustomerAcceptanceFilter、impl Repository<'a, CustomerAcceptance>，其中包含 search_customer_acceptances 和 find_by_acceptance_no；放置私有 customer_acceptance_projection。
    - 依赖/注意：通过 super::sort_doc 构建 accepted_at/created_at 白名单排序。CustomerAcceptanceFilter 由 mod.rs re-export，CustomerAcceptanceRow 保持 pub。acceptance_lines_by_acceptance_ids、allocations_by_acceptance_lines、allocations_by_fulfillment_fact 和 create_customer_acceptance_with_lines 涉及行集合或跨集合操作，应继续留在模块根的 FulfillmentRepository 中；这样 customer_acceptance.rs 只依赖共享 helper，不与电子交付或服务履约子模块形成循环引用。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
