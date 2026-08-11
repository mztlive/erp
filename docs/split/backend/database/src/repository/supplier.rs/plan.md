# `backend/database/src/repository/supplier.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/database/src/repository/supplier.rs` |
| 扫描行数 | 1214 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议参考 database/src/repository/purchase_order/ 的目录模式，将 supplier.rs 改为 supplier/ 域目录，由 mod.rs 保持模块根、统一 re-export，并按供应商账户、商务版本、能力、资质聚合拆分查询代码。跨集合事务仓储和体量很小的幂等命令查询保留在 mod.rs，共享的排序与供应商 ID 投影查询下沉到 common.rs。这样各文件预计约 80 至 380 行，均明显低于 800 行，同时保持 Repository 屏蔽查询细节、聚合事务入口位于域模块根的仓库约定。
- 拆分建议：
  - **backend/database/src/repository/supplier/mod.rs**：作为 supplier 仓储模块根：声明 `mod account`、`mod capability`、`mod commercial_profile`、`mod common`、`mod qualification`；re-export `SupplierAccountRow`、`SupplierAccountFilter`、`SupplierCommercialProfileRow`、`SupplierCommercialProfileFilter`、`SupplierCapabilityRow`、`SupplierCapabilityFilter`、`SupplierQualificationRow`、`SupplierQualificationFilter`；保留集合常量 `SUPPLIER_ACCOUNTS`、`SUPPLIER_COMMERCIAL_PROFILE_REVISIONS`、`SUPPLIER_QUALIFICATION_CAPABILITIES`；放置 `SupplierRepository` 及其事务写入实现；同时放置体量很小的 `Repository<SupplierProfileCommand>` 幂等查询实现。
    - 依赖/注意：原 `supplier.rs` 必须删除，不能与 `supplier/mod.rs` 同时存在。`repository/mod.rs` 中的 `mod supplier;` 可保持不变。必须公开 re-export 四组 Filter，使 `repository/extensions/supplier.rs` 中 `super::super::supplier::{...}` 的路径继续有效；同时 re-export `SupplierAccountRow`，保证上层 `pub use supplier::SupplierAccountRow` 和 Service 现有路径不变。集合常量继续通过 `SupplierExt` 关联常量取得，做法与 purchase_order 模块根一致。
  - **backend/database/src/repository/supplier/common.rs**：放置跨账户之外多个子模块共用的仓储 helper：`SupplierIdRow`、泛型异步函数 `find_supplier_ids<T>`、`sort_doc`；新增 `SUPPLIER_ACCOUNT_SORT_FIELDS`、`COMMERCIAL_PROFILE_SORT_FIELDS`、`SUPPLIER_CAPABILITY_SORT_FIELDS`、`SUPPLIER_QUALIFICATION_SORT_FIELDS` 四个 `pub(super)` 白名单常量，替代各查询中的内联字符串数组；迁入排序白名单单元测试。
    - 依赖/注意：`find_supplier_ids` 和 `sort_doc` 应使用 `pub(super)`，仅供 supplier 域子模块访问。`common.rs` 只能依赖 `crate::Repository`、`Executor`、`mongo_ops`、MongoDB BSON、serde 和实体 ID，不应反向导入 `account`、`capability`、`qualification`，避免形成子模块循环依赖。
  - **backend/database/src/repository/supplier/account.rs**：放置供应商账户集合的完整查询簇：`SupplierAccountRow`、`SupplierAccountFilter`、对应 `QueryFilter` 与 `Pagination` 实现；`impl Repository<SupplierAccount>` 中的 `search_supplier_accounts`、`find_by_supplier_no`、`find_by_party`；私有 helper `insert_supplier_id_constraints`、`supplier_id_strings`、`supplier_account_projection`；迁入账户筛选测试。
    - 依赖/注意：`insert_supplier_id_constraints`、`supplier_id_strings`、`supplier_account_projection` 保持文件私有，无需扩大可见性。排序改用 `super::common::{sort_doc, SUPPLIER_ACCOUNT_SORT_FIELDS}`。字面量正则 helper 应改为稳定的 crate 路径 `crate::repository::regex_filter::insert_literal_regex_filter`，避免依赖拆分前的 `super` 层级。
  - **backend/database/src/repository/supplier/commercial_profile.rs**：放置商务结算修订集合的查询簇：`SupplierCommercialProfileRow`、`SupplierCommercialProfileFilter`、对应 `QueryFilter` 与 `Pagination` 实现；`impl Repository<SupplierCommercialProfileRevision>` 中的 `search_commercial_profiles`、`find_by_supplier_and_revision`、`list_revision_history`；私有投影 helper `commercial_profile_projection`。
    - 依赖/注意：`commercial_profile_projection` 保持文件私有。排序通过 `super::common::{sort_doc, COMMERCIAL_PROFILE_SORT_FIELDS}` 获取。该文件只处理追加式商务修订查询，不应引用 `SupplierRepository` 或账户写入逻辑，从而保持修订历史查询与聚合事务写入解耦。
  - **backend/database/src/repository/supplier/capability.rs**：放置供应商能力集合的完整查询簇：`SupplierCapabilityRow`、`SupplierCapabilityFilter`、对应 `QueryFilter` 与 `Pagination` 实现；`impl Repository<SupplierCapability>` 中的 `search_supplier_capabilities`、`find_by_supplier_and_code`、`list_active_for_expiry_warning`、`find_supplier_ids_by_active_capability_codes`；私有投影 helper `supplier_capability_projection`；迁入能力筛选测试。
    - 依赖/注意：`supplier_capability_projection` 保持文件私有。供应商 ID 去重查询调用 `super::common::find_supplier_ids`，排序调用 `super::common::{sort_doc, SUPPLIER_CAPABILITY_SORT_FIELDS}`。不要把 `find_supplier_ids` 复制到该文件，否则资质模块会形成重复规则源。
  - **backend/database/src/repository/supplier/qualification.rs**：放置资质聚合及资质到能力关联查询：`SupplierQualificationRow`、`SupplierQualificationFilter`、对应 `QueryFilter` 与 `Pagination` 实现；`impl Repository<SupplierQualification>` 中的 `search_supplier_qualifications`、`list_active_for_expiry_warning`、`find_supplier_ids_by_qualification_types`、`find_supplier_ids_by_valid_qualifications`、`find_supplier_ids_by_expiring_qualifications`、`find_supplier_ids_by_expired_qualifications`；`impl Repository<SupplierQualificationCapability>` 中的 `list_by_qualification_ids`、`list_by_capability_id`；私有 helper `qualification_type_filter`、`supplier_qualification_projection`；迁入资质筛选测试。
    - 依赖/注意：资质与 `SupplierQualificationCapability` 放在同一文件，因为关联行以资质为主要批量读取入口，仍属于同一聚合查询簇。`qualification_type_filter` 和投影 helper 保持文件私有；供应商 ID 查询复用 `super::common::find_supplier_ids`，排序复用 `SUPPLIER_QUALIFICATION_SORT_FIELDS`。跨集合替换关联的写入方法 `replace_qualification_capabilities` 必须留在模块根的 `SupplierRepository`，避免把事务写入与单集合查询混在一起。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
