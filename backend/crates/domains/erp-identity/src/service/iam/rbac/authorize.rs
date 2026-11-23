use std::collections::HashMap;
use std::sync::atomic::Ordering;

use application_core::AuditActor;
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
use crate::entity::rbac::{Permission, PermissionSet, RoleId, RoleIdSet};
use crate::entity::role::Role;
use crate::error::{Error, Result};
use crate::repository::policy_store::PolicySnapshot;
use crate::repository::prelude::*;

// 管理授权检查使用的同一策略快照，冻结操作人的授权上限与策略版本。
struct ActorPermissionSnapshot<'a> {
    // 持有策略快照读锁，供调用方读取同版的目标策略；后续网络 I/O 前须释放。
    snapshot: tokio::sync::RwLockReadGuard<'a, PolicySnapshot>,
    // 操作人在该快照中的隐式权限集，作为子集校验的上限。
    actor_permissions: PermissionSet,
    // 快照对应的策略版本，供后续写事务重验。
    policy_revision: u64,
}

impl RbacService {
    // 四个管理授权入口共用此方法，在同一读锁下取得操作人的隐式权限与策略版本。
    // 调用方在锁内完成各自的子集校验，后续网络 I/O 前须释放读锁；快照加载或权限解析失败时返回错误。
    async fn actor_permission_snapshot(&self, actor: &AuditActor) -> Result<ActorPermissionSnapshot<'_>> {
        let snapshot = self.fresh_snapshot().await?.read().await;
        let actor_permissions = permissions_for_actor(&snapshot, actor)?;
        let policy_revision = self.loaded_policy_revision.load(Ordering::Acquire);
        Ok(ActorPermissionSnapshot { snapshot, actor_permissions, policy_revision })
    }

    /// 在普通管理入口授予角色前，确认 `role_ids` 全部存在且启用，拒绝内建 `role-root` 和系统角色。
    /// 目标角色的隐式权限须由操作人的当前隐式权限覆盖；校验成功后冻结角色集合和策略版本供写事务使用。
    /// 角色无效、权限越界或策略加载失败时返回错误。
    pub async fn authorize_role_assignment(
        &self,
        actor: &AuditActor,
        role_ids: RoleIdSet,
    ) -> Result<AuthorizedRoleGrant> {
        let roles = self.db.roles().enabled_roles(&role_ids, &mut NoTransaction).await?;
        ensure_all_roles_assignable(role_ids.len(), roles.len())?;
        ensure_roles_delegable(&roles)?;

        let snapshot = self.actor_permission_snapshot(actor).await?;
        ensure_permission_subset(
            &snapshot.actor_permissions,
            &permissions_for_roles(&snapshot.snapshot, &role_ids)?,
        )?;
        Ok(AuthorizedRoleGrant { role_ids, policy_revision: snapshot.policy_revision })
    }

    /// 在普通入口管理目标账号前，用同一策略快照校验目标现有权限和拟授予角色权限均在操作人权限内。
    /// 目标若绑定内建 `role-root`、系统角色或无法确认的角色即拒绝；拟授予角色须存在、启用且允许普通分配。
    /// 返回写事务所需的策略版本；`requested_role_ids` 为 `None` 时保持绑定，有值时返回经校验的完整替换上下文。
    /// 任一校验或策略加载失败时返回错误。
    pub async fn authorize_target_management(
        &self,
        actor: &AuditActor,
        target_kind: AccountKind,
        target_id: &str,
        requested_role_ids: Option<RoleIdSet>,
    ) -> Result<AuthorizedAccountManagement> {
        let snapshot = self.actor_permission_snapshot(actor).await?;
        let target_role_ids = role_ids_for_account(&snapshot.snapshot, target_kind, target_id)?;
        authorize_account_permissions(
            &snapshot.snapshot,
            &snapshot.actor_permissions,
            target_kind,
            target_id,
            requested_role_ids.as_ref(),
        )?;
        let policy_revision = snapshot.policy_revision;
        drop(snapshot.snapshot);

        let roles = self.load_management_roles(&target_role_ids, requested_role_ids.as_ref()).await?;
        ensure_target_roles_manageable(&target_role_ids, &roles)?;
        if let Some(role_ids) = &requested_role_ids {
            ensure_requested_roles_assignable(role_ids, &roles)?;
        }
        Ok(AuthorizedAccountManagement { policy_revision, replacement_roles: requested_role_ids })
    }

    /// 创建角色或补齐种子角色前，规范化拟授予权限并校验其在操作人的当前隐式权限内。
    /// 返回经校验的权限集和供写事务重验的策略版本；越权或策略加载失败时返回错误。
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

    // 检查账号管理资格时，将现有和拟分配角色去重后批量加载，供校验角色保护和可分配状态。
    async fn load_management_roles(
        &self,
        target_role_ids: &[RoleId],
        requested_role_ids: Option<&RoleIdSet>,
    ) -> Result<HashMap<String, Role>> {
        let mut role_ids = target_role_ids.to_vec();
        if let Some(requested_role_ids) = requested_role_ids {
            role_ids.extend(requested_role_ids.iter().cloned());
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

    /// 编辑或删除角色前，用同一 Casbin 快照校验现有隐式权限及可选的新权限均在操作人权限内。
    /// `updated_permissions` 为 `None` 时保留现有权限；返回可选的新权限集和待写事务复核的策略版本。
    /// 管理或授予越权、策略加载失败时返回错误。
    pub(super) async fn authorize_role_update(
        &self,
        actor: &AuditActor,
        role_id: &str,
        updated_permissions: Option<Vec<Permission>>,
    ) -> Result<AuthorizedRoleUpdate> {
        let permissions = updated_permissions.map(PermissionSet::new);
        let snapshot = self.actor_permission_snapshot(actor).await?;
        let current_permissions = implicit_permissions_for_role(&snapshot.snapshot, role_id)?;
        ensure_management_subset(&snapshot.actor_permissions, &current_permissions)?;
        if let Some(permissions) = permissions.as_ref() {
            ensure_permission_subset(&snapshot.actor_permissions, permissions)?;
        }
        Ok(AuthorizedRoleUpdate { permissions, policy_revision: snapshot.policy_revision })
    }

    /// 普通管理入口使用已按操作人权限校验的 `grant`，在写事务内复核角色并覆盖账号实际生效的 Casbin 绑定。
    /// 调用方须用授予上下文的策略版本经 [`Self::run_authorized_policy_transaction`] 开启取消安全的事务，
    /// 并传入其执行器，使角色校验与写入原子生效，由事务边界在提交后刷新本地 Enforcer。
    /// 角色无效或写入失败时返回错误。
    pub async fn assign_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        grant: AuthorizedRoleGrant,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.write_subject_roles(account_kind, account_id, grant.role_ids, executor).await
    }

    /// 仅在初始化或修复内建超级管理员时使用，沿用系统初始化事务复核并写入完整角色集合。
    /// 普通管理入口必须通过 [`Self::assign_roles`] 使用已校验的授予上下文。
    /// 角色无效或策略写入失败时返回错误。
    pub async fn assign_system_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        role_ids: RoleIdSet,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.write_subject_roles(account_kind, account_id, role_ids, executor).await
    }

    /// 普通管理、授权文件应用和系统初始化共用此写入：复核启用状态并更新角色版本，
    /// 用已规范化的完整集合覆盖 Casbin 绑定，同事务初始化人员目录资格。任一步失败均返回错误。
    /// 调用方须传入原策略事务执行器；普通管理使用 [`Self::run_authorized_policy_transaction`]，
    /// 各调用方的取消安全事务边界负责版本推进和提交后刷新，本方法仅写数据。
    pub(super) async fn write_subject_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        role_ids: RoleIdSet,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.ensure_roles_assignable(&role_ids, executor).await?;
        let role_keys = role_ids.iter().map(|role_id| role_key(role_id.as_str())).collect::<Vec<_>>();
        self.policy_store
            .replace_subject_roles(&subject(account_kind, account_id), &role_keys, executor)
            .await?;

        crate::service::person_directory::grant_assigned_roles(&self.db, account_id, &role_ids, executor)
            .await?;
        Ok(())
    }

    /// 删除账号等操作中，使用调用方的事务执行器清除该账号全部 Casbin 绑定；删除失败时返回错误。
    /// 调用方须经 [`Self::run_authorized_policy_transaction`] 建立取消安全的事务，由该边界在提交后刷新本地 Enforcer。
    pub async fn clear_roles(
        &self,
        account_kind: AccountKind,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.policy_store.clear_subject_roles(&subject(account_kind, account_id), executor).await?;
        Ok(())
    }

    // 替换绑定前，在同一事务内确认全部角色存在且启用，并用版本条件更新每个角色。
    // 角色缺失、停用或写冲突时返回错误；版本更新使并发编辑或删除与绑定产生写冲突，避免绑定已删除角色。
    async fn ensure_roles_assignable(&self, role_ids: &RoleIdSet, executor: &mut dyn Executor) -> Result<()> {
        let mut roles = self.db.roles().enabled_roles(role_ids, executor).await?;
        ensure_all_roles_assignable(role_ids.len(), roles.len())?;
        for role in &mut roles {
            self.db.roles().update(role, executor).await?;
        }
        Ok(())
    }
}

/// 授权预检或绑定写入时，核对实际找到的有效角色数与请求数；数量不符按角色缺失或停用报业务错误。
pub(super) fn ensure_all_roles_assignable(requested_count: usize, existing_count: usize) -> Result<()> {
    if existing_count != requested_count {
        return Err(Error::BusinessLogicError("角色不存在或已停用".to_string()));
    }
    Ok(())
}

/// 普通管理入口授予角色前检查其可分配性；包含内建 `role-root`、系统角色或停用角色时返回 `Forbidden`。
pub(super) fn ensure_roles_delegable(roles: &[Role]) -> Result<()> {
    roles.iter().try_for_each(|role| {
        role.ensure_assignable()
            .map_err(|_| Error::Forbidden("系统角色或已停用角色不能通过普通接口分配".into()))
    })
}

// 管理目标账号前，在同一策略快照内核对目标现有隐式权限及拟授予角色权限均由操作人覆盖。
fn authorize_account_permissions(
    snapshot: &PolicySnapshot,
    actor_permissions: &PermissionSet,
    target_kind: AccountKind,
    target_id: &str,
    requested_role_ids: Option<&RoleIdSet>,
) -> Result<()> {
    let target_permissions = permissions_for_account(snapshot, target_kind, target_id)?;
    ensure_management_subset(actor_permissions, &target_permissions)?;
    if let Some(role_ids) = requested_role_ids {
        let requested_permissions = permissions_for_roles(snapshot, role_ids)?;
        ensure_permission_subset(actor_permissions, &requested_permissions)?;
    }
    Ok(())
}

/// 普通管理入口按目标账号的直接绑定检查保护状态；绑定内建 `role-root`、系统角色或缺少角色实体时返回 `Forbidden`。
pub(super) fn ensure_target_roles_manageable(
    target_role_ids: &[RoleId],
    roles: &HashMap<String, Role>,
) -> Result<()> {
    let protected = target_role_ids.iter().any(|role_id| {
        role_id.as_str() == ROOT_ROLE_ID || roles.get(role_id.as_str()).is_none_or(|role| role.system)
    });
    if protected {
        return Err(Error::Forbidden("绑定系统角色的账号不能通过普通管理接口修改".to_string()));
    }
    Ok(())
}

// 先确认替换集合中的角色均存在，再检查是否允许通过普通入口分配。
fn ensure_requested_roles_assignable(role_ids: &RoleIdSet, roles: &HashMap<String, Role>) -> Result<()> {
    let requested_roles =
        role_ids.iter().filter_map(|role_id| roles.get(role_id.as_str())).collect::<Vec<_>>();
    ensure_all_roles_assignable(role_ids.len(), requested_roles.len())?;
    requested_roles.into_iter().try_for_each(|role| {
        role.ensure_assignable()
            .map_err(|_| Error::Forbidden("系统角色或已停用角色不能通过普通接口分配".into()))
    })
}

/// 授予角色或权限前，要求操作人权限覆盖待授予集合；越出授予上限时返回 `Forbidden`。
pub(super) fn ensure_permission_subset(actor: &PermissionSet, required: &PermissionSet) -> Result<()> {
    ensure_covers(actor, required, "不能授予超出自身权限范围的角色或权限")
}

/// 管理已有账号或角色前，要求操作人权限覆盖目标的当前权限；目标越界时返回 `Forbidden`。
pub(super) fn ensure_management_subset(actor: &PermissionSet, target: &PermissionSet) -> Result<()> {
    ensure_covers(actor, target, "不能管理权限范围高于自身的账号或角色")
}

// 供管理和授予两类子集检查共用权限覆盖规则，越界时用对应场景的文案拒绝。
fn ensure_covers(actor: &PermissionSet, required: &PermissionSet, message: &str) -> Result<()> {
    if !actor.covers(required) {
        return Err(Error::Forbidden(message.to_string()));
    }
    Ok(())
}
