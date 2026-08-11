# `backend/database/src/repository/mall_after_sales.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/mall_after_sales.rs` |
| 扫描行数 | 1103 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议参考 database/src/repository/purchase_order/ 的目录模块模式，将 mall_after_sales.rs 替换为同名目录。以售后申请、退款聚合、余额恢复聚合三个业务簇拆分子文件，mod.rs 作为模块根统一维护集合常量、公开重导出和跨集合事务仓储。拆分后最大文件预计约 450 行，所有文件均可控制在 800 行以内，同时不改变 MallAfterSalesExt 和 Service 层调用方式。
- 拆分建议：
  - **backend/database/src/repository/mall_after_sales/mod.rs**：放置 D30 仓储模块级文档；声明 mod request、mod refund、mod balance_restoration；通过 pub use 重导出 MallAfterSalesRequestRow、MallAfterSalesRequestFilter、MallRefundRepository、MallRefundLineRepository、MallRefundAllocationRepository、MallBalanceRestorationRepository、MallBalanceRestorationAllocationRepository；保留 MALL_REFUNDS、MALL_REFUND_LINES、MALL_REFUND_ALLOCATIONS、MALL_BALANCE_RESTORATIONS、MALL_BALANCE_RESTORATION_ALLOCATIONS 常量；保留 MallAfterSalesRepository<'a>、MallAfterSalesRepository::new、create_refund_with_lines_and_allocations、create_balance_restoration_with_allocations。
    - 依赖/注意：继续从 super::extensions::MallAfterSalesExt 获取集合名的权威来源，维持与 indexes 侧一致；extensions/mall_after_sales.rs 依赖本模块重导出的公开类型，因此 pub use 不可遗漏。repository/mod.rs 中的 mod mall_after_sales; 无需修改，但必须删除原 mall_after_sales.rs，避免文件模块与目录模块同时存在。该模块与 extensions 的交叉引用沿用 purchase_order 模式，不应再新增反向子模块依赖。
  - **backend/database/src/repository/mall_after_sales/request.rs**：放置 MallAfterSalesRequestRow、MallAfterSalesRequestFilter；QueryFilter for MallAfterSalesRequestFilter、Pagination for MallAfterSalesRequestFilter；Repository<MallAfterSalesRequest> 的 search_after_sales_requests；Repository<MallAfterSalesRequestLine> 的 list_by_request；私有 helper sort_doc、after_sales_request_projection；测试 after_sales_request_filter_applies_optional_fields_and_deleted_filter、sort_doc_maps_only_whitelisted_fields_and_defaults_to_created_at。
    - 依赖/注意：移入子模块后，原 super::regex_filter 路径应改为 crate::repository::regex_filter::insert_literal_regex_filter，PageResult、Pagination、QueryFilter 建议从 crate::repository 导入，Repository、mongo_ops、Result 从 crate 根导入。sort_doc 与 after_sales_request_projection 仅在本文件使用，保持 fn 私有即可；测试继续作为 request.rs 内联 tests，通过 super 访问私有 helper。MallAfterSalesRequestRow 和 MallAfterSalesRequestFilter 应由 mod.rs 重导出，以保持扩展关联类型和查询返回类型的可达性。已知 MallAfterSalesRequest created_at 序列化缺陷仅保留说明，不在此次拆分中修复。
  - **backend/database/src/repository/mall_after_sales/refund.rs**：放置 MallRefundRepository 及 new、create、find_by_id、find_by_fact_id、find_by_identity、list_by_after_sales_request、list_by_order、collection；MallRefundLineRepository 及 new、create、find_by_id、list_by_refund、list_by_refunds、collection；MallRefundAllocationRepository 及 new、create、find_by_id、list_by_lines、list_by_consumption、collection。
    - 依赖/注意：三个仓储共同组成退款聚合且共享 MongoDB、Executor、mongo_ops、FindOptions 与未删除过滤依赖，放在同一文件可保持内聚并避免过度拆分。集合名不要在本文件重复定义，应通过 super::{MALL_REFUNDS, MALL_REFUND_LINES, MALL_REFUND_ALLOCATIONS} 引用模块根常量。collection helper 继续保持私有；三个 Repository 类型必须由 mod.rs pub use，供 MallAfterSalesExt 的返回类型使用。
  - **backend/database/src/repository/mall_after_sales/balance_restoration.rs**：放置 MallBalanceRestorationRepository 及 new、create、find_by_id、find_by_fact_id、find_by_identity、list_by_after_sales_request、collection；MallBalanceRestorationAllocationRepository 及 new、create、find_by_id、list_by_restoration、list_by_refund_allocation、collection。
    - 依赖/注意：余额恢复头与恢复分配属于同一聚合，合并后预计约 300 行，无需进一步拆分。集合名通过 super::{MALL_BALANCE_RESTORATIONS, MALL_BALANCE_RESTORATION_ALLOCATIONS} 引用模块根常量；两个 collection helper 保持私有。两个公开仓储类型应由 mod.rs 重导出，避免 extensions/mall_after_sales.rs 改变导入路径。该文件只依赖父模块常量，不应引用 refund.rs，以避免形成子模块之间的耦合或潜在循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
