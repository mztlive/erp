//! 内部组织查询、影响预览及事务变更。

mod access;
mod bootstrap;
pub mod leader;
mod profile;

use std::sync::Arc;

use application_core::AuditActor;
pub(crate) use bootstrap::ensure_home_department;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_permissions::organization;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use self::access::{prepare_organization_change, visible_state};
use crate::dto::{OrgPersonView, OrgRoleView, OrganizationStateView};
use crate::entity::organization_change::*;
use crate::ports::OrganizationBusinessPort;
use crate::repository::OrganizationRepository;
use crate::repository::prelude::*;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::service::authorization::SnapshotScope;
use crate::{AccessControlExt, AuthorizationService, Error, Result, SharedRbacService};

/// 提供组织配置读取、影响预览和事务提交，并通过业务事实确认组织能否停用。
#[derive(Clone)]
pub struct OrganizationService {
    // 组织节点、成员、变更回执及人员资料所在的应用数据库。
    db: Database,
    // 共享角色权限服务，用于修改人员角色和读取策略版本。
    rbac: SharedRbacService,
    // 共享授权服务，负责核对组织配置权限和本次管理范围。
    authorization: Arc<AuthorizationService>,
    // 组织停用前向所属业务域确认未结事项的事实端口。
    business: Arc<dyn OrganizationBusinessPort>,
}

impl OrganizationService {
    /// 在应用启动时绑定组织数据库、共享权限服务和外域未结事实端口，供查询、预览及提交统一取证。
    pub fn new(
        db: Database,
        rbac: SharedRbacService,
        business: Arc<dyn OrganizationBusinessPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, rbac, authorization, business }
    }

    /// 从同一快照读取组织配置页面需要的节点、关系、人员和角色。调用方须具备入口权限；无管理范围时返回空组织结果并标记 `no_scope`。
    pub async fn state(&self, actor: &AuditActor) -> Result<OrganizationStateView> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.state_snapshot(&actor, executor).await })
            })
            .await
    }

    // 按当前管理范围裁剪组织、补充配置候选和版本；交付前核对原读取阶段仍有效。
    async fn state_snapshot(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationStateView> {
        let access = self.access(actor, executor).await?;
        let visible = visible_state(access.organizations.clone(), &access);
        let people = people_for(&self.db, &visible, access.as_of, executor).await?;
        let roles = roles_for(&self.db, executor).await?;

        access.ensure_current(executor)?;
        Ok(OrganizationStateView::compose(
            visible,
            access.scope_version.clone(),
            access.policy_version,
            access.as_of.as_utc().to_rfc3339(),
            !access.scope.has_role_scope(),
            people,
            roles,
        ))
    }

    /// 在提交前试算组织变更，返回当前范围可见的前后状态。沿用正式提交的权限、版本和业务校验，全程只读取和计算。
    pub async fn preview(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        self.execute(actor, request, true).await
    }

    /// 在同一事务内写入组织关系、全局版本、幂等回执和审计所需的前后状态。相同请求可以回放；越权、载荷冲突或持久化失败时回滚。
    pub async fn change(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        self.execute(actor, request, false).await
    }

    // 为预览和提交选择事务。正式提交若涉及角色变更则走策略事务，其余共用组织快照和校验路径。
    async fn execute(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
        preview: bool,
    ) -> Result<OrganizationChangeReceipt> {
        request.validate()?;
        if !preview
            && matches!(&request.change,
            OrganizationOperation::UpdatePersonProfile { profile } if profile.role_ids.is_some())
        {
            return self.save_profile_roles(actor, request).await;
        }

        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.execute_change(&actor, request, preview, executor).await })
            })
            .await
    }

    // 在调用方事务内检查管理边界并准备或重放回执；仅在需要正式提交时写入人员资料及组织变更。
    async fn execute_change(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
        preview: bool,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationChangeReceipt> {
        let access = self.profile_access(actor, &request.change, executor).await?;

        let repository = OrganizationRepository::new(&self.db);
        let receipt_id = format!("{}:{}", actor.id(), request.idempotency_key);
        let existing = repository.receipt(&receipt_id, executor).await?;
        if existing.is_none() {
            self.ensure_change(&request.change, &access, executor).await?;
            self.update_profile_name(&request.change, false, executor).await?;
            if self.rbac.current_policy_revision().await? != access.policy_version {
                return Err(Error::ConflictError("授权版本已变化，请刷新后重试".into()));
            }
        }

        let (mut receipt, persist) = prepare_organization_change(
            preview,
            existing,
            request,
            &access,
            &id_generator::next_id(),
            actor.id(),
            Instant::from_unix_secs(Instant::now().unix_secs()),
        )?;
        access.ensure_current(executor)?;

        if persist {
            self.update_profile_name(&receipt.request.change, true, executor).await?;
            self.assign_profile_roles(actor, &receipt.request.change, access.policy_version, executor)
                .await?;
            repository.save(&mut receipt, executor).await?;
        }
        Ok(receipt)
    }

    // 通过共享授权服务取得组织配置上下文，供组织配置页面读取使用。
    async fn access(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<SnapshotScope> {
        self.authorization.organization_scope(actor, &organization::LIST, executor).await
    }

    // 按命令核对管理范围、组织关系及目标账号资格，保护调岗、负责人配置和停用的业务约束。
    async fn ensure_change(
        &self,
        change: &OrganizationOperation,
        access: &AuthorizedDataScope,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        access::ensure_targets(change, access)?;
        match change {
            OrganizationOperation::UpdatePersonProfile { profile } => {
                self.ensure_account(&profile.user_id, executor).await?;
                let roles = self.rbac.role_ids(AccountKind::Admin, &profile.user_id).await?;
                profile.validate_roles(&roles)?;
            },
            OrganizationOperation::SetLeader { user_id: Some(user_id), .. }
            | OrganizationOperation::TransferMember { user_id, .. } => {
                self.ensure_account(user_id, executor).await?;
            },
            OrganizationOperation::TransferMembers { user_ids, .. } => {
                for user_id in user_ids {
                    self.ensure_account(user_id, executor).await?;
                }
            },
            OrganizationOperation::DisableUnit { org_unit_id } => {
                self.ensure_no_unsettled(org_unit_id, executor).await?
            },
            _ => {},
        }
        Ok(())
    }

    // 向权威业务端口确认组织下没有待交接事项。有未结记录或查询失败时阻止停用。
    async fn ensure_no_unsettled(&self, org: &str, executor: &mut dyn Executor) -> Result<()> {
        if self.business.has_unsettled_business(org, executor).await? {
            return Err(Error::ConflictError("组织仍有未结业务，请先完成交接".into()));
        }
        Ok(())
    }

    // 变更成员或负责人前，确认目标账号是当前有效的后台人员。
    async fn ensure_account(&self, user: &str, executor: &mut dyn Executor) -> Result<()> {
        if self.db.accounts().find_by_id(user, executor).await?.is_none_or(|a| !a.is_active_backoffice()) {
            return Err(Error::ValidationError("接收人不是有效后台账号".into()));
        }
        Ok(())
    }
}

// 根据可见组织快照为页面补充后台人员姓名、登录名、状态和主属组织。主属关系冲突时返回错误。
async fn people_for(
    db: &Database,
    visible: &OrganizationState,
    as_of: Instant,
    executor: &mut dyn Executor,
) -> Result<Vec<OrgPersonView>> {
    let accounts = db.accounts().list_by_kind(AccountKind::Admin, executor).await?;
    let mut people = Vec::with_capacity(accounts.len());
    for account in accounts {
        let active = account.is_active_backoffice();
        let own_org_unit_id = visible.own_org(&account.base.id, as_of)?.map(str::to_owned);
        people.push(OrgPersonView {
            id: account.base.id,
            label: account.name,
            account: account.secret.account().to_string(),
            active,
            own_org_unit_id,
        });
    }
    people.sort_by(|left, right| (&left.label, &left.id).cmp(&(&right.label, &right.id)));
    Ok(people)
}

// 读取启用角色并按名称排序，供人员资料配置页面展示可分配角色。
async fn roles_for(db: &Database, executor: &mut dyn Executor) -> Result<Vec<OrgRoleView>> {
    let roles = db.roles().list_enabled(executor).await?;
    let mut views = roles
        .into_iter()
        .map(|role| OrgRoleView { id: role.base.id, name: role.name, enabled: !role.disabled })
        .collect::<Vec<_>>();
    views.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
    views.dedup_by(|left, right| left.id == right.id);
    Ok(views)
}
