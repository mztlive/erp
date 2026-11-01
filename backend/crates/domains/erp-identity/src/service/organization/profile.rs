//! 人员姓名与组织关系共用同一个提交事务。
use application_core::AuditActor;
use erp_core::AccountKind;
use persistence_core::{Executor, NoTransaction};

use super::OrganizationService;
use super::access::replay;
use crate::entity::account_core::AccountCoreUpdate;
use crate::entity::organization_change::{
    OrganizationChangeReceipt, OrganizationChangeRequest, OrganizationOperation,
};
use crate::repository::OrganizationRepository;
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use crate::{AccessControlExt, Error, Permission, Result};

impl OrganizationService {
    /// 按变更内容重验组织配置权及账号管理边界，幂等回放也不能绕过撤权。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `change` - 待执行的组织命令。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回已证明 `org_unit:manage` 的授权上下文。
    ///
    /// # 错误
    /// 权限解析失败，或账号管理版本与本次授权版本不一致时返回冲突错误。
    pub(super) async fn profile_access(
        &self,
        actor: &AuditActor,
        change: &OrganizationOperation,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let mut required = Vec::new();
        let mut account_revision = None;
        if let OrganizationOperation::UpdatePersonProfile { profile } = change
            && (profile.name.is_some() || profile.role_ids.is_some())
        {
            required.push(Permission::parse("admin:update")?);
            account_revision = Some(
                self.rbac
                    .authorize_target_management(
                        actor,
                        AccountKind::Admin,
                        &profile.user_id,
                        profile.role_ids.clone(),
                    )
                    .await?
                    .policy_revision(),
            );
        }
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "org_unit", "manage", &required, executor)
            .await?;
        if account_revision.is_some_and(|revision| revision != access.policy_version) {
            return Err(Error::ConflictError("授权版本已变化，请刷新后重试".into()));
        }
        Ok(access)
    }

    /// 角色写入使用现有取消安全的 policy 事务，提交后刷新权限缓存。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `request` - 含幂等键和人员资料变更。
    ///
    /// # 返回
    /// 已有回执时返回回放结果，否则返回本次提交的变更回执。
    ///
    /// # 错误
    /// 授权、回放载荷不一致或 policy 事务失败时返回错误。
    pub(super) async fn save_profile_roles(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        let access = self.profile_access(actor, &request.change, &mut NoTransaction).await?;
        let receipt_id = format!("{}:{}", actor.id(), request.idempotency_key);
        if let Some(receipt) =
            OrganizationRepository::new(&self.db).receipt(&receipt_id, &mut NoTransaction).await?
        {
            return replay(receipt, &request, &access);
        }
        let this = self.clone();
        let actor = actor.clone();
        self.rbac
            .run_authorized_policy_transaction(access.policy_version, move |executor| {
                Box::pin(async move { this.execute_change(&actor, request, false, executor).await })
            })
            .await
    }

    /// 在账号资料事务内替换完整角色集，不另行提交。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `change` - 组织命令；不是人员资料更新时直接返回。
    /// * `revision` - 本次组织变更使用的 policy 版本。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 没有角色变更时无返回值；否则角色绑定已写入调用方事务。
    ///
    /// # 错误
    /// 授权版本变化、缺少角色授予上下文或角色写入失败时返回错误。
    pub(super) async fn assign_profile_roles(
        &self,
        actor: &AuditActor,
        change: &OrganizationOperation,
        revision: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let OrganizationOperation::UpdatePersonProfile { profile } = change else {
            return Ok(());
        };
        let Some(roles) = &profile.role_ids else {
            return Ok(());
        };
        let authorization = self
            .rbac
            .authorize_target_management(actor, AccountKind::Admin, &profile.user_id, Some(roles.clone()))
            .await?;
        if authorization.policy_revision() != revision {
            return Err(Error::ConflictError("授权版本已变化，请刷新后重试".into()));
        }
        let grant = authorization
            .into_role_grant()
            .ok_or_else(|| Error::ValidationError("缺少角色授予上下文".into()))?;
        self.rbac.assign_roles(AccountKind::Admin, &profile.user_id, grant, executor).await
    }

    /// 预览校验姓名版本与值，提交时加入组织关系所在事务。
    ///
    /// # 参数
    /// * `change` - 组织命令；不是人员姓名更新时直接返回。
    /// * `persist` - 为真时把姓名写入调用方事务。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 姓名与期望值一致，且在需要时已写入后无返回值。
    ///
    /// # 错误
    /// 人员不存在、姓名版本冲突、领域校验或账号更新失败时返回错误。
    pub(super) async fn update_profile_name(
        &self,
        change: &OrganizationOperation,
        persist: bool,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let OrganizationOperation::UpdatePersonProfile { profile } = change else {
            return Ok(());
        };
        let Some(name) = &profile.name else {
            return Ok(());
        };
        let mut account = self
            .db
            .accounts()
            .find_by_id(&profile.user_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("人员不存在".into()))?;
        if account.name != profile.expected_name {
            return Err(Error::ConflictError("人员姓名已变化，请刷新后重新编辑".into()));
        }
        account.update(AccountCoreUpdate { name: Some(name.clone()), ..Default::default() })?;
        if persist {
            self.db.accounts().update(&mut account, executor).await?;
        }
        Ok(())
    }
}
