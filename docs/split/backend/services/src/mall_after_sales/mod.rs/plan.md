# `backend/services/src/mall_after_sales/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/mall_after_sales/mod.rs` |
| 扫描行数 | 1017 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议按“服务定义与共用校验、退款写入、余额恢复写入、查询分页”四个内聚簇拆分。mod.rs 继续作为 mall_after_sales 模块根，仅保留领域文档、子模块声明、DTO re-export 和 MallAfterSalesService re-export，从而保持 Handler 的公共导入路径不变。退款和余额恢复各自包含独立事务与累计金额校验，拆开后职责清晰；三个列表查询共享排序和分页逻辑，集中到 query.rs 更合适。拆分后各文件预计均低于 500 行，满足约 800 行以内的目标。主要风险是 Rust 兄弟模块之间的字段和 helper 可见性，需要将 db、load_original_payment、hit_view 精确调整为 pub(super)，并避免业务子模块互相引用。
- 拆分建议：
  - **backend/services/src/mall_after_sales/service.rs**：放置公开类型 MallAfterSalesService、构造函数 MallAfterSalesService::new，以及退款和余额恢复共同使用的 MallAfterSalesService::load_original_payment 和自由函数 hit_view。mod.rs 通过 pub use self::service::MallAfterSalesService 保持现有公共导入路径。
    - 依赖/注意：MallAfterSalesService.db 需改为 pub(super)，供 refund.rs、balance_restoration.rs 和 query.rs 中的兄弟模块 impl 访问；load_original_payment 和 hit_view 需改为 pub(super)。该文件依赖 mongodb::Database、MallOrderExt、NoTransaction、MallOrderFactId、MallOrderFact、FactType、ProcessingStatus、ReceivedFactView、Error 和 Result。共用 helper 集中在此文件，避免 refund.rs 与 balance_restoration.rs 互相引用。
  - **backend/services/src/mall_after_sales/refund.rs**：放置退款事实接收和退款写入计划的完整编排：MallAfterSalesService::receive_refund、MallAfterSalesService::build_refund_plan、MallAfterSalesService::refunded_net_for_entry。包括幂等检查、原订单及订单行加载、退款事实、退款头、退款行、退款分配、消费反向事实构造、累计退款上限校验和事务写入。
    - 依赖/注意：通过 super::service::{MallAfterSalesService, hit_view} 使用服务类型和共用幂等视图 helper，并通过 self.load_original_payment 调用 pub(super) 共用校验。build_refund_plan 与 refunded_net_for_entry 保持文件私有。需要引入 AccessControlExt、MallAfterSalesExt、MallOrderExt、NoTransaction、Transactional、AuditActor、退款相关实体与 ID、Amount、Instant、next_id、Validate 和 FromStr。不要依赖 balance_restoration.rs，以免形成跨业务子模块耦合。
  - **backend/services/src/mall_after_sales/balance_restoration.rs**：放置卡券余额恢复事实接收和恢复分配计划：MallAfterSalesService::receive_balance_restoration、MallAfterSalesService::build_restoration_allocations、MallAfterSalesService::restored_for_refund_allocation。包括幂等检查、原支付校验、原退款分配和卡实例归属校验、累计恢复上限校验，以及事实、恢复头、恢复分配和审计的事务写入。
    - 依赖/注意：通过 super::service::{MallAfterSalesService, hit_view} 使用服务类型和共用 helper，并通过 self.load_original_payment 调用 pub(super) 原支付校验。build_restoration_allocations 与 restored_for_refund_allocation 保持文件私有。需要引入 AccessControlExt、CardInstanceExt、MallAfterSalesExt、MallOrderExt、NoTransaction、Transactional、AuditActor、恢复相关实体与 ID、MallPaymentSource、Amount、Instant、next_id、Validate 和 FromStr。不要从 refund.rs 调用私有实现；两条写入流程只通过 service.rs 共享最小逻辑。
  - **backend/services/src/mall_after_sales/query.rs**：放置三个只读查询入口 MallAfterSalesService::mall_refund_list、MallAfterSalesService::mall_balance_restoration_list、MallAfterSalesService::after_sales_request_list，以及私有类型别名 AfterSalesRequestFilter 和查询专用 helper sort_refunds、sort_restorations、slice_page。
    - 依赖/注意：通过 super::service::MallAfterSalesService 引用服务类型，通过 super::dto 引用 SortDir、PageParams 和查询 DTO/视图。AfterSalesRequestFilter、sort_refunds、sort_restorations、slice_page 均保持文件私有。需要引入 MallAfterSalesExt、NoTransaction、MallRefund、MallBalanceRestoration 和 Validate。该模块不应依赖 refund.rs 或 balance_restoration.rs；查询与写入编排完全隔离。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
