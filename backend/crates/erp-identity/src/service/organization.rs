//! 内部组织查询、影响预览及事务变更。

mod access;

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use self::access::{prepare_organization_change, visible_state};
use crate::dto::{OrgPersonView, OrgRoleView, OrganizationStateView};
use crate::entity::organization_change::*;
use crate::ports::OrganizationBusinessPort;
use crate::repository::OrganizationRepository;
use crate::repository::prelude::*;
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use crate::{AccessControlExt, Error, Result, SharedRbacService};

/// 组织管理应用服务；停用检查必须显式装配真实业务事实 Port。
#[derive(Clone)]
pub struct OrganizationService {
    db: Database,
    rbac: SharedRbacService,
    business: Arc<dyn OrganizationBusinessPort>,
}

impl OrganizationService {
    /// 装配组织用例和外域未结事实检查。
    ///
    /// # 返回
    /// 返回无宽范围兜底的组织服务。
    pub fn new(db: Database, rbac: SharedRbacService, business: Arc<dyn OrganizationBusinessPort>) -> Self {
        Self { db, rbac, business }
    }

    /// 返回当前有权配置的组织、关系及展示字段。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    ///
    /// # 返回
    /// 配置边界内的组织事实，以及范围版本、空集原因和人员/角色标签。
    ///
    /// # 错误
    /// 无读取动作、失效账号或快照不一致时拒绝。
    ///
    /// # 关键业务约束
    /// 缺范围返回空集并标记 `no_scope`；部门负责人身份不代替 `org_unit:list`。
    pub async fn state(&self, actor: &AuditActor) -> Result<OrganizationStateView> {
        let mut no_tx = NoTransaction;
        let access = self.access(actor, "list", &mut no_tx).await?;
        let visible = visible_state(access.organizations.clone(), &access);
        let people = self.people_for_view(&visible, access.as_of, &mut no_tx).await?;
        let roles = self.roles_for_view(&visible, &mut no_tx).await?;
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

    /// 读取组织页面人员展示与分派候选，不含联系方式。
    ///
    /// # 参数
    /// * `visible` - 当前可管理组织事实
    /// * `as_of` - 与范围解析相同的时点
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 后台账号 ID、显示名、登录账号、状态和当前主属组织。
    ///
    /// # 错误
    /// 账号读取或主属关系冲突时失败。
    ///
    /// # 关键业务约束
    /// 不要求账号管理权限；分派仍只接受有效后台账号。不返回邮箱、电话或密钥。
    async fn people_for_view(
        &self,
        visible: &OrganizationState,
        as_of: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Vec<OrgPersonView>> {
        people_for(&self.db, visible, as_of, executor).await
    }

    /// 读取管理授权展示与可选角色，不含权限清单。
    ///
    /// # 参数
    /// * `visible` - 当前可管理组织事实
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 启用角色及现有管理关系引用的角色名称。
    ///
    /// # 错误
    /// 角色读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// 组织配置不把角色权限当作业务执行权授予。
    async fn roles_for_view(
        &self,
        visible: &OrganizationState,
        executor: &mut dyn Executor,
    ) -> Result<Vec<OrgRoleView>> {
        roles_for(&self.db, visible, executor).await
    }

    /// 预览通过同一校验准备的变更前后事实，不写入组织或业务任务。
    ///
    /// # 错误
    /// 与正式提交相同的权限、版本和业务约束错误。
    pub async fn preview(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        self.execute(actor, request, true).await
    }

    /// 原子提交组织关系、全局版本、幂等回执及前后值审计。
    ///
    /// # 错误
    /// 并发或幂等异载荷冲突、越权及存储失败时全部回滚。
    pub async fn change(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
    ) -> Result<OrganizationChangeReceipt> {
        self.execute(actor, request, false).await
    }

    /// 统一预览和提交的事务快照与授权校验路径。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `request` - 含期望版本、幂等键和变更命令
    /// * `preview` - 预览不写入仓储
    ///
    /// # 返回
    /// 可见边界内的变更回执。
    ///
    /// # 错误
    /// 权限、期望版本、越界、未结业务或幂等异载荷失败时回滚。
    ///
    /// # 关键业务约束
    /// 预览与提交共用校验；只有提交且无回放时持久化。
    async fn execute(
        &self,
        actor: &AuditActor,
        request: OrganizationChangeRequest,
        preview: bool,
    ) -> Result<OrganizationChangeReceipt> {
        request.validate()?;
        let this = self.clone();
        let actor = actor.clone();
        let id = id_generator::next_id();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = this.access(&actor, "manage", executor).await?;
                    let repository = OrganizationRepository::new(&this.db);
                    let receipt_id = format!("{}:{}", actor.id(), request.idempotency_key);
                    let existing = repository.receipt(&receipt_id, executor).await?;
                    if existing.is_none() {
                        this.ensure_change(&request.change, &access, executor).await?;
                        if this.rbac.current_policy_revision().await? != access.policy_version {
                            return Err(Error::ConflictError("授权版本已变化，请刷新后重试".into()));
                        }
                    }
                    let (mut receipt, persist) = prepare_organization_change(
                        preview,
                        existing,
                        request,
                        &access,
                        &id,
                        actor.id(),
                        // 持久化按秒截断，同秒内重复调岗由实体守卫拒绝，避免纳秒精度截断后产生零长有效期。
                        Instant::from_unix_secs(Instant::now().unix_secs()),
                    )?;
                    if persist {
                        repository.save(&mut receipt, executor).await?;
                    }
                    Ok::<_, Error>(receipt)
                })
            })
            .await
    }

    /// 解析组织配置资源，部门管理身份不代替管理动作。
    async fn access(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(actor, "org_unit", action, executor)
            .await
    }

    /// 核对修改目标、原边界及接收方资格，组织关系不自动交接业务对象。
    async fn ensure_change(
        &self,
        change: &OrganizationOperation,
        access: &AuthorizedDataScope,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        access::ensure_targets(change, access)?;
        match change {
            OrganizationOperation::TransferMember { user_id, .. } => {
                self.ensure_account(user_id, executor).await?;
            },
            OrganizationOperation::GrantManagement { user_id, role_id, .. } => {
                self.ensure_account(user_id, executor).await?;
                let roles = self.rbac.role_ids(erp_core::AccountKind::Admin, user_id).await?;
                if !roles.contains(role_id)
                    || self
                        .db
                        .roles()
                        .enabled_roles(std::slice::from_ref(role_id), executor)
                        .await?
                        .is_empty()
                {
                    return Err(Error::ValidationError("接收人必须持有有效的指定角色".into()));
                }
            },
            OrganizationOperation::DisableUnit { org_unit_id } => {
                self.ensure_no_unsettled(org_unit_id, executor).await?
            },
            _ => {},
        }
        Ok(())
    }

    /// 未结业务会阻止组织停用，查询失败不能视为没有业务。
    async fn ensure_no_unsettled(&self, org: &str, executor: &mut dyn Executor) -> Result<()> {
        if self.business.has_unsettled_business(org, executor).await? {
            return Err(Error::ConflictError("组织仍有未结业务，请先完成交接".into()));
        }
        Ok(())
    }

    /// 成员与管理关系只授予有效后台账号。
    async fn ensure_account(&self, user: &str, executor: &mut dyn Executor) -> Result<()> {
        if self.db.accounts().find_by_id(user, executor).await?.is_none_or(|a| !a.is_active_backoffice()) {
            return Err(Error::ValidationError("接收人不是有效后台账号".into()));
        }
        Ok(())
    }
}

/// 读取组织页面人员展示与分派候选（借用可见状态，只克隆可见子集）。
///
/// # 参数
/// * `db` - 数据库句柄（只借用，不克隆 client/actor）
/// * `visible` - 当前可管理组织事实（借用）
/// * `as_of` - 与范围解析相同的时点
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 后台账号 ID、显示名、登录账号、状态和当前主属组织；排序与空集语义不变。
///
/// # 错误
/// 账号读取或主属关系冲突时失败。
async fn people_for(
    db: &Database,
    visible: &OrganizationState,
    as_of: Instant,
    executor: &mut dyn Executor,
) -> Result<Vec<OrgPersonView>> {
    let accounts = db.accounts().list_by_kind(erp_core::AccountKind::Admin, executor).await?;
    let mut people = Vec::with_capacity(accounts.len());
    for account in &accounts {
        people.push(OrgPersonView {
            id: account.base.id.clone(),
            label: account.name.clone(),
            account: account.secret.account().to_string(),
            active: account.is_active_backoffice(),
            own_org_unit_id: visible.own_org(&account.base.id, as_of)?.map(str::to_owned),
        });
    }
    people.sort_by(|left, right| (&left.label, &left.id).cmp(&(&right.label, &right.id)));
    Ok(people)
}

/// 读取管理授权展示与可选角色（借用可见状态，只克隆可见子集）。
///
/// # 参数
/// * `db` - 数据库句柄（只借用，不克隆 client/actor）
/// * `visible` - 当前可管理组织事实（借用）
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 启用角色及现有管理关系引用的角色名称；排序与去重语义不变。
///
/// # 错误
/// 角色读取失败时返回错误。
async fn roles_for(
    db: &Database,
    visible: &OrganizationState,
    executor: &mut dyn Executor,
) -> Result<Vec<OrgRoleView>> {
    let mut roles = db.roles().list_enabled(executor).await?;
    let known = roles.iter().map(|role| role.base.id.clone()).collect::<std::collections::BTreeSet<_>>();
    let extra = visible
        .management
        .iter()
        .map(|item| item.role_id.clone())
        .filter(|id| !known.contains(id))
        .collect::<Vec<_>>();
    roles.extend(db.roles().roles_by_ids(&extra, executor).await?);
    let mut views = roles
        .into_iter()
        .map(|role| OrgRoleView { id: role.base.id, name: role.name, enabled: !role.disabled })
        .collect::<Vec<_>>();
    views.sort_by(|left, right| (&left.name, &left.id).cmp(&(&right.name, &right.id)));
    views.dedup_by(|left, right| left.id == right.id);
    Ok(views)
}
