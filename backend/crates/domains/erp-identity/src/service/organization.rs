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
use crate::service::authorization::OrganizationScope;
use crate::{AccessControlExt, AuthorizationService, Error, Result, SharedRbacService};

/// 组织管理应用服务；停用检查必须显式装配真实业务事实 Port。
#[derive(Clone)]
pub struct OrganizationService {
    db: Database,
    rbac: SharedRbacService,
    authorization: Arc<AuthorizationService>,
    business: Arc<dyn OrganizationBusinessPort>,
}

impl OrganizationService {
    /// 装配组织用例和外域未结事实检查。
    /// 返回无宽范围兜底的组织服务。
    ///
    /// `rbac`：共享 RBAC 服务；`authorization`：应用注入的共享统一授权服务。
    pub fn new(
        db: Database,
        rbac: SharedRbacService,
        business: Arc<dyn OrganizationBusinessPort>,
        authorization: Arc<AuthorizationService>,
    ) -> Self {
        Self { db, rbac, authorization, business }
    }

    /// 返回当前有权配置的组织、关系及展示字段。
    /// 缺范围返回空集并标记 `no_scope`；所属部门不代替 `org_unit:list`。
    ///
    /// 配置边界内的组织事实，以及范围版本、空集原因和人员/角色标签。
    ///
    /// 无读取动作、失效账号或快照不一致时拒绝。
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

    // 沿同一快照取得配置范围、展示事实并完成交付期限检查。
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

    /// 预览通过同一校验准备的变更前后事实，不写入组织或业务任务。
    /// 返回可见边界内的变更回执。
    ///
    /// 与正式提交相同的权限、版本和业务约束错误。
    ///
    /// `request`：含期望版本、幂等键和变更命令。
    pub async fn preview(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        self.execute(actor, request, true).await
    }

    /// 原子提交组织关系、全局版本、幂等回执及前后值审计。
    /// 返回已提交或幂等回放的变更回执。
    ///
    /// 并发或幂等异载荷冲突、越权及存储失败时全部回滚。
    ///
    /// `request`：含期望版本、幂等键和变更命令。
    pub async fn change(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        self.execute(actor, request, false).await
    }

    // 统一预览和提交的事务快照与授权校验路径。
    // 预览与提交共用校验；只有提交且无回放时持久化。
    //
    // 可见边界内的变更回执。
    //
    // 权限、期望版本、越界、未结业务或幂等异载荷失败时回滚。
    //
    // `request`：含期望版本、幂等键和变更命令；`preview`：预览不写入仓储。
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

    // 一个事务完成授权、资料、组织关系及幂等回执，不自行开启事务。
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

    // 为组织配置读取建立资源授权上下文，操作入口权限和所在部门管理范围各自核验。
    async fn access(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<OrganizationScope> {
        self.authorization.organization_scope(actor, &organization::LIST, executor).await
    }

    // 组织写入前核对目标、原管辖边界和接收方资格；验证组织关系不代表完成业务对象责任交接。
    async fn ensure_change(
        &self,
        change: &OrganizationOperation,
        access: &AuthorizedDataScope,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        access::ensure_targets(change, access)?;
        if let OrganizationOperation::UpdatePersonProfile { profile } = change {
            self.ensure_account(&profile.user_id, executor).await?;
            let roles = self.rbac.role_ids(AccountKind::Admin, &profile.user_id).await?;
            profile.validate_roles(&roles)?;
            for operation in profile.operations()? {
                Box::pin(self.ensure_change(&operation, access, executor)).await?;
            }
            return Ok(());
        }

        match change {
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

    // 停用组织前检查权威业务端口是否仍有未结事项；有记录或取证失败都不能视作可停用。
    async fn ensure_no_unsettled(&self, org: &str, executor: &mut dyn Executor) -> Result<()> {
        if self.business.has_unsettled_business(org, executor).await? {
            return Err(Error::ConflictError("组织仍有未结业务，请先完成交接".into()));
        }
        Ok(())
    }

    // 创建或调整成员关系前确认目标为有效后台账号，不将主属关系授予外部或失效账号。
    async fn ensure_account(&self, user: &str, executor: &mut dyn Executor) -> Result<()> {
        if self.db.accounts().find_by_id(user, executor).await?.is_none_or(|a| !a.is_active_backoffice()) {
            return Err(Error::ValidationError("接收人不是有效后台账号".into()));
        }
        Ok(())
    }
}

// 读取组织页面人员展示与分派候选，消费账号的展示字段。
// 后台账号 ID、显示名、登录账号、状态和当前主属组织；排序与空集语义不变。
//
// 账号读取或主属关系冲突时失败。
//
// `db`：数据库句柄（只借用，不克隆 client/actor）；`visible`：当前可管理组织事实（借用）。
// `as_of`：与范围解析相同的时点；`executor`：调用方事务执行器。
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

// 读取账号资料的可选角色，不因旧管理关系补入失效角色。
// 按名称稳定排序的启用角色。
//
// 角色读取失败时返回错误。
//
// `db`：数据库句柄（只借用，不克隆 client/actor）；`executor`：调用方事务执行器。
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
