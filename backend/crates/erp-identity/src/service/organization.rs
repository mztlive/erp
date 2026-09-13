//! 内部组织查询、影响预览及事务变更。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use std::sync::Arc;

use crate::access_control::ScopedObject;
use crate::entity::organization_change::*;
use crate::ports::OrganizationBusinessPort;
use crate::repository::OrganizationRepository;
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

    /// 返回当前有权配置的组织及成员关系。
    ///
    /// # 错误
    /// 无读取动作、失效账号或快照不一致时拒绝。
    pub async fn state(&self, actor: &AuditActor) -> Result<OrganizationState> {
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move {
                    let access = this.access(&actor, "list", session).await?;
                    Ok::<_, Error>(visible_state(access.organizations.clone(), &access))
                })
            })
            .await
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
            .with_transaction(move |session| {
                Box::pin(async move {
                    let access = this.access(&actor, "manage", session).await?;
                    let repository = OrganizationRepository::new(&this.db);
                    let receipt_id = format!("{}:{}", actor.id(), request.idempotency_key);
                    if let Some(receipt) = repository.receipt(&receipt_id, session).await? {
                        return replay(receipt, &request, &access);
                    }
                    this.ensure_change(&request.change, &access, session).await?;
                    if this.rbac.current_policy_revision().await? != access.policy_version {
                        return Err(Error::ConflictError("授权版本已变化，请刷新后重试".into()));
                    }
                    let at = Instant::now();
                    let after = access.organizations.changed(&request, &id, actor.id(), at)?;
                    let mut receipt = OrganizationChangeReceipt {
                        base: BaseModel::new(receipt_id),
                        actor_id: actor.id().into(),
                        request,
                        before: access.organizations.clone(),
                        after,
                        as_of: at,
                    };
                    if !preview {
                        repository.save(&mut receipt, session).await?;
                    }
                    Ok::<_, Error>(visible_receipt(receipt, &access))
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
        ensure_targets(change, access)?;
        match change {
            OrganizationOperation::TransferMember { user_id, .. } => {
                self.ensure_account(user_id, executor).await?;
            }
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
            }
            OrganizationOperation::DisableUnit { org_unit_id } => {
                self.ensure_no_unsettled(org_unit_id, executor).await?
            }
            _ => {}
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
        if self
            .db
            .accounts()
            .find_by_id(user, executor)
            .await?
            .is_none_or(|a| !a.is_active_backoffice())
        {
            return Err(Error::ValidationError("接收人不是有效后台账号".into()));
        }
        Ok(())
    }
}

/// 以组织对象身份判断配置边界，不使用“同部门”作为权限。
fn covers(access: &AuthorizedDataScope, id: Option<&str>) -> bool {
    access.scope.allows(
        &ScopedObject {
            owned: false,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: id,
            settlement_party_id: None,
            warehouse_id: None,
        },
        false,
    )
}

/// 组织移动和包含下级授权须覆盖整个相关子树；调岗同时检查原组织和新组织。
fn ensure_targets(change: &OrganizationOperation, access: &AuthorizedDataScope) -> Result<()> {
    use OrganizationOperation::*;
    let state = &access.organizations;
    let mut targets = Vec::<Option<&str>>::new();
    match change {
        CreateUnit { parent_id, .. } => targets.push(parent_id.as_deref()),
        MoveUnit {
            org_unit_id,
            parent_id,
        } => {
            targets.push(parent_id.as_deref());
            add_subtree_targets(state, org_unit_id, &mut targets)?;
        }
        RenameUnit { org_unit_id, .. } | DisableUnit { org_unit_id } => targets.push(Some(org_unit_id)),
        TransferMember { user_id, org_unit_id } => {
            targets.push(Some(org_unit_id));
            targets.push(state.own_org(user_id, access.as_of)?);
        }
        EndMembership { user_id } => targets.push(state.own_org(user_id, access.as_of)?),
        GrantManagement {
            org_unit_id,
            include_descendants,
            ..
        } => {
            targets.push(Some(org_unit_id));
            if *include_descendants {
                add_subtree_targets(state, org_unit_id, &mut targets)?;
            }
        }
        RevokeManagement { assignment_id } => {
            let grant = state
                .management
                .iter()
                .find(|g| g.base.id == *assignment_id)
                .ok_or_else(|| Error::NotFound("管理关系不存在".into()))?;
            targets.push(Some(&grant.org_unit_id));
            if grant.include_descendants {
                add_subtree_targets(state, &grant.org_unit_id, &mut targets)?;
            }
        }
    }
    if targets.into_iter().any(|id| !covers(access, id)) {
        return Err(Error::Forbidden("目标超出组织配置管理边界".into()));
    }
    Ok(())
}

/// 使用完整树验证子树边界；不按当前页或名称判断。
fn add_subtree_targets<'a>(
    state: &'a OrganizationState,
    id: &str,
    targets: &mut Vec<Option<&'a str>>,
) -> Result<()> {
    let tree = crate::entity::organization::OrgTree::new(&state.units)?;
    let ids = tree.expand(id, true)?;
    targets.extend(
        state
            .units
            .iter()
            .filter(|u| ids.contains(&u.base.id))
            .map(|u| Some(u.base.id.as_str())),
    );
    Ok(())
}

/// 幂等回放前仍检查当前管理边界，撤权后不能通过回执取回旧宽范围事实。
fn replay(
    receipt: OrganizationChangeReceipt,
    request: &OrganizationChangeRequest,
    access: &AuthorizedDataScope,
) -> Result<OrganizationChangeReceipt> {
    if receipt.request != *request {
        return Err(Error::ConflictError("幂等键已用于不同变更".into()));
    }
    ensure_targets(&request.change, access)?;
    Ok(visible_receipt(receipt, access))
}

/// 客户端只接收当前可管理节点和相应关系，隐藏不可见父身份。
fn visible_state(mut state: OrganizationState, access: &AuthorizedDataScope) -> OrganizationState {
    state.units.retain(|u| covers(access, Some(&u.base.id)));
    let ids = state
        .units
        .iter()
        .map(|u| u.base.id.clone())
        .collect::<std::collections::BTreeSet<_>>();
    for unit in &mut state.units {
        if unit.parent_id.as_ref().is_some_and(|id| !ids.contains(id)) {
            unit.parent_id = None;
        }
    }
    state.memberships.retain(|m| ids.contains(&m.org_unit_id));
    state.management.retain(|m| ids.contains(&m.org_unit_id));
    state
}

/// 审计回执仅投影当前管理范围内的前后事实。
fn visible_receipt(
    mut receipt: OrganizationChangeReceipt,
    access: &AuthorizedDataScope,
) -> OrganizationChangeReceipt {
    receipt.before = visible_state(receipt.before, access);
    receipt.after = visible_state(receipt.after, access);
    receipt
}
