# `backend/services/src/customer/profile.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/customer/profile.rs` |
| 扫描行数 | 1791 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | L |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 L，风险 medium）
- 摘要：该文件同时承担客户资料服务入口、创建/修订事务、幂等命令、Party 时态事实差异、详情查询装配、敏感字段揭示和输入校验，职责边界已经明显分化。建议将 profile.rs 改为 profile/ 目录模块，由 mod.rs 作为模块根并重新导出 CustomerProfileService；其余逻辑按服务基础设施、根命令、从属事实、详情查询和敏感字段五个内聚簇拆分。预计最大的 mutation.rs 和 facts.rs 均可控制在约 650～750 行，所有文件可保持在约 800 行以内，同时维持 services::customer::profile::CustomerProfileService 现有调用路径。
- 拆分建议：
  - **backend/services/src/customer/profile/mod.rs**：作为 profile 子域模块根，声明 mod service、mod mutation、mod facts、mod query、mod sensitive，并通过 pub use service::CustomerProfileService 重新导出公开服务类型。可在此保留原文件顶部的模块级文档，说明客户资料根命令、事务边界和敏感数据职责。
    - 依赖/注意：不能让 profile.rs 与 profile/mod.rs 同时存在。backend/services/src/customer/mod.rs 保留 pub mod profile；建议另加 pub use profile::CustomerProfileService，但必须继续兼容现有 services::customer::profile::CustomerProfileService 路径。模块声明顺序不会解决可见性问题，跨兄弟模块成员仍需显式 pub(super)。
  - **backend/services/src/customer/profile/service.rs**：放置公开类型 CustomerProfileService、构造方法 CustomerProfileService::new，以及跨用例共享的仓储加载方法 load_customer、load_party、ensure_user_exists、next_revision_no。该文件只负责服务状态和公共基础设施，不放创建、查询或敏感字段流程。
    - 依赖/注意：db 和 sensitive_data 需要声明为 pub(super)，或提供 pub(super) 访问器，使 mutation.rs、facts.rs、query.rs、sensitive.rs 中的 impl 块能够访问。共享加载方法也需要 pub(super)，因为 Rust 兄弟模块不能调用定义在 service.rs 中的普通私有方法。service.rs 不应依赖其他 profile 子模块，以保持依赖方向单向。
  - **backend/services/src/customer/profile/mutation.rs**：放置创建、修订和幂等结果查询流程：CustomerProfileService::create、update、command_result、command_record、resolve_transaction、prepare_create、prepare_update；事务载荷 PreparedCreateParts、PreparedCreate、PreparedUpdate 及其 new/persist；根聚合构造函数 create_party、create_customer、create_owner、update_roots；幂等命令类型与函数 ProfileCommandInput、profile_command、request_fingerprint、replay_command、command_view；通用命令 helper business_no、string_update、party_id、required_version、ensure_version、required_text、validate_create_or_update_shape；迁入 command_replay_requires_same_operation_customer_and_fingerprint 测试。
    - 依赖/注意：依赖 facts.rs 暴露的 pub(super) PartyFacts、PartyFactChanges、create_facts 和 prepare_fact_changes；依赖 service.rs 的共享加载方法和字段。PreparedCreate/PreparedUpdate 只应在本模块可见，除事务闭包所需接口外不扩大可见性。必须保留事务内写入顺序、审计写入和 command 写入的原子性。该模块不得反向被 facts.rs 依赖，以避免 mutation 与 facts 的循环职责。
  - **backend/services/src/customer/profile/facts.rs**：放置 Party 联系人、地址和银行账户事实的构造、比较、替换、结束及持久化：create_facts、prepare_fact_changes、new_contact、new_address、new_bank_account、diff_contacts、diff_addresses、diff_bank_accounts、current_contacts、current_addresses、current_bank_accounts；PartyFacts、EntityChanges<T>、PartyFactChanges 及 persist/Default；by_id、take_existing、contact_matches、address_matches、bank_account_matches、update_contact_default、update_address_default、update_bank_default、close_contact、close_address、close_bank、close_remaining_contacts、close_remaining_addresses、close_remaining_banks、close_date、current_fact_filter、normalized_optional；请求事实集合校验 validate_request、validate_contact_input、validate_address_input、validate_bank_input、validate_default_count、validate_unique_existing_ids；迁入默认项数量校验测试。
    - 依赖/注意：PartyFacts、PartyFactChanges 及其 persist 方法需要 pub(super)，供 mutation.rs 的事务载荷使用；EntityChanges<T> 可保持私有。create_facts 和 prepare_fact_changes 是跨模块调用点，应使用 pub(super)，其余 new_*、diff_* 和校验 helper 保持私有。该文件依赖 CustomerProfileService 的 sensitive_data 和 db，因此服务字段需 pub(super)。current_fact_filter 是 Repository 查询参数组装，仍属于 Service 编排，不应移入 entities。
  - **backend/services/src/customer/profile/query.rs**：放置对象中心详情读取和 View 装配：CustomerProfileService::detail、current_party_facts、assignments、assignment_account_names；类型别名 CurrentPartyFacts；结构体 ProfileDetailParts；函数 build_detail、customer_status_blockers、current_revision；迁入 disabled_customer_blocks_new_business_actions 测试。
    - 依赖/注意：detail 依赖 service.rs 中 pub(super) 的 load_customer 和 load_party，并调用 sensitive.rs 中 pub(super) 的 sensitive_fields。应保持 query.rs 到 sensitive.rs 的单向依赖，sensitive.rs 不应引用 ProfileDetailParts 或 build_detail。CustomerProfileDetailView、CustomerView 等 DTO 可从 crate::customer 导入，减少 super::super 层级引用。
  - **backend/services/src/customer/profile/sensitive.rs**：放置敏感字段令牌签发、密文读取、解密和审计流程：CustomerProfileService::reveal_sensitive、sensitive_fields、sensitive_field、sensitive_ciphertext；辅助函数 ensure_party、unix_now、masked_last4。
    - 依赖/注意：sensitive_fields 需要 pub(super)，供 query.rs 的 detail 调用；sensitive_field 和 sensitive_ciphertext 可保持私有。reveal_sensitive 依赖 service.rs 的 load_customer、db 和 sensitive_data，因此相应成员需要 pub(super)。必须保留令牌范围校验、Party 归属检查和成功揭示审计，不要让 query.rs 直接接触密文或解密逻辑。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
