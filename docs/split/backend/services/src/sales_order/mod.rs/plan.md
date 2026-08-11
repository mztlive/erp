# `backend/services/src/sales_order/mod.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/sales_order/mod.rs` |
| 扫描行数 | 2231 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | L |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 L，风险 medium）
- 摘要：目标文件同时承载 Service 根类型、建单与草稿事务、提交事务、列表与详情查询、待办责任人解析、视图映射、纯领域资格规则和测试，内聚边界清晰且拆分后各文件可控制在约 800 行以内。建议让 mod.rs 保持模块根及 re-export，将服务编排按草稿、提交、查询拆分；将公共视图转换集中到 view_mapping.rs；并依据仓库约定把不依赖数据库的结案资格、改单资格和行金额汇总规则下沉到 entities/src/sales_order/rules.rs。
- 拆分建议：
  - **backend/services/src/sales_order/service.rs**：放置 SalesOrderService 结构体、SalesOrderService::new，以及跨草稿和提交流程共享的公司商品池资格校验方法：ensure_sellable_draft_lines、ensure_sellable_working_copy_lines、sellable_working_copy_refs、ensure_sellable_refs。
    - 依赖/注意：SalesOrderService.db 从父模块移入子模块后应声明为 pub(super) 或提供域内访问器，否则兄弟模块中的 impl 无法访问。供 draft.rs 和 submission.rs 调用的校验方法应使用 pub(super)。该文件依赖 CatalogExt、Executor、NoTransaction、BusinessDate、SalesOrderDraftLineRequest 和 SalesOrderWorkingCopyLine；mod.rs 通过 pub use self::service::SalesOrderService 保持外部 API 不变。
  - **backend/services/src/sales_order/draft.rs**：放置建单、保存草稿和作废流程的 impl SalesOrderService：create_sales_order、save_working_copy、void_sales_order；同时放置草稿聚合构造函数 build_stable_lines、build_working_copy、build_working_copy_line_datas、materialize_working_copy_lines、build_working_copy_lines、header_snapshot、draft_hash。
    - 依赖/注意：通过 super::service::SalesOrderService 使用服务类型和共享 SKU 校验；金额汇总改用 entities::sales_order::sales_order_line_totals。create_sales_order 会调用定义在 submission.rs 的 submit_sales_order，create_sales_order 和 void_sales_order 会调用定义在 query.rs 的 sales_order_detail，但它们都是同一类型的固有方法，不需要模块间互相 re-export。事务闭包仍必须在 Service 层创建，并继续向 Repository 传入 session。
  - **backend/services/src/sales_order/submission.rs**：放置销售单提交事务及提交快照构造：submit_sales_order、build_submission、build_submission_lines、working_copy_goods、working_copy_voucher。
    - 依赖/注意：使用 service.rs 中 pub(super) 的 ensure_sellable_working_copy_lines、sellable_working_copy_refs 和 ensure_sellable_refs；使用 entities::sales_order::sales_order_line_totals 汇总金额；通过 super::view_mapping::submission_view 构造返回 DTO。避免从 query.rs 引入 submission_view，以免形成 query 与 submission 的双向依赖。提交、订单审核轨、审批或采购确认、待办和审计仍须保留在同一 MongoDB 事务中。
  - **backend/services/src/sales_order/query.rs**：放置 SalesOrderFilter 类型别名和只读查询编排：sales_order_list、sales_order_detail、working_copy_view、account_name、resolve_stage_owner、resolve_stage_owners_batch；保留仅供应收金额 fold 初始化使用的私有 zero_amount，或直接用等价的确定性零金额构造。
    - 依赖/注意：依赖 view_mapping.rs 中 pub(super) 的 detail_owner_user_id、stage_code_label_tone、submission_view、working_copy_line_view 和领域资格结果到 DTO 的转换函数。列表责任人解析仍应保持当前批量查询方案，不能因拆分退化为 N+1。结案与改单资格应调用 entities::sales_order::rules 中的纯领域函数，再映射为服务 DTO；查询继续使用 NoTransaction。
  - **backend/services/src/sales_order/view_mapping.rs**：集中放置无数据库 I/O 的服务视图映射：detail_owner_user_id、stage_code_label_tone、submission_view、working_copy_line_view、submission_line_view；新增 close_eligibility_view，将实体层 CloseEligibility 转为 dto::CloseEligibilityView；新增 sales_change_eligibility_message，将实体层 SalesChangeEligibility 和 blocker 转为 can_start_sales_change_order 与中文 change_order_blocker。保留负责人优先级、阶段展示和中文映射相关测试。
    - 依赖/注意：函数需按调用范围声明 pub(super)，不得公开到 crate 外。该模块只依赖 dto 和 entities，不依赖 Database，供 query.rs 与 submission.rs 单向调用。领域层 blocker 应在这里转换为中文，避免 entities 依赖 services DTO 或展示文案。
  - **backend/entities/src/sales_order/rules.rs**：放置销售单聚合的纯业务规则和金额汇总：CloseEligibility、CloseEligibilityBlocker、SalesChangeEligibility、SalesChangeBlocker、compute_close_eligibility、compute_can_start_sales_change；将 LineAmounts 重命名为公开且领域化的 SalesOrderLineAmounts，为 SalesOrderWorkingCopyLine、SalesOrderSubmissionLine 实现该 trait，并提供 sales_order_line_totals。迁移结案资格、改单资格和金额汇总相关单元测试。
    - 依赖/注意：必须在 backend/entities/src/sales_order/mod.rs 中声明 mod rules 并 re-export 必要类型和函数。entities 不得依赖 services，因此不能返回 CloseEligibilityView，也不应接收 stage_code 或 stage_label；改单判断应直接接收 OriginSystem、CommercialStatus、ReviewStatus、CloseStatus 和 has_active_change_order。公共类型和函数须补充符合 AGENTS.md 的多行文档注释。该金额汇总还可替代 sales_review/sales_change_order.rs 中近似重复的 change_line_totals，但后者可作为后续清理，避免扩大本次拆分范围。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
