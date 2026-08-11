# `backend/entities/src/sales_order/working_copy.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/entities/src/sales_order/working_copy.rs` |
| 扫描行数 | 1139 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | low |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 low）
- 摘要：建议保留 working_copy.rs 作为工作副本表头聚合主文件，将工作副本枚举、工作副本行实体、跨工作副本/提交/修订共用的金额三元组校验，以及共享测试夹具拆到 sales_order 域目录中的独立文件。工作副本行具有独立的数据结构、构造规则和测试，是最明确的内聚簇；WorkingPurpose 与 WorkingCopyStatus 及其状态机实现构成稳定的值类型簇；validate_amount_triple 已被 revision.rs 和 submission.rs 复用，不应继续归属于 working_copy 模块。通过 working_copy.rs 和 sales_order/mod.rs 的 re-export 可保持现有公开 API 路径不变。预计拆分后主文件约 650 至 730 行，其余文件均远低于 800 行。
- 拆分建议：
  - **backend/entities/src/sales_order/working_copy_types.rs**：放置 WorkingPurpose、impl WorkingPurpose、WorkingCopyStatus、impl WorkingCopyStatus，以及 impl DocumentState for WorkingCopyStatus。该文件只负责工作副本的目的、状态代码、中文标签和合法状态迁移。
    - 依赖/注意：依赖 serde::{Serialize, Deserialize} 和 crate::common::state::DocumentState。建议在 sales_order/mod.rs 中声明私有 mod working_copy_types，并由 working_copy.rs 使用 pub use super::working_copy_types::{WorkingCopyStatus, WorkingPurpose} 重新导出，以保持 sales_order::working_copy::* 和 sales_order::* 两层公开路径。状态机测试迁移后若仍在 working_copy.rs 内调用 allowed_next，需要在测试模块显式导入 DocumentState，不能再依赖原文件顶层导入。
  - **backend/entities/src/sales_order/working_copy_line.rs**：放置工作副本行的全部生产代码：ITEM_NAME_MAX_LEN、SPEC_MAX_LEN、UNIT_MAX_LEN、SalesOrderWorkingCopyLineData、SalesOrderWorkingCopyLine、impl SalesOrderWorkingCopyLine 和 SalesOrderWorkingCopyLine::new；同时内联放置行实体测试 line_new_computes_amounts_and_normalizes、line_new_rejects_zero_no_mismatch_and_inconsistent_voucher、voucher_line_builds_with_derived_gift_rate。
    - 依赖/注意：依赖 BaseModel、Entity、Instant、SalesOrderWorkingCopyId、SalesOrderWorkingCopyLineId、SalesOrderLineId、SkuId、Amount、Quantity、Rate、UnitPrice、normalize_required_text、normalize_optional_text，以及 super::types::{build_line_groups, FulfillmentMode, GoodsLineFields, LineType, VoucherLineDraft, WelfareScenario}。该模块不应依赖 SalesOrderWorkingCopy 表头实体，避免生产代码循环依赖。建议在 sales_order/mod.rs 中声明私有 mod working_copy_line，再由 working_copy.rs pub use SalesOrderWorkingCopyLine 和 SalesOrderWorkingCopyLineData，确保原公开路径兼容。
  - **backend/entities/src/sales_order/amount_validation.rs**：放置销售单域共享的表头金额三元组校验函数 validate_amount_triple，保留其 gross = net + tax 业务说明和 pub(crate) 可见性。
    - 依赖/注意：仅依赖 crate::errors::{Error, Result} 和 crate::money::Amount。需要将 working_copy.rs、revision.rs、submission.rs 中的调用统一改为 super::amount_validation::validate_amount_triple。模块应保持私有，仅函数使用 pub(crate) 或改为 pub(super)；若确认调用方始终局限于 sales_order 域，优先使用 pub(super) 进一步收窄可见性。独立放置可消除 revision/submission 对 working_copy 模块的反向依赖，不会形成循环依赖。
  - **backend/entities/src/sales_order/working_copy_test_support.rs**：仅在 cfg(test) 下提供工作副本表头测试和工作副本行测试共用的构造函数：amt、rate、qty、price、goods_line、line_data、header_data。函数使用 pub(super) 或 pub(crate) 测试期可见性，不包含任何生产逻辑或测试用例。
    - 依赖/注意：必须通过 #[cfg(test)] mod working_copy_test_support 在 sales_order/mod.rs 中声明，避免进入生产构建。该模块会引用 SalesOrderWorkingCopyData、SalesOrderWorkingCopyLineData、HeaderSnapshotData、GoodsLineFields 及相关 ID/金额类型；建议使用明确的 super::working_copy、super::working_copy_line、super::snapshot 和 super::types 路径，避免迁移后 super::super 层级变化。测试期存在对工作副本模块的引用，但不构成生产代码循环依赖。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。

## 处理状态

- 状态：**已解决**
- 验证：实现与拆分方案匹配；`entities` 定向格式、编译、Clippy 与 960 项单元测试通过；独立 Review 通过。
- 说明：workspace 全量格式与 Clippy 门禁仍受本次范围外的既有问题阻断，详见执行记录。
