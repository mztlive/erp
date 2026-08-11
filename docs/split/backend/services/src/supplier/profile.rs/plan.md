# `backend/services/src/supplier/profile.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/supplier/profile.rs` |
| 扫描行数 | 1952 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | L |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 L，风险 medium）
- 摘要：建议将约 1952 行的 supplier/profile.rs 替换为 profile/ 子目录，并以 profile/mod.rs 作为模块根统一 re-export SupplierProfileService。现有代码可清晰分成公开根命令、创建事务载荷、修订与 Party 事实、能力/资质同步、共享幂等与校验规则五个业务簇；拆分后各业务文件预计均低于 700 行，满足约 800 行以内的目标。主要风险不是业务行为，而是 Rust 子模块间私有可见性、固有 impl 跨文件调用和事务载荷类型的访问范围；统一使用 pub(super)、保持 service 到 create/update 再到 capability_qualification/shared 的依赖方向，并保留原 profile 公共路径，可将风险控制在中等水平。
- 拆分建议：
  - **backend/services/src/supplier/profile/mod.rs**：作为 supplier::profile 的模块根，声明 mod service、mod create、mod update、mod capability_qualification、mod shared，并通过 pub use self::service::SupplierProfileService 对外导出唯一公开服务类型。文件只保留模块文档、模块声明和 re-export，预计少于 40 行。
    - 依赖/注意：用该文件替代原 profile.rs，并删除原文件，否则 Rust 模块解析会发生 profile.rs 与 profile/mod.rs 冲突。backend/services/src/supplier/mod.rs 保留 pub mod profile，并建议增加 pub use self::profile::SupplierProfileService；这样既保持 services::supplier::profile::SupplierProfileService 兼容，也提供领域根级 re-export。
  - **backend/services/src/supplier/profile/service.rs**：放置公开类型 SupplierProfileService，以及 impl SupplierProfileService 中的 new、create、update、command_result、command_record、resolve_transaction_result、reveal_sensitive、validate_request、ensure_party_active、ensure_attachment_references、ensure_unique_inputs。该文件负责公开用例入口、事务启动、幂等恢复、请求形态校验、外部引用预检和敏感字段揭示，预计约 350 至 450 行。
    - 依赖/注意：SupplierProfileService 的 db 与 sensitive_data 字段需标记 pub(super)，供 create.rs、update.rs、capability_qualification.rs 中的固有 impl 访问。create 和 update 分别调用兄弟模块中的 prepare_create、prepare_update，并调用其事务载荷 persist，因此这些方法、返回类型和 persist 应仅暴露为 pub(super)。从 shared.rs 导入 request_fingerprint、replay_command、command_view、required_create_identity、ensure_sensitive_party、ensure_qualification_sensitivity。不要让 shared.rs 反向依赖本文件。
  - **backend/services/src/supplier/profile/create.rs**：放置创建场景的聚合构造与持久化：SupplierProfileService::prepare_create、create_contact、create_address、create_bank_account；PreparedCreate 及 PreparedCreate::persist；create_party_entities、create_supplier_entities、create_tax_profile、create_rating。能力和资质首版集合通过 capability_qualification.rs 的 create_capabilities、create_qualifications 构造，预计约 500 至 600 行。
    - 依赖/注意：PreparedCreate、prepare_create 和 PreparedCreate::persist 需要 pub(super)，供 service.rs 启动事务并提交。create_capabilities、create_qualifications 以及 CreatedCapabilities 的必要字段从 capability_qualification.rs 以 pub(super) 暴露。command_view 从 shared.rs 引入。该文件只负责实体构造和事务写入，不应反向调用公开 create 用例，以免形成流程循环。
  - **backend/services/src/supplier/profile/update.rs**：放置修订根聚合、Party 从属事实和评级变更：SupplierProfileService::prepare_update、load_supplier_for_update、load_party_for_update、next_party_revision_no、next_profile_revision_no、prepare_party_facts、prepare_rating_changes；PartyFactChanges、RatingChanges、PreparedUpdate 及其全部 impl；update_party、update_commercial_profile、disable_contacts、disable_addresses、disable_tax_profiles、disable_bank_accounts。预计约 550 至 700 行。
    - 依赖/注意：PreparedUpdate、PreparedUpdate::new、PreparedUpdate::persist、PartyFactChanges 和 RatingChanges 仅需 pub(super)。该文件依赖 capability_qualification.rs 的 CapabilityChanges、QualificationChanges、prepare_capability_changes、prepare_qualification_changes 及对应 persist；这些跨兄弟模块项应使用 pub(super)。从 shared.rs 引入 command_view、required_update_version、ensure_version、next_revision_no、option_as_authoritative_update。事务入口仍留在 service.rs，避免该文件自行建立第二层事务。
  - **backend/services/src/supplier/profile/capability_qualification.rs**：集中供应商能力和资质集合的创建、差异计算、快照及持久化：SupplierProfileService::prepare_capability_changes、capability_revision、prepare_qualification_changes、qualification_revision；CapabilityChanges、QualificationChanges、CreatedCapabilities 及对应 persist；new_capability、apply_qualification_input、new_qualification、qualification_snapshot、qualification_links、qualification_key、qualification_matches_input、create_capabilities、create_qualifications。预计约 600 至 700 行。
    - 依赖/注意：CapabilityChanges、QualificationChanges 及 persist 需要 pub(super) 供 update.rs 使用；CreatedCapabilities、create_capabilities、create_qualifications 需要 pub(super) 供 create.rs 使用。共享的 next_revision_no、option_as_authoritative_update 从 shared.rs 导入。该文件不得依赖 PreparedCreate 或 PreparedUpdate，以维持 capability_qualification 到 shared 的单向结构。qualification_matches_input 和 qualification_key 属于无 I/O 规则，后续若在其他 Service 重复，应按仓库约定下沉到 entities，而不是复制。
  - **backend/services/src/supplier/profile/shared.rs**：放置跨创建、修订和公开命令共用的纯 helper：option_as_authoritative_update、next_revision_no、request_fingerprint、replay_command、required_create_identity、required_update_version、ensure_version、ensure_sensitive_party、ensure_qualification_sensitivity、command_view；同时迁移原 tests 模块及 legal_person_attachment_requires_highest_sensitivity、command_replay_is_bound_to_supplier 两个测试。预计约 180 至 250 行。
    - 依赖/注意：被兄弟模块使用的 helper 标记为 pub(super)，不要使用 pub(crate) 或 pub，避免扩大服务公共 API。shared.rs 只能依赖 DTO、entities、Error/Result 等基础类型，不能依赖 service.rs、create.rs、update.rs 或 capability_qualification.rs，从而避免潜在循环依赖。ensure_qualification_sensitivity 等确定性规则未来可下沉实体或值对象，但本次拆分应先保持实现与测试不变。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
