//! 人员姓名与组织关系共用同一个提交事务。
use application_core::AuditActor;
use erp_core::AccountKind;
use erp_permissions::organization;
use persistence_core::{Executor, Transactional};

use super::OrganizationService;
use super::access::replay;
use crate::entity::access_control::scope_spec::ORGANIZATION_PERSON_PROFILE_CHANGE;
use crate::entity::account_core::AccountCoreUpdate;
use crate::entity::organization_change::{
    OrganizationChangeReceipt, OrganizationChangeRequest, OrganizationOperation,
};
use crate::repository::OrganizationRepository;
use crate::service::authorization::SnapshotScope;
use crate::{AccessControlExt, Error, Result};

impl OrganizationService {
    /// 按人员资料命令复核组织配置权限和账号管理边界。幂等回放也须重新授权；策略版本不一致时返回冲突。
    pub(super) async fn profile_access(
        &self,
        actor: &AuditActor,
        change: &OrganizationOperation,
        executor: &mut dyn Executor,
    ) -> Result<SnapshotScope> {
        let profile = match change {
            OrganizationOperation::UpdatePersonProfile { profile }
                if profile.name.is_some() || profile.role_ids.is_some() =>
            {
                Some(profile)
            },
            _ => None,
        };
        let permission =
            if profile.is_some() { &ORGANIZATION_PERSON_PROFILE_CHANGE } else { &organization::CHANGE };
        let access = self.authorization.organization_scope(actor, permission, executor).await?;

        let mut account_revision = None;
        if let Some(profile) = profile {
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

        if account_revision.is_some_and(|revision| revision != access.policy_version) {
            return Err(Error::ConflictError("授权版本已变化，请刷新后重试".into()));
        }
        access.ensure_current(executor)?;
        Ok(access)
    }

    /// 通过策略写事务处理含角色调整的人员资料提交，原子记录变更或重放回执；提交后通过策略事务更新权限缓存。
    pub(super) async fn save_profile_roles(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        let this = self.clone();
        let precheck_actor = actor.clone();
        let precheck_request = request.clone();
        let (revision, receipt) = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.profile_roles_precheck(&precheck_actor, &precheck_request, executor).await
                })
            })
            .await?;
        if let Some(receipt) = receipt {
            return Ok(receipt);
        }

        let this = self.clone();
        let actor = actor.clone();
        self.rbac
            .run_authorized_policy_transaction(revision, move |executor| {
                Box::pin(async move { this.execute_change(&actor, request, false, executor).await })
            })
            .await
    }

    // 正式角色写入前先取得策略修订号，用于约束后续事务；正式事务会重新解析人员范围。
    async fn profile_roles_precheck(
        &self,
        actor: &AuditActor,
        request: &OrganizationChangeRequest,
        executor: &mut dyn Executor,
    ) -> Result<(u64, Option<OrganizationChangeReceipt>)> {
        let access = self.profile_access(actor, &request.change, executor).await?;
        let receipt_id = format!("{}:{}", actor.id(), request.idempotency_key);
        let receipt = OrganizationRepository::new(&self.db).receipt(&receipt_id, executor).await?;
        let receipt = receipt.map(|receipt| replay(receipt, request, &access)).transpose()?;
        access.ensure_current(executor)?;
        Ok((access.policy_version, receipt))
    }

    /// 在人员资料事务中按本次策略版本替换全部角色绑定。只有角色变更命令会写入；授权上下文缺失或版本变化时返回错误。
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

    /// 在预览阶段校验姓名和值版本，在正式提交时用同一执行器更新姓名。人员缺失、版本冲突或领域校验失败时返回错误。
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
