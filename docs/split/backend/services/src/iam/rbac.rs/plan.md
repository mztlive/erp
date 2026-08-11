# `backend/services/src/iam/rbac.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/iam/rbac.rs` |
| 扫描行数 | 1687 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | L |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 L，风险 medium）
- 摘要：建议拆分。当前文件同时承载 Casbin 模型与转换、Enforcer 缓存一致性、policy 事务、角色 CRUD/root/种子维护、账号角色绑定和授权边界，存在多个独立变化原因。按仓库多文件 Service 约定，将 rbac.rs 改为 rbac/mod.rs 模块根并 re-export，再拆为 service.rs、policy.rs、role_management.rs、account_roles.rs；测试随责任就近内联。预计最大文件约 600 至 700 行，其余文件约 50 至 500 行，均可控制在约 800 行以内。实施时保持外部 API、可见性、事务和失败关闭语义不变，仅调整模块路径及必要的 pub(super) 内部接口。
- 拆分建议：
  - **backend/services/src/iam/rbac/mod.rs**：作为 RBAC 子模块根，声明 account_roles、policy、role_management、service 子模块；承载 ROOT_ROLE_ID；re-export shared_rbac_service、RbacService、SharedRbacService、ensure_root_role、subject，以及 crate 内使用的 AuthorizedAccountManagement、AuthorizedRoleGrant，保持 iam/mod.rs 当前公共出口不变。
    - 依赖/注意：迁移完成后必须删除原 backend/services/src/iam/rbac.rs，避免 rbac.rs 与 rbac/mod.rs 模块路径冲突。现有 iam/mod.rs 的 mod rbac 和 pub use rbac::{...} 可保持不变。
  - **backend/services/src/iam/rbac/service.rs**：放置 SharedRbacService、RbacService 及核心基础设施 impl：new、enforcer、fresh_enforcer、policy_cache_is_current、reload_enforcer、refresh_policy、enforce、run_system_policy_transaction、run_authorized_policy_transaction、run_policy_transaction_at_revision、run_authorized_audited_policy_transaction、finish_policy_transaction、ensure_policy_consistency_known；放置 shared_rbac_service、MAX_STABLE_POLICY_LOAD_ATTEMPTS、commit_outcome_unknown、stable_policy_revision、policy_revisions_match、rbac_error及相关内联测试。
    - 依赖/注意：role_management.rs 和 account_roles.rs 需要访问 db、policy_store、loaded_policy_revision，建议仅将这些字段设为 pub(super)。fresh_enforcer 和 run_policy_transaction_at_revision 需要 pub(super)；现有 crate/IAM 可见方法保持原可见性。该文件只依赖 policy.rs 的 RBAC_MODEL，不反向依赖角色或账号子模块。
  - **backend/services/src/iam/rbac/policy.rs**：放置 Casbin 模型和格式转换：RBAC_MODEL、ROLE_PREFIX、subject、role_key、permissions_for_role、implicit_permissions_for_role、collect_role_permissions、permissions_for_actor、permissions_for_account、permissions_for_roles、role_ids_for_account、collect_role_ids、parse_policy_permissions、permission_pairs及相关内联测试。
    - 依赖/注意：除 subject 保持 pub 外，跨 RBAC 子模块使用的 helper 应为 pub(super)，不提升为 pub(crate)。service.rs 使用 RBAC_MODEL；role_management.rs 和 account_roles.rs 使用其余转换与查询 helper。该模块不依赖 RbacService，从而维持单向依赖。Casbin policy 格式属于适配边界，不下沉到 entities。
  - **backend/services/src/iam/rbac/role_management.rs**：放置角色生命周期、列表、权限覆盖、root 和种子维护：AuthorizedPermissions、AuthorizedRoleUpdate；create_role、update_role、delete_role、audit_role_update、update_role_name、role_list、assignable_role_list、replace_role_permissions、assignable_role_items、role_items、authorize_permissions、authorize_role_update、create_role_with_id、seed_role_if_absent、upgrade_seeded_role_permissions_if_exact、direct_role_permissions、update_role_with_permissions、repair_root_role、delete_role_with_policy；ensure_root_role、ensure_root_role_once；ROOT_ROLE_NAME、ROOT_ROLE_INIT_ATTEMPTS；ensure_all_roles_assignable、role_is_assignable、ensure_role_mutable、ensure_role_deletable、role_or_not_found、root_role_is_current及相关测试。
    - 依赖/注意：通过 super::service::RbacService 扩展同一类型的 inherent impl，并调用 service.rs 中 pub(super) 的 fresh_enforcer 和 run_policy_transaction_at_revision。Casbin 转换从 policy.rs 引用。account_roles.rs 需要的 ensure_all_roles_assignable 和 role_is_assignable 设为 pub(super)，其他 helper 保持私有。种子方法继续使用 pub(in crate::iam)，供 predefined_roles.rs 调用。
  - **backend/services/src/iam/rbac/account_roles.rs**：放置账号角色绑定及授权边界：AuthorizedRoleGrant、AuthorizedAccountManagement 及其 impl；authorize_role_assignment、authorize_target_management、load_management_roles、assign_roles、assign_system_roles、clear_roles、role_ids、role_ids_by_accounts、permissions、ensure_roles_assignable；ensure_roles_delegable、authorize_account_permissions、ensure_target_roles_manageable、authorized_role_grant、ensure_permission_subset、ensure_management_subset及相关测试。
    - 依赖/注意：AuthorizedRoleGrant 和 AuthorizedAccountManagement 继续为 pub(crate)，其访问器保持 pub(crate)，兼容 iam/account/admin.rs。assign_system_roles 保持 pub(in crate::iam)，其余账号服务入口保持现有 pub(crate) 范围。从 policy.rs 引用权限和绑定读取 helper，从 role_management.rs 引用 pub(super) 的 ensure_all_roles_assignable 与 role_is_assignable。权限覆盖算法继续委托 entities::PermissionSet::covers，避免复制领域规则。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
