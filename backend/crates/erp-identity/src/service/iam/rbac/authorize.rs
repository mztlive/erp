use std::collections::HashMap;
use std::sync::atomic::Ordering;

use application_core::AuditActor;
use casbin::Enforcer;
use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};

use super::policy::{
    implicit_permissions_for_role, permissions_for_account, permissions_for_actor, permissions_for_roles,
    role_ids_for_account, role_key,
};
use super::{
    AuthorizedAccountManagement, AuthorizedPermissions, AuthorizedRoleGrant, AuthorizedRoleUpdate,
    ROOT_ROLE_ID, RbacService, subject,
};
use crate::AccessControlExt;
use crate::entity::rbac::{Permission, PermissionSet, RoleIdSet};
use crate::entity::role::Role;
use crate::error::{Error, Result};
use crate::repository::prelude::*;

/// 同一 Enforcer 快照下的操作人权限集与 policy 版本。
struct ActorPermissionSnapshot<'a> {
    /// 持有读锁的 Enforcer 快照守卫；调用方用完后 `drop` 再做网络 I/O。
    enforcer: tokio::sync::RwLockReadGuard<'a, Enforcer>,
    /// 快照内解析出的操作人权限集。
    actor_permissions: PermissionSet,
    /// 快照时刻的 policy 版本。
    policy_revision: u64,
}

impl RbacService {
    /// 在同一 Enforcer 快照下取操作人权限集并捕获 policy 版本。
    ///
    /// 四个 `authorize_*` 入口共享该快照序列：快照内读操作人权限，调用方再按
    /// 各自业务规则做子集校验；拒绝/子集语义不变。
    ///
    /// # 参数
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回操作人权限集、`policy_revision` 与持有读锁的 Enforcer 快照守卫；
    /// 调用方用完后 `drop` 守卫再做网络 I/O。
    ///
    /// # 错误
    /// Enforcer 快照加载或操作人权限解析失败时返回错误。
    async fn actor_permission_snapshot(&self, actor: &AuditActor) -> Result<ActorPermissionSnapshot<'_>> {
        let enforcer = self.fresh_enforcer().await?.read().await;
        let actor_permissions = permissions_for_actor(&enforcer, actor)?;
        let policy_revision = self.loaded_policy_revision.load(Ordering::Acquire);
        Ok(ActorPermissionSnapshot { enforcer, actor_permissions, policy_revision })
    }

    /// 校验操作人是否可以授予目标角色，并捕获当前 policy 版本。
    ///
    /// 系统角色不允许通过普通管理接口分配；目标角色权限必须是操作人当前隐式权限的子集。
    ///
    /// # 错误
    /// 当角色不存在、不可分配、权限越界或 policy 加载失败时返回错误。
    pub async fn authorize_role_assignment(
        &self,
        actor: &AuditActor,
        role_ids: Vec<String>,
    ) -> Result<AuthorizedRoleGrant> {
        let role_ids = RoleIdSet::parse(role_ids)?.to_strings();
        let roles = self.db.roles().enabled_roles(&role_ids, &mut NoTransaction).await?;
        ensure_all_roles_assignable(role_ids.len(), roles.len())?;
        ensure_roles_delegable(&roles)?;

        let snapshot = self.actor_permission_snapshot(actor).await?;
        ensure_permission_subset(
            &snapshot.actor_permissions,
            &permissions_for_roles(&snapshot.enforcer, &role_ids)?,
        )?;
        Ok(AuthorizedRoleGrant { role_ids, policy_revision: snapshot.policy_revision })
    }

    /// 校验操作人可管理目标账号，并按需校验新的角色集合。
    ///
    /// 目标账号当前权限与待分配角色权限都必须是操作人权限的子集；绑定任一系统
    /// 角色的账号禁止通过普通管理接口修改。整个判断共享同一 Enforcer 快照。
    ///
    /// # 错误
    /// 当目标含系统角色、权限越界、待分配角色无效或 policy 加载失败时返回错误。
    pub async fn authorize_target_management(
        &self,
        actor: &AuditActor,
        target_kind: AccountKind,
        target_id: &str,
        requested_role_ids: Option<Vec<String>>,
    ) -> Result<AuthorizedAccountManagement> {
        let requested_role_ids =
            requested_role_ids.map(RoleIdSet::parse).transpose()?.map(|role_ids| role_ids.to_strings());
        let snapshot = self.actor_permission_snapshot(actor).await?;
        let target_role_ids = role_ids_for_account(&snapshot.enforcer, target_kind, target_id);
        authorize_account_permissions(
            &snapshot.enforcer,
            &snapshot.actor_permissions,
            target_kind,
            target_id,
            requested_role_ids.as_deref(),
        )?;
        let policy_revision = snapshot.policy_revision;
        drop(snapshot.enforcer);

        let roles = self.load_management_roles(&target_role_ids, requested_role_ids.as_deref()).await?;
        ensure_target_roles_manageable(&target_role_ids, &roles)?;
        let role_grant = requested_role_ids
            .map(|role_ids| authorized_role_grant(role_ids, policy_revision, &roles))
            .transpose()?;
        Ok(AuthorizedAccountManagement { policy_revision, role_grant })
    }

    /// 校验待写入角色权限不超过操作人当前权限。
    pub(super) async fn authorize_permissions(
        &self,
        actor: &AuditActor,
        permissions: Vec<Permission>,
    ) -> Result<AuthorizedPermissions> {
        let permissions = PermissionSet::new(permissions);
        let snapshot = self.actor_permission_snapshot(actor).await?;
        ensure_permission_subset(&snapshot.actor_permissions, &permissions)?;
        Ok(AuthorizedPermissions { permissions, policy_revision: snapshot.policy_revision })
    }

    /// 批量加载目标账号当前角色和可选待分配角色。
    async fn load_management_roles(
        &self,
        target_role_ids: &[String],
        requested_role_ids: Option<&[String]>,
    ) -> Result<HashMap<String, Role>> {
        let mut role_ids = target_role_ids.to_vec();
        if let Some(requested_role_ids) = requested_role_ids {
            role_ids.extend_from_slice(requested_role_ids);
        }
        role_ids.sort();
        role_ids.dedup();
        Ok(self
            .db
            .roles()
            .roles_by_ids(&role_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|role| (role.base.id.clone(), role))
            .collect())
    }

    /// 校验操作人可管理角色的当前权限范围，并按需校验更新后的权限。
    pub(super) async fn authorize_role_update(
        &self,
        actor: &AuditActor,
        role_id: &str,
        updated_permissions: Option<Vec<Permission>>,
    ) -> Result<AuthorizedRoleUpdate> {
        let permissions = updated_permissions.map(PermissionSet::new);
        let snapshot = self.actor_permission_snapshot(actor).await?;
        let current_permissions = implicit_permissions_for_role(&snapshot.enforcer, role_id)?;
        ensure_management_subset(&snapshot.actor_permissions, &current_permissions)?;
        if let Some(permissions) = permissions.as_ref() {
            ensure_permission_subset(&snapshot.actor_permissions, permissions)?;
        }
        Ok(AuthorizedRoleUpdate { permissions, policy_revision: snapshot.policy_revision })
    }

    /// 覆盖账号角色绑定。
    ///
    /// 该方法只写数据，不更新本地 Enforcer；角色校验与绑定替换必须原子生效，
    /// 调用方必须传入事务执行器，并通过
    /// [`Self::run_authorized_policy_transaction`] 建立取消安全的事务与刷新边界。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `account_id` - 账号 ID
    /// * `grant` - 已按操作人权限和 policy 版本校验的授予上下文
    /// * `executor` - 数据访问执行器，必须为事务执行器
    ///
    /// # 返回值
    /// 事务内角色校验和绑定替换成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当角色无效或 MongoDB policy 写入失败时返回错误。
    pub async fn assign_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        grant: AuthorizedRoleGrant,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.write_subject_roles(account_kind, account_id, grant.role_ids, executor).await
    }

    /// 执行系统初始化角色绑定。
    ///
    /// 仅超级管理员初始化可调用；普通管理入口必须使用
    /// [`Self::assign_roles`] 的授权上下文。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `account_id` - 账号 ID
    /// * `role_ids` - 完整角色 ID 集合
    /// * `executor` - 数据访问执行器，必须为事务执行器
    ///
    /// # 返回值
    /// 角色校验和绑定替换成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当角色无效或 MongoDB policy 写入失败时返回错误。
    pub async fn assign_system_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        role_ids: Vec<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let role_ids = RoleIdSet::parse(role_ids)?.to_strings();
        self.write_subject_roles(account_kind, account_id, role_ids, executor).await
    }

    /// 校验角色有效并覆盖账号角色绑定（授权与系统初始化入口共用该写入）。
    ///
    /// 该方法只写数据，不更新本地 Enforcer；调用方必须传入事务执行器，并通过
    /// [`Self::run_authorized_policy_transaction`] 建立取消安全的事务与刷新边界。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `account_id` - 账号 ID
    /// * `role_ids` - 已规范化的完整角色 ID 集合
    /// * `executor` - 数据访问执行器，必须为事务执行器
    ///
    /// # 返回值
    /// 事务内角色校验和绑定替换成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当角色无效或 MongoDB policy 写入失败时返回错误。
    async fn write_subject_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        role_ids: Vec<String>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_roles_assignable(&role_ids, executor).await?;
        let role_keys = role_ids.iter().map(|role_id| role_key(role_id)).collect::<Vec<_>>();
        self.policy_store
            .replace_subject_roles(&subject(account_kind, account_id), &role_keys, executor)
            .await?;
        Ok(())
    }

    /// 清除账号全部角色绑定。
    ///
    /// 该方法只写数据，不更新本地 Enforcer。调用方必须传入事务执行器，并通过
    /// [`Self::run_authorized_policy_transaction`] 建立取消安全的事务与刷新边界。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `account_id` - 账号 ID
    /// * `executor` - 数据访问执行器，必须为事务执行器
    ///
    /// # 返回值
    /// 事务内角色绑定清除成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当 MongoDB policy 删除失败时返回错误。
    pub async fn clear_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.policy_store.clear_subject_roles(&subject(account_kind, account_id), executor).await?;
        Ok(())
    }

    /// 校验并 CAS touch 全部待分配角色。
    ///
    /// 写触碰会与并发角色更新或删除产生写冲突，避免纯快照读取允许已删除角色被绑定；
    /// 因此调用方必须传入事务执行器。
    ///
    /// # 参数
    /// * `role_ids` - 待分配角色 ID
    /// * `executor` - 数据访问执行器，必须为事务执行器
    ///
    /// # 返回值
    /// 全部角色存在、启用且写触碰成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当角色缺失、被停用或 MongoDB 写入冲突时返回错误。
    async fn ensure_roles_assignable(&self, role_ids: &[String], executor: &mut dyn Executor) -> Result<()> {
        let mut roles = self.db.roles().enabled_roles(role_ids, executor).await?;
        ensure_all_roles_assignable(role_ids.len(), roles.len())?;
        for role in &mut roles {
            self.db.roles().update(role, executor).await?;
        }
        Ok(())
    }
}

/// 校验事务内解析出的可分配角色数量与请求一致。
pub(super) fn ensure_all_roles_assignable(requested_count: usize, existing_count: usize) -> Result<()> {
    if existing_count != requested_count {
        return Err(Error::BusinessLogicError("角色不存在或已停用".to_string()));
    }
    Ok(())
}

pub(super) fn ensure_roles_delegable(roles: &[Role]) -> Result<()> {
    roles.iter().try_for_each(|role| {
        if !role_is_assignable(role) {
            return Err(Error::Forbidden("系统角色或已停用角色不能通过普通接口分配".to_string()));
        }
        Ok(())
    })
}

pub(super) fn role_is_assignable(role: &Role) -> bool {
    role.base.id != ROOT_ROLE_ID && role.ensure_assignable().is_ok()
}

pub(super) fn ensure_role_mutable(role: &Role) -> Result<()> {
    if role.base.id == ROOT_ROLE_ID || role.ensure_mutable().is_err() {
        return Err(Error::Forbidden("系统角色不能修改".to_string()));
    }
    Ok(())
}

pub(super) fn ensure_role_deletable(role: &Role) -> Result<()> {
    if role.base.id == ROOT_ROLE_ID || role.ensure_deletable().is_err() {
        return Err(Error::Forbidden("系统角色不能删除".to_string()));
    }
    Ok(())
}

fn authorize_account_permissions(
    enforcer: &Enforcer,
    actor_permissions: &PermissionSet,
    target_kind: AccountKind,
    target_id: &str,
    requested_role_ids: Option<&[String]>,
) -> Result<()> {
    let target_permissions = permissions_for_account(enforcer, target_kind, target_id)?;
    ensure_management_subset(actor_permissions, &target_permissions)?;
    if let Some(role_ids) = requested_role_ids {
        let requested_permissions = permissions_for_roles(enforcer, role_ids)?;
        ensure_permission_subset(actor_permissions, &requested_permissions)?;
    }
    Ok(())
}

pub(super) fn ensure_target_roles_manageable(
    target_role_ids: &[String],
    roles: &HashMap<String, Role>,
) -> Result<()> {
    let protected = target_role_ids
        .iter()
        .any(|role_id| role_id == ROOT_ROLE_ID || roles.get(role_id).is_none_or(|role| role.system));
    if protected {
        return Err(Error::Forbidden("绑定系统角色的账号不能通过普通管理接口修改".to_string()));
    }
    Ok(())
}

fn authorized_role_grant(
    role_ids: Vec<String>,
    policy_revision: u64,
    roles: &HashMap<String, Role>,
) -> Result<AuthorizedRoleGrant> {
    let requested_roles =
        role_ids.iter().filter_map(|role_id| roles.get(role_id)).cloned().collect::<Vec<_>>();
    ensure_all_roles_assignable(role_ids.len(), requested_roles.len())?;
    ensure_roles_delegable(&requested_roles)?;
    Ok(AuthorizedRoleGrant { role_ids, policy_revision })
}

pub(super) fn ensure_permission_subset(actor: &PermissionSet, required: &PermissionSet) -> Result<()> {
    ensure_covers(actor, required, "不能授予超出自身权限范围的角色或权限")
}

pub(super) fn ensure_management_subset(actor: &PermissionSet, target: &PermissionSet) -> Result<()> {
    ensure_covers(actor, target, "不能管理权限范围高于自身的账号或角色")
}

/// 校验操作人权限覆盖目标集合；两个授权子集检查共用该覆盖语义。
fn ensure_covers(actor: &PermissionSet, required: &PermissionSet, message: &str) -> Result<()> {
    if !actor.covers(required) {
        return Err(Error::Forbidden(message.to_string()));
    }
    Ok(())
}
