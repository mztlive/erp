# `backend/services/src/mall_order/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/mall_order/mod.rs` |
| 扫描行数 | 1891 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | L |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 L，风险 medium）
- 摘要：建议将当前 1891 行模块拆为公共服务入口、事实接收、支付入账计划、成本评估、查询加载和视图组装六个域内文件；mod.rs 继续作为模块根，保留领域文档、dto 模块及 DTO re-export，并 re-export MallOrderService。拆分后预计各新文件约 180～520 行，均低于约 800 行。database 的 purchase_order 仓储和 entities 的 mall_order 聚合已经按目录分文件，无需在本次服务层拆分中新增仓储或实体文件。
- 拆分建议：
  - **backend/services/src/mall_order/service.rs**：放置公开服务类型 MallOrderService、构造函数 MallOrderService::new，以及四个公开入口 mall_order_list、mall_order_detail、mall_order_fact_list、receive_fact。该文件负责参数校验、公开流程入口、事实类型分派和顶层查询编排，不放置具体视图映射、成本计算或事务写入细节。
    - 依赖/注意：mod.rs 必须增加 mod service 和 pub use service::MallOrderService，以维持 services::mall_order::MallOrderService 的现有公共路径。由于其他兄弟模块中的 impl 需要访问数据库，db 字段应设为 pub(super)，或提供 pub(super) 访问器。该文件调用 query.rs、view.rs、fact_receiving.rs 中的方法时，对方方法必须为 pub(super)。
  - **backend/services/src/mall_order/fact_receiving.rs**：放置关键事实接收的具体事务编排：receive_payment、receive_cancel、receive_completion，以及原支付校验 ensure_original_payment 和幂等结果恢复 existing_received_view。保留支付事实、订单取消事实、订单完成事实与审计日志的事务边界。
    - 依赖/注意：上述方法由 service.rs 调用，应标记为 pub(super)。receive_payment 依赖 payment_plan.rs 的 build_payment_plan 和 PaymentPlan；返回视图依赖 view.rs 的 fact_view。不得改为调用其他 Service，继续只通过 DatabaseExt/Repository 跨域协作。事务闭包中的实体快照与审计写入顺序必须保持不变。
  - **backend/services/src/mall_order/payment_plan.rs**：放置消费入账写入计划 PaymentPlan，以及 MallOrderService::build_payment_plan、MallOrderService::ensure_conservation 和私有 helper attribution_for。负责订单、明细、支付来源、商品×支付来源分摊、消费事实的构造，选择履约链和归集状态，并校验行、列及订单金额守恒。
    - 依赖/注意：PaymentPlan 在本文件构造、在 fact_receiving.rs 的事务写入循环中读取，因此类型及 order、items、sources、allocations、entries、assessments、cost_entries、cost_allocations 字段需为 pub(super)，或提供等价的域内访问接口。build_payment_plan 需为 pub(super)，ensure_conservation 与 attribution_for 可继续私有。其对 cost_assessment.rs 的 build_cost_assessments 调用要求后者为 pub(super)。
  - **backend/services/src/mall_order/cost_assessment.rs**：放置 MallOrderService::build_cost_assessments、MallOrderService::none_assessment、MallOrderService::actual_assessment。负责按商品明细和资金分摊构造 ACTUAL/NONE 成本评估，并同步构造 D20 CostEntry 与 CostAllocation，包括来源排序、比例分摊、尾差处理及税额拆分。
    - 依赖/注意：build_cost_assessments 由 payment_plan.rs 调用，应标记为 pub(super)；none_assessment 和 actual_assessment 仅在本文件使用，可保持私有。虽然其中部分计算无 I/O，但该簇直接构造 cost 域的 CostEntry/CostAllocation，按 entities 的跨域依赖约束不宜整体下沉到 entities::mall_order。注意继续导入 std::str::FromStr、id_generator::next_id 和 D20/D29 相关实体。
  - **backend/services/src/mall_order/query.rs**：放置仓储筛选关联类型 MallOrderFilter、MallOrderFactFilter、事实分组类型 OrderFactMap，以及查询加载 helper：facts_grouped_by_order、load_facts_for_order、load_entries_for_sources、load_current_assessments。该文件只负责查询参数类型和仓储数据装载，不负责响应 DTO 映射。
    - 依赖/注意：两个筛选类型需要供 service.rs 使用，OrderFactMap 需要供 service.rs 和 view.rs 使用，均应设为 pub(super)。四个加载方法被其他兄弟模块调用，也应设为 pub(super)。query.rs 不应反向导入 view.rs，以维持 query 到 repository 的单一职责并避免逻辑循环。现有逐来源和逐消费查询可能存在 N+1，但本次只拆文件，不应混入查询行为优化。
  - **backend/services/src/mall_order/view.rs**：放置列表与详情响应组装：build_list_row、build_detail_view、build_conservation、fact_view、cost_assessment_view、mask_reference，以及列表投影/摘要和展示辅助项 OrderListRow、OrderFactSummary、AssessmentAmountString 及其 impl。负责事实摘要、支付构成、成本口径分项、守恒结果、敏感引用脱敏及实体到 DTO 的映射。
    - 依赖/注意：build_list_row 和 build_detail_view 由 service.rs 调用，应为 pub(super)；fact_view 还被 service.rs 和 fact_receiving.rs 使用，也应为 pub(super)。OrderListRow 由 service.rs 构造，因此类型及全部字段需为 pub(super)。build_conservation、cost_assessment_view、mask_reference、OrderFactSummary 和 AssessmentAmountString 可保持文件私有。该文件可调用 query.rs 的加载 helper，但 query.rs 不应依赖本文件。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
