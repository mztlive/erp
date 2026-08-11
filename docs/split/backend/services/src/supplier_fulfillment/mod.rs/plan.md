# `backend/services/src/supplier_fulfillment/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/supplier_fulfillment/mod.rs` |
| 扫描行数 | 1714 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：该文件同时包含服务类型、查询投影、供应商下单、取消/退款动作、外部结果登记、事务外网关派发和多组私有 helper，已形成至少五个清晰的业务内聚簇。建议让 mod.rs 继续作为模块根，只保留模块声明、DTO/gateway re-export，并从 service.rs re-export SupplierFulfillmentService；其余实现按查询、下单、售后动作、回调登记和派发结果处理拆分。预计各新文件约 120–520 行，均可控制在 800 行以内，且不需要改动 database 或 entities 层。
- 拆分建议：
  - **backend/services/src/supplier_fulfillment/service.rs**：放置 SupplierFulfillmentService、SupplierFulfillmentService::new，以及跨多个编排文件共享的最小基础设施：load_order、latest_place_action、ensure_capability、zero_amount、zero_quantity、qty_add、qty_sub、amount_sub、refund_fact_view、refund_allocation_view。该文件作为服务类型和共享 helper 的单一来源，预计约 180–250 行。
    - 依赖/注意：db 与 gateway 字段应声明为 pub(super)，以允许 query/place/after_sales/callbacks/dispatch 中的兄弟模块 impl 访问；load_order、latest_place_action、ensure_capability、zero_amount 和退款视图转换函数按实际跨模块使用声明为 pub(super)。通过 mod.rs 执行 pub use self::service::SupplierFulfillmentService，保持现有公共路径不变。
  - **backend/services/src/supplier_fulfillment/query.rs**：放置只读查询和详情组装：FulfillmentOrderFilter、supplier_fulfillment_order_list、supplier_fulfillment_order_detail、refund_views_for_order、item_view。负责分页过滤、订单详情聚合、退款事实批量读取和履约明细视图映射，预计约 250–330 行。
    - 依赖/注意：通过 super::service::SupplierFulfillmentService 扩展 inherent impl；调用 service.rs 中 pub(super) 的 load_order 和 refund_fact_view。SortDir 继续从 super::dto 引入。该模块只读 Repository，不应依赖 callbacks.rs，避免查询与命令形成循环依赖。
  - **backend/services/src/supplier_fulfillment/place.rs**：放置供应商下单完整编排：submit_place、ensure_placeable、ensure_mall_items、build_place_facts、build_place_items。保留下单幂等检查、跨域前置查询、事实构造、下单事务以及事务提交后的网关派发，预计约 380–460 行。
    - 依赖/注意：调用 service.rs 中 pub(super) 的 ensure_capability，并调用 dispatch.rs 中 pub(super) 的 build_action_message 和 settle_dispatch。下单事务闭包应整体迁移，保持 supplier_fulfillment、inbox_message、audit_log 的写入顺序和原有克隆语义，不要把网关调用移入事务。
  - **backend/services/src/supplier_fulfillment/after_sales.rs**：放置取消与退款动作提交簇：submit_cancel、submit_refund、submit_after_sales_action、build_after_sales_action、build_action_lines、ensure_action_lines、action_line_view。取消和退款共享幂等键、连接能力检查、动作头行构造、净余额校验和事务外派发，应作为一个文件维护，预计约 430–520 行。
    - 依赖/注意：调用 service.rs 中 pub(super) 的 load_order、ensure_capability、zero_amount、zero_quantity、qty_add、qty_sub、amount_sub；调用 dispatch.rs 中 pub(super) 的 build_action_message 和 settle_dispatch。不要将 CANCEL 与 REFUND 拆成两个文件，否则会复制公共编排并增加幂等和状态推进规则漂移风险。
  - **backend/services/src/supplier_fulfillment/callbacks.rs**：放置供应商外部结果登记：record_reject、record_refund_result、build_refund_fact、order_total_cost、build_refund_message。负责拒单事件幂等、退款结果幂等、状态历史、退款事实及分配行构建和事务写入，预计约 350–430 行。
    - 依赖/注意：调用 service.rs 中 pub(super) 的 load_order、latest_place_action、zero_amount 和 refund_fact_view。拒单与退款结果的事务闭包必须完整保留；record_refund_result 中 inbox_message、订单更新、退款事实及审计仍需处于同一事务。该文件不应反向被 query.rs 依赖。
  - **backend/services/src/supplier_fulfillment/dispatch.rs**：放置事务外供应商派发及结果承接：settle_dispatch、apply_dispatch_outcome、build_error_task、write_dispatch_result、build_action_message、supplier_source_system_id、outcome_label。集中管理 DispatchOutcome 到订单、动作、InboxMessage 和 IntegrationErrorTask 的状态映射，预计约 320–400 行。
    - 依赖/注意：settle_dispatch 和 build_action_message 需要 pub(super)，供 place.rs 与 after_sales.rs 调用；其余 helper 保持模块私有。使用 super::gateway::{DispatchOutcome, SupplierGateway} 或由 service 字段间接访问网关，避免依赖 mod.rs 的公共 re-export。必须继续保证 gateway.dispatch 位于事务外，而 write_dispatch_result 单独开启结果写回事务。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
