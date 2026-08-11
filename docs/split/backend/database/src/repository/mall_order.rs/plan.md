# `backend/database/src/repository/mall_order.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/mall_order.rs` |
| 扫描行数 | 1328 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：该文件同时承载关键事实、订单追溯、消费与成本评估、跨集合事务写入及共享查询 helper，已形成多个明确的领域内聚簇。建议参照 repository/purchase_order/ 的目录模式，将原 mall_order.rs 替换为 mall_order/mod.rs，并按 fact、order、consumption、common 拆分。mod.rs 保留模块说明、集合常量、公开重导出和跨集合 MallOrderRepository。拆分后最大文件预计约 500 行，所有文件均可控制在约 800 行以内，同时保持 MallOrderExt 当前使用的模块根类型路径不变。
- 拆分建议：
  - **backend/database/src/repository/mall_order/mod.rs**：作为 D29 仓储模块根：保留模块级说明；声明 mod common、mod fact、mod order、mod consumption；通过 pub use 重导出 MallOrderFactFilter、MallOrderFactRepository、MallOrderCancelFactRepository、MallOrderCompletionFactRepository、MallOrderFilter、MallConsumptionEntryFilter、MallConsumptionEntryRepository、MallConsumptionCostAssessmentRepository；保留集合常量 MALL_ORDER_FACTS、MALL_ORDER_CANCEL_FACTS、MALL_ORDER_COMPLETION_FACTS、MALL_ORDERS、MALL_CONSUMPTION_ENTRIES、MALL_CONSUMPTION_COST_ASSESSMENTS；保留 MallOrderRepository 及其 new、create_payment_fact_with_order。
    - 依赖/注意：拆分完成后必须删除原 backend/database/src/repository/mall_order.rs，避免同一模块同时存在 mall_order.rs 与 mall_order/mod.rs。上级 repository/mod.rs 的 mod mall_order; 可保持不变。mod.rs 需继续导入 super::extensions::MallOrderExt 以生成集合常量；extensions/mall_order.rs 已依赖 mall_order 模块根类型，因此所有被其导入的筛选和仓储类型必须在此 pub use。该关联属于现有的模块解析依赖，不要再让子文件直接依赖 extensions，以免扩大循环引用。三个 Row 类型可按 purchase_order 模式不重导出；若 private_interfaces 门禁报错，则也应从模块根重导出。
  - **backend/database/src/repository/mall_order/common.rs**：放置跨多个列表查询共享的私有排序 helper：sort_doc；放置测试 sort_doc_maps_only_whitelisted_fields_and_defaults_to_created_at。函数使用 pub(super) 可见性，仅向 mall_order 的兄弟子模块开放。
    - 依赖/注意：sort_doc 原为 mall_order.rs 文件私有函数，迁移后必须改为 pub(super) fn sort_doc，fact.rs、order.rs、consumption.rs 通过 super::common::sort_doc 引用。不要提升为 pub，避免把内部 MongoDB 排序实现暴露为数据库 crate 的公共 API。
  - **backend/database/src/repository/mall_order/fact.rs**：放置关键事实领域簇：MallOrderFactRow、MallOrderFactFilter、MallOrderFactFilter 的 QueryFilter/Pagination 实现；MallOrderFactRepository 及 new、create、find_by_id、find_by_business_fact_key、find_by_inbox_message、search_facts、list_by_after_sales_request、collection；MallOrderCancelFactRepository 及 new、create、find_by_id、find_by_fact_id、collection；MallOrderCompletionFactRepository 及 new、create、find_by_id、find_by_fact_id、collection；私有投影 helper order_fact_projection；测试 fact_filter_applies_optional_fields_and_deleted_filter。
    - 依赖/注意：通过 super::{MALL_ORDER_FACTS, MALL_ORDER_CANCEL_FACTS, MALL_ORDER_COMPLETION_FACTS} 使用模块根集合常量，通过 super::common::sort_doc 使用共享 helper。原 super::regex_filter::insert_literal_regex_filter 在目录子模块中路径会失效，必须改为 crate::repository::regex_filter::insert_literal_regex_filter 或 super::super::regex_filter::insert_literal_regex_filter。三个仓储类型和 MallOrderFactFilter 必须由 mod.rs pub use，供 extensions/mall_order.rs 使用。各 collection helper 继续保持私有，不应迁移到 common.rs。
  - **backend/database/src/repository/mall_order/order.rs**：放置订单追溯聚合查询簇：MallOrderRow、MallOrderFilter 及其 QueryFilter/Pagination 实现；Repository<MallOrder>::search_orders；Repository<MallOrderItem>::list_items_by_order；Repository<MallPaymentSource>::list_by_order、list_by_card_instance；Repository<MallItemFundingAllocation>::list_by_items、list_by_payment_source；私有投影 helper mall_order_projection；测试 order_filter_applies_time_range_and_pagination。
    - 依赖/注意：通过 super::common::sort_doc 共享排序构造。原 super::regex_filter 路径必须改为 crate::repository::regex_filter。PageResult、Pagination、QueryFilter 建议按 purchase_order/order.rs 模式从 crate::repository 导入，Repository、mongo_ops、Result 从 crate 根导入。MallOrderFilter 必须由 mod.rs pub use，以保持 MallOrderExt 关联类型定义不变。list_by_items 内部 ID 字符串转换只服务资金分摊查询，应继续留在本文件，暂不抽到 common.rs。
  - **backend/database/src/repository/mall_order/consumption.rs**：放置消费和成本评估簇：MallConsumptionEntryRow、MallConsumptionEntryFilter 及其 QueryFilter/Pagination 实现；MallConsumptionEntryRepository 及 new、create、find_by_id、search_consumption_entries、list_by_original_payment_source、collection；MallConsumptionCostAssessmentRepository 及 new、create、find_by_id、list_by_entry、collection；私有投影 helper consumption_entry_projection；测试 consumption_entry_filter_maps_fields_and_status。
    - 依赖/注意：通过 super::{MALL_CONSUMPTION_ENTRIES, MALL_CONSUMPTION_COST_ASSESSMENTS} 使用集合常量，通过 super::common::sort_doc 使用排序 helper。MallConsumptionEntryFilter、MallConsumptionEntryRepository、MallConsumptionCostAssessmentRepository 必须由 mod.rs pub use，供 MallOrderExt 的关联类型和访问器返回类型使用。两个 collection helper 应分别留在各自 impl 内，避免把具体实体集合类型耦合进 common.rs。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
