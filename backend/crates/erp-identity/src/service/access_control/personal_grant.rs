//! 个人部门扩展授权：动作资格、范围配置权限、版本与审计在同一事务内。
use application_core::AuditActor;
use id_generator::next_id;
use persistence_core::{Executor, Transactional};

use super::consumers::validate_binding;
use super::resolve::DataScopeService;
use super::{AccessControlService, ensure_scope_configuration};
use crate::dto::personal_grant::{
    CreatePersonalGrantRequest, PersonalGrantListView, RevokePersonalGrantRequest,
};
use crate::entity::access_control::personal_grant::{PersonalBusinessGrant, PersonalBusinessGrantId};
use crate::{AccessControlExt, Error, Permission, Result, SharedRbacService};

impl AccessControlService {
    /// 查询指定人员的独立部门授权及保存版本。
    /// # 参数
    /// * `user_id` - 固定人员身份。
    /// * `actor` - 发起查看的管理员。
    /// # 返回
    /// 未撤销授权和当前策略版本。
    /// # 错误
    /// 缺少公司配置读取资格或数据读取失败时拒绝。
    pub async fn personal_grants(&self, user_id: &str, actor: &AuditActor) -> Result<PersonalGrantListView> {
        let service = self.grant_service();
        let user_id = user_id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let version = service.authorize_grant_read(&actor, executor).await?;
                    service.personal_grant_view(&user_id, version, executor).await
                })
            })
            .await
    }

    /// 新增明确业务动作的个人部门范围；不得增加动作权限。
    /// # 参数
    /// * `user_id` - 接受授权的人员。
    /// * `req` - 已选择的角色、业务、动作、部门及版本。
    /// * `actor` - 当前管理员。
    /// # 返回
    /// 已保存的独立授权。
    /// # 错误
    /// 越权、角色失效、版本变化、部门失效或写入失败时拒绝整次命令。
    pub async fn create_personal_grant(
        &self,
        user_id: &str,
        req: CreatePersonalGrantRequest,
        actor: &AuditActor,
    ) -> Result<PersonalBusinessGrant> {
        let grant = PersonalBusinessGrant::new(PersonalBusinessGrantId::new(next_id()), user_id, req.grant)?;
        let event = self
            .build_audit_event(
                actor,
                "personal_business_grant.create",
                "personal_business_grant",
                Some(grant.base.id.clone()),
                vec![
                    "user_id".into(),
                    "role_id".into(),
                    "resource".into(),
                    "actions".into(),
                    "org_unit_ids".into(),
                ],
            )
            .await?;
        let service = self.grant_service();
        let actor = actor.clone();
        self.grant_rbac()?
            .run_authorized_policy_transaction(req.expected_policy_version, move |executor| {
                Box::pin(async move {
                    service.authorize_grant(&actor, &grant, "create", executor).await?;
                    service.db.personal_business_grants().create(&grant, executor).await?;
                    service.db.audit_events().create(&event, executor).await?;
                    Ok(grant)
                })
            })
            .await
    }

    /// 撤销个人附加范围，保留角色原有范围与审计。
    /// # 参数
    /// * `user_id`、`id` - 人员与授权身份。
    /// * `req` - 授权和策略的期望版本。
    /// * `actor` - 当前管理员。
    /// # 返回
    /// 原子撤销成功。
    /// # 错误
    /// 越权、跨人员目标、重复撤销、版本冲突或写入失败时拒绝。
    pub async fn revoke_personal_grant(
        &self,
        user_id: &str,
        id: &str,
        req: RevokePersonalGrantRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let event = self
            .build_audit_event(
                actor,
                "personal_business_grant.revoke",
                "personal_business_grant",
                Some(id.into()),
                vec!["deleted_at".into()],
            )
            .await?;
        let service = self.grant_service();
        let (user_id, id, actor) = (user_id.to_owned(), id.to_owned(), actor.clone());
        self.grant_rbac()?
            .run_authorized_policy_transaction(req.expected_policy_version, move |executor| {
                Box::pin(async move {
                    let mut grant = service
                        .db
                        .personal_business_grants()
                        .find_by_id(&id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("个人业务授权不存在或已撤销".into()))?;
                    grant.ensure_revoke(&user_id, req.version)?;
                    service.authorize_grant(&actor, &grant, "delete", executor).await?;
                    service.db.personal_business_grants().soft_delete(&mut grant, executor).await?;
                    service.db.audit_events().create(&event, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    /// 只复制服务依赖用于拥有事务闭包，不复制业务状态。
    fn grant_service(&self) -> Self {
        Self { db: self.db.clone(), rbac: self.rbac.clone(), targets: self.targets.clone() }
    }

    /// 未装配 RBAC 时失败关闭。
    pub(super) fn grant_rbac(&self) -> Result<SharedRbacService> {
        self.rbac.clone().ok_or_else(|| Error::Forbidden("未装配个人业务授权".into()))
    }

    /// 公司级配置读取资格必须由同一个角色证明。
    async fn authorize_grant_read(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<u64> {
        let required = ["admin:list", "role:list", "data_scope:list"]
            .into_iter()
            .map(Permission::parse)
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let access = DataScopeService::new(self.db.clone(), self.grant_rbac()?)
            .resolve_permissions(actor, "org_unit", "list", &required, executor)
            .await?;
        if !access.scope.role_clauses.iter().any(|clause| clause.company)
            || access.scope.user_limit.as_ref().is_some_and(|limit| !limit.company)
        {
            return Err(Error::Forbidden("查看个人业务授权需要公司范围的组织及人员权限读取资格".into()));
        }
        Ok(access.policy_version)
    }

    /// 配置资格和目标资格均在写入执行器内重验。
    async fn authorize_grant(
        &self,
        actor: &AuditActor,
        grant: &PersonalBusinessGrant,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let rbac = self.grant_rbac()?;
        let scope = grant.as_role_scope()?;
        validate_binding(&scope.binding)?;
        ensure_scope_configuration(&self.db, rbac.clone(), actor, &scope, action, executor).await?;
        if action == "delete" {
            return Ok(());
        }
        let account = self
            .db
            .accounts()
            .find_by_id(&grant.user_id, executor)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::ValidationError("授权人员不存在或未启用".into()))?;
        let required = grant
            .actions
            .iter()
            .map(|item| Permission::parse(format!("{}:{item}", grant.resource)))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let snapshot = rbac.role_permission_snapshot(account.kind, &grant.user_id, &required).await?;
        rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        if !snapshot.granting_role_ids_for_all(&required).contains(&grant.role_id) {
            return Err(Error::ValidationError(
                "该人员未持有提供全部所选操作的有效角色，请刷新后重新选择".into(),
            ));
        }
        Ok(())
    }
}
