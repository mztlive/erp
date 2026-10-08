//! 责任队列授权快照、范围过滤与允许动作。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_identity::{Permission, PermissionSet};
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use erp_workflow::ports::WorkflowQueueAccessFact;
use erp_workflow::repository::prelude::*;
use erp_workflow::{DocumentRegistryExt, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::facts::{WorkbenchObjectFact, WorkbenchObjectFactMap, apply_object_display, object_policy};
use super::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemFilter, WorkItemScope,
    WorkbenchReadService, dto,
};
use crate::errors::{Error, Result};

pub(super) const MANAGE_PERMISSION: &str = "work_item:manage";
pub(super) const REASSIGN_PERMISSION: &str = "work_item:reassign";
pub(super) const CLOSE_PERMISSION: &str = "work_item:close";

pub(super) struct ActorAccess {
    pub(super) actor_id: String,
    pub(super) permissions: Vec<Permission>,
    pub(super) participant_document_ids: HashSet<String>,
    pub(super) managed_owner_ids: Option<Vec<String>>,
    pub(super) can_manage: bool,
}

impl ActorAccess {
    /// 构造责任队列授权快照。
    ///
    /// # 参数
    /// * `actor_id` - 账号稳定 ID
    ///
    /// # 返回
    /// 返回权限、参与与管理范围全空的快照。
    ///
    /// # 错误
    /// 无。
    pub(super) fn new(actor_id: String) -> Self {
        Self {
            actor_id,
            permissions: Vec::new(),
            participant_document_ids: HashSet::new(),
            managed_owner_ids: None,
            can_manage: false,
        }
    }

    /// 设置已授予权限。
    ///
    /// # 参数
    /// * `permissions` - 已授予权限
    ///
    /// # 返回
    /// 返回更新后的快照。
    ///
    /// # 错误
    /// 无。
    pub(super) fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
        self
    }

    /// 设置参与单据。
    ///
    /// # 参数
    /// * `participant_document_ids` - 参与单据集合
    ///
    /// # 返回
    /// 返回更新后的快照。
    ///
    /// # 错误
    /// 无。
    pub(super) fn with_participant_document_ids(mut self, participant_document_ids: HashSet<String>) -> Self {
        self.participant_document_ids = participant_document_ids;
        self
    }

    /// 设置管理范围。
    ///
    /// # 参数
    /// * `managed_owner_ids` - 管理组织下的任务负责人
    /// * `can_manage` - 是否具备管理范围
    ///
    /// # 返回
    /// 返回更新后的快照。
    ///
    /// # 错误
    /// 无。
    pub(super) fn with_managed_scope(
        mut self,
        managed_owner_ids: Option<Vec<String>>,
        can_manage: bool,
    ) -> Self {
        self.managed_owner_ids = managed_owner_ids;
        self.can_manage = can_manage;
        self
    }
}

impl<A: erp_workflow::WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 按账号类型与稳定 ID 构造责任队列授权快照。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `actor_id` - 账号稳定 ID
    /// * `executor` - 当前读取执行器
    ///
    /// # 返回
    /// 返回权限代码、参与单据、管理组织与责任范围。
    ///
    /// # 错误
    /// 角色、权限、数据范围或参与关系读取失败时返回错误。
    pub(super) async fn actor_access_for(
        &self,
        account_kind: erp_core::AccountKind,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ActorAccess> {
        let actor = AuditActor::new(actor_id.to_string(), actor_id.to_string(), account_kind);
        if let Some((access, _)) = optimized_access(&self.auth, &actor, false, executor).await? {
            return Ok(access);
        }
        self.legacy_actor_access_for(account_kind, actor_id, executor).await
    }

    /// 首拍身份版本与访问事实可共用只读阶段，末拍重验必须另行调用原版本端口。
    ///
    /// # 参数
    /// * `actor` - 当前审计账号。
    /// * `executor` - 当前读取执行器。
    ///
    /// # 返回
    /// 返回授权快照与首拍身份版本。
    ///
    /// # 错误
    /// 批量身份端口失败、首拍版本缺失，或回退路径的角色、权限与范围读取失败时返回错误。
    ///
    /// # Panics
    /// 优化端口在要求版本时仍给出空版本属于程序错误；该情况已先返回 `Error::Internal`，随后的 `expect` 不应触发。
    pub(super) async fn queue_access_with_version(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(ActorAccess, String)> {
        if let Some((access, version)) = optimized_access(&self.auth, actor, true, executor).await? {
            return Ok((access, version.expect("首拍授权版本已校验")));
        }
        let version = self.auth.queue_scope_version(actor, executor).await?;
        let access = self.legacy_actor_access_for(actor.kind(), actor.id(), executor).await?;
        Ok((access, version))
    }

    /// 未装配批量身份端口时沿原角色、权限、参与和管理范围的查询顺序读取。
    async fn legacy_actor_access_for(
        &self,
        account_kind: erp_core::AccountKind,
        actor_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ActorAccess> {
        let role_ids = self.auth.role_ids_with_executor(account_kind, actor_id, executor).await?;
        let permissions = self
            .auth
            .permission_codes(account_kind, actor_id)
            .await?
            .into_iter()
            .map(|code| Permission::parse(&code).expect("granted permission must be valid"))
            .collect::<Vec<_>>();
        let participant_document_ids = self
            .db
            .document_participants()
            .document_ids_by_user(actor_id, executor)
            .await?
            .into_iter()
            .collect();
        let manage_permission = Permission::parse(MANAGE_PERMISSION).expect("固定权限合法");
        let has_manage_permission =
            permissions.iter().any(|permission| permission.covers(&manage_permission));
        let manage_role_ids =
            self.roles_granting_permission(&role_ids, &manage_permission, has_manage_permission).await?;
        let managed_owner_ids = self
            .auth
            .managed_task_owners(
                &AuditActor::new(actor_id.to_string(), actor_id.to_string(), account_kind),
                executor,
            )
            .await?;
        Ok(ActorAccess::new(actor_id.to_string())
            .with_permissions(permissions)
            .with_participant_document_ids(participant_document_ids)
            .with_managed_scope(managed_owner_ids, !manage_role_ids.is_empty()))
    }

    /// 定位实际授予指定权限的角色，使管理数据范围与权限来源关联。
    ///
    /// # 参数
    /// * `role_ids` - 账号已有角色。
    /// * `permission` - 待定位的权限。
    /// * `account_has_permission` - 账号是否已具备该权限；为假时不查角色。
    ///
    /// # 返回
    /// 返回实际授予该权限的角色 ID；账号不具备该权限时为空。
    ///
    /// # 错误
    /// 角色权限判定失败时返回对应错误。
    pub(super) async fn roles_granting_permission(
        &self,
        role_ids: &[String],
        permission: &Permission,
        account_has_permission: bool,
    ) -> Result<Vec<String>> {
        if !account_has_permission {
            return Ok(Vec::new());
        }
        let mut granting_roles = Vec::new();
        for role_id in role_ids {
            if self.auth.enforce(&format!("role:{role_id}"), &permission.to_string()).await? {
                granting_roles.push(role_id.clone());
            }
        }
        Ok(granting_roles)
    }

    /// 把列表查询和授权快照收成仓储过滤条件。
    ///
    /// # 参数
    /// * `query` - 工作项列表查询。
    /// * `actor` - 当前账号。
    /// * `access` - 已装载的授权快照。
    ///
    /// # 返回
    /// `Mine` 限定本人；`Managed` 限定管理范围内的负责人；`History` 带上历史操作人，能管理且管理负责人不是空列表时再附带可管理负责人。
    ///
    /// # 错误
    /// `Managed` 且账号没有管理范围时返回 `Error::Forbidden`。
    pub(super) fn scope_filter(
        &self,
        query: &dto::WorkItemListQuery,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<WorkItemFilter> {
        let mut filter = WorkItemFilter {
            work_item_types: query.work_item_types.clone(),
            statuses: query.statuses.clone(),
            priorities: query.priorities.clone(),
            query: query.query.clone(),
            object_access_shapes: Some(object_access_shapes(access)),
            page: query.page,
            page_size: query.page_size,
            sort_by: Some(query.sort_by.to_string()),
            sort_ascending: query.sort_ascending,
            ..WorkItemFilter::default()
        };
        match query.scope {
            WorkItemScope::Mine => filter.owner_user_id = Some(actor.id().to_string()),
            WorkItemScope::Managed => {
                ensure_managed_access(access)?;
                filter.managed_owner_ids = access.managed_owner_ids.clone();
            },
            WorkItemScope::History => {
                filter.history_actor_id = Some(actor.id().to_string());
                if access.can_manage && !access.managed_owner_ids.as_ref().is_some_and(Vec::is_empty) {
                    filter.history_managed_owner_ids = Some(access.managed_owner_ids.clone());
                }
            },
        }
        Ok(filter)
    }

    /// 计算当前视图下的处理状态与允许动作。
    ///
    /// # 参数
    /// * `item` - 已授权的工作项投影。
    /// * `scope` - 当前队列范围。
    /// * `actor` - 当前账号。
    /// * `access` - 授权快照。
    ///
    /// # 返回
    /// 历史范围或非开放任务返回无动作的就绪结果；开放任务附带允许动作。当前处理阻塞恒为空。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn view_access(
        &self,
        item: &dto::WorkItemFields,
        scope: WorkItemScope,
        actor: &AuditActor,
        access: &ActorAccess,
    ) -> Result<ViewAccess> {
        if let Some(blocker) = self.processing_blocker(None::<&str>)? {
            return Ok(ViewAccess::blocked(blocker));
        }
        if scope == WorkItemScope::History || item.status != WorkItemStatus::Open {
            return Ok(ViewAccess::ready(Vec::new()));
        }
        let actions = allowed_actions(item, scope, actor.id(), access);
        Ok(ViewAccess::ready(actions))
    }

    fn processing_blocker(&self, _step_id: Option<&str>) -> Result<Option<ProcessingBlockerView>> {
        Ok(None)
    }
}

/// 从兼容端口请求身份事实；缺失首拍版本失败关闭，未装配时保留原授权路径。
async fn optimized_access(
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    include_version: bool,
    executor: &mut dyn Executor,
) -> Result<Option<(ActorAccess, Option<String>)>> {
    let Some(mut facts) = auth.queue_access_facts(actor, include_version, executor).await? else {
        return Ok(None);
    };
    let version = facts.identity_version.take();
    if include_version && version.is_none() {
        return Err(Error::Internal("工作台首拍授权版本缺失".into()));
    }
    Ok(Some((access_from_facts(actor.id(), facts), version)))
}

/// 按原投影合同转移同阶段身份事实；对象范围和精确任务资格仍由后续路径验证。
fn access_from_facts(actor_id: &str, facts: WorkflowQueueAccessFact) -> ActorAccess {
    let permissions = facts
        .permission_codes
        .into_iter()
        .map(|code| Permission::parse(&code).expect("granted permission must be valid"))
        .collect();
    ActorAccess::new(actor_id.to_string())
        .with_permissions(permissions)
        .with_participant_document_ids(facts.participant_document_ids.into_iter().collect())
        .with_managed_scope(facts.managed_owner_ids, facts.can_manage)
}

/// 根据实体注册关系和当前权限形成仓储候选对象形状。
///
/// # 参数
/// * `access` - 当前账号的权限与参与范围事实
///
/// # 返回
/// 返回当前账号具备读取权限的工作项类型和业务对象类型组合。
///
/// # 错误
/// 不返回错误。
///
/// # Panics
/// 注册关系中的固定权限代码无法解析时，`has_permission` 会 `expect`。
pub(super) fn object_access_shapes(access: &ActorAccess) -> Vec<(WorkItemType, String)> {
    WorkItemType::registered_brief_relations()
        .iter()
        .filter(|policy| {
            has_permission(
                access,
                if policy.work_item_type.is_document_approval() {
                    "approval_instance:read"
                } else {
                    policy.read_permission
                },
            )
        })
        .map(|policy| (policy.work_item_type, policy.business_object_type.to_string()))
        .collect()
}

/// 判断当前访问快照是否覆盖实体关系要求的读取权限。
///
/// # 参数
/// * `access` - 当前账号权限快照
/// * `permission` - 实体关系注册的固定权限代码
///
/// # 返回
/// 任一已授予权限覆盖要求时返回 `true`。
///
/// # 错误
/// 不返回错误。
///
/// # Panics
/// `permission` 无法解析时 `expect`，属于程序错误。
pub(super) fn has_permission(access: &ActorAccess, permission: &str) -> bool {
    let required = Permission::parse(permission).expect("对象注册表权限必须合法");
    PermissionSet::new(access.permissions.clone()).covers_one(&required)
}

/// 按对象权限、参与关系与权威版本过滤工作项列表投影。
///
/// # 参数
/// * `rows` - 仓储返回的候选工作项行
/// * `access` - 当前账号访问事实
/// * `facts` - 已批量加载的业务对象事实
///
/// # 返回
/// 返回当前账号可见且已补齐对象展示字段的工作项。
///
/// # 错误
/// 无；未注册或无法证明访问权的候选项会失败关闭并被过滤。
pub(super) fn authorized_fields(
    rows: Vec<erp_workflow::WorkItemRow>,
    access: &ActorAccess,
    facts: &WorkbenchObjectFactMap,
) -> Vec<dto::WorkItemFields> {
    rows.into_iter()
        .filter_map(|row| {
            let policy = object_policy(row.work_item_type, &row.business_object_type)?;
            let fact = facts.get(&(policy.object_kind, row.business_object_id.clone()))?;
            if !has_permission(access, policy.read_permission)
                || !has_item_participation(
                    row.work_item_type,
                    row.owner_user_id.as_deref(),
                    &row.owner_role,
                    &row.owner_organization_id,
                    access,
                    fact,
                )
                || !fact.authority.subject_versions.accepts(&row.subject_version)
            {
                return None;
            }
            let mut fields = dto::WorkItemFields::from(row);
            apply_object_display(&mut fields, fact);
            Some(fields)
        })
        .collect()
}

/// 按对象权限、参与关系与权威版本形成单个工作项投影。
///
/// # 参数
/// * `item` - 待授权工作项实体
/// * `access` - 当前账号访问事实
/// * `facts` - 已加载的业务对象事实
///
/// # 返回
/// 授权通过时返回补齐对象展示字段的投影，否则返回 `None`。
///
/// # 错误
/// 无；未注册或无法证明访问权时失败关闭。
pub(super) fn authorized_item_fields(
    item: WorkItem,
    access: &ActorAccess,
    facts: &WorkbenchObjectFactMap,
) -> Option<dto::WorkItemFields> {
    let policy = object_policy(item.work_item_type, &item.business_object_type)?;
    let fact = facts.get(&(policy.object_kind, item.business_object_id.clone()))?;
    if !has_permission(access, policy.read_permission)
        || !has_item_participation(
            item.work_item_type,
            item.owner_user_id.as_deref(),
            &item.owner_role,
            &item.owner_organization_id,
            access,
            fact,
        )
        || !fact.authority.subject_versions.accepts(&item.subject_version)
    {
        return None;
    }
    let mut fields = dto::WorkItemFields::from(item);
    apply_object_display(&mut fields, fact);
    Some(fields)
}

/// 返回执行任务的完整权限；普通任务返回空集，未注册执行对象失败关闭。
///
/// # 参数
/// * `work_item_type` - 工作项类型。
/// * `business_object_type` - 业务对象类型。
///
/// # 返回
/// 供应商供给的 `BusinessException` 返回 `supplier_offering:resolve_supply_exception`。其余对象委托 `required_execution_permissions`：有注册结果时返回对应权限集，未注册返回 `None`。
///
/// # 错误
/// 不返回错误。
///
/// # Panics
/// 固定或注册的执行权限代码无法解析时 `expect`。
pub(super) fn required_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
) -> Option<PermissionSet> {
    if work_item_type == WorkItemType::BusinessException && business_object_type == "SUPPLIER_OFFERING" {
        return Some(PermissionSet::new([Permission::parse("supplier_offering:resolve_supply_exception")
            .expect("业务异常固定权限必须合法")]));
    }
    work_item_type.required_execution_permissions(business_object_type).map(|codes| {
        PermissionSet::new(
            codes
                .iter()
                .map(|code| Permission::parse(code).expect("完整执行权限必须合法"))
                .collect::<Vec<_>>(),
        )
    })
}

/// 判断账号是否覆盖执行任务在目标工作面所需的全部权限。
///
/// # 参数
/// * `work_item_type` - 工作项类型。
/// * `business_object_type` - 业务对象类型。
/// * `access` - 当前账号授权快照。
///
/// # 返回
/// 所需权限集存在且账号权限覆盖它时返回 `true`；未注册执行对象返回 `false`。
///
/// # 错误
/// 不返回错误。
///
/// # Panics
/// 执行权限代码无法解析时，经 `required_execution_permissions` 触发 `expect`。
pub(super) fn has_execution_permissions(
    work_item_type: WorkItemType,
    business_object_type: &str,
    access: &ActorAccess,
) -> bool {
    required_execution_permissions(work_item_type, business_object_type)
        .is_some_and(|required| PermissionSet::new(access.permissions.clone()).covers(&required))
}

/// 判断账号是否满足工作项的参与条件。
///
/// # 参数
/// * `work_item_type` - 工作项类型
/// * `owner_user_id` - 当前具体负责人
/// * `owner_role` - 责任角色标识
/// * `owner_organization_id` - 责任组织 ID
/// * `access` - 当前账号访问事实
/// * `fact` - 业务对象事实
///
/// # 返回
/// 具备对象参与关系，或是供给分配任务的具体负责人时返回 `true`。
///
/// # 错误
/// 无；调用方必须另行验证对象权限。
pub(super) fn has_item_participation(
    work_item_type: WorkItemType,
    owner_user_id: Option<&str>,
    owner_role: &str,
    owner_organization_id: &str,
    access: &ActorAccess,
    fact: &WorkbenchObjectFact,
) -> bool {
    let is_explicit_owner =
        work_item_type.uses_explicit_owner_authorization() && owner_user_id == Some(access.actor_id.as_str());
    is_explicit_owner
        || (access.can_manage && covers_owner(access, owner_user_id))
        || has_object_participation(access, owner_role, owner_organization_id, fact)
}

/// 判断账号是否因创建或参与根单据而具备对象参与关系。
///
/// # 参数
/// * `access` - 当前账号授权快照。
/// * `_owner_role` - 责任角色；本函数不使用。
/// * `_owner_organization_id` - 责任组织；本函数不使用。
/// * `fact` - 业务对象事实。
///
/// # 返回
/// 创建人是当前账号，或参与单据包含事实根单据时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub(super) fn has_object_participation(
    access: &ActorAccess,
    _owner_role: &str,
    _owner_organization_id: &str,
    fact: &WorkbenchObjectFact,
) -> bool {
    fact.authority.created_by == access.actor_id
        || access.participant_document_ids.contains(&fact.authority.root_document_id)
}

pub(super) struct ViewAccess {
    pub(super) processing_state: ProcessingState,
    pub(super) processing_blocker: Option<ProcessingBlockerView>,
    pub(super) allowed_actions: Vec<WorkItemAllowedAction>,
    pub(super) action_blockers: Vec<ProcessingBlockerView>,
}

impl ViewAccess {
    /// 构造可处理且无阻塞的视图访问结果。
    ///
    /// # 参数
    /// * `allowed_actions` - 当前允许的动作。
    ///
    /// # 返回
    /// 处理状态为 `ProcessingState::Ready`，阻塞为空。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn ready(allowed_actions: Vec<WorkItemAllowedAction>) -> Self {
        Self {
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            allowed_actions,
            action_blockers: Vec::new(),
        }
    }

    /// 构造被审批阻塞、不允许动作的视图访问结果。
    ///
    /// # 参数
    /// * `blocker` - 处理阻塞说明；同时写入主动作阻塞列表。
    ///
    /// # 返回
    /// 处理状态为 `ProcessingState::ApprovalBlocked`，允许动作为空。
    ///
    /// # 错误
    /// 不返回错误。
    pub(super) fn blocked(blocker: ProcessingBlockerView) -> Self {
        Self {
            processing_state: ProcessingState::ApprovalBlocked,
            processing_blocker: Some(blocker.clone()),
            allowed_actions: Vec::new(),
            action_blockers: vec![blocker],
        }
    }
}

/// 计算当前账号在指定队列范围内可执行的工作项动作。
///
/// # 参数
/// * `item` - 已完成对象授权的工作项投影
/// * `scope` - 当前队列范围
/// * `actor_id` - 当前账号 ID
/// * `access` - 当前账号访问事实
///
/// # 返回
/// 返回查看、处理、审批或管理动作集合。
///
/// # 错误
/// 无；未满足责任或管理条件的动作不会出现在结果中。
pub(super) fn allowed_actions(
    item: &dto::WorkItemFields,
    scope: WorkItemScope,
    actor_id: &str,
    access: &ActorAccess,
) -> Vec<WorkItemAllowedAction> {
    let mut actions = vec![WorkItemAllowedAction::View];
    if item.owner_user_id.as_deref() == Some(actor_id)
        && has_execution_permissions(item.work_item_type, &item.business_object_type, access)
    {
        actions.push(WorkItemAllowedAction::Process);
    }
    // 开放的单据审批任务由审批运行时直接指派给责任人（owner_user_id = 指派
    // 人，owner_role 为语义标签而非角色 ID，无法用责任范围校验）；最终授权由
    // /admin/approval-decisions 写时重验（账号启用 + approval_instance:decide +
    // 精确任务责任链）。
    if item.work_item_type.is_document_approval()
        && item.status == WorkItemStatus::Open
        && item.approval_node_execution_id.is_some()
        && item.owner_user_id.as_deref() == Some(actor_id)
        && has_permission(access, "approval_instance:decide")
    {
        actions.push(WorkItemAllowedAction::Approve);
        actions.push(WorkItemAllowedAction::Reject);
    }
    let is_approval_responsibility =
        item.work_item_type.is_document_approval() || item.approval_node_execution_id.is_some();
    if access.can_manage
        && covers_owner(access, item.owner_user_id.as_deref())
        && scope == WorkItemScope::Managed
        && !is_approval_responsibility
    {
        if has_permission(access, REASSIGN_PERMISSION) {
            actions.push(WorkItemAllowedAction::Reassign);
        }
        if is_w29_fields_closable(item) && has_permission(access, CLOSE_PERMISSION) {
            actions.push(WorkItemAllowedAction::Close);
        }
    }
    actions
}

/// 确认账号具备非空的任务责任管理范围。
///
/// # 参数
/// * `access` - 当前账号授权快照。
///
/// # 返回
/// 具备管理资格，且管理负责人不是空列表时无返回值。`managed_owner_ids` 为 `None` 表示不限定名单，仍算具备范围。
///
/// # 错误
/// 不能管理，或管理负责人列表为空时返回 `Error::Forbidden`。
pub(super) fn ensure_managed_access(access: &ActorAccess) -> Result<()> {
    if !access.can_manage || access.managed_owner_ids.as_ref().is_some_and(Vec::is_empty) {
        return Err(Error::Forbidden("当前账号没有任务责任管理范围".to_string()));
    }
    Ok(())
}

/// 判断当前账号查看该任务时应使用的队列范围。
///
/// # 参数
/// * `item` - 工作项。
/// * `actor_id` - 当前账号 ID。
/// * `access` - 授权快照。
///
/// # 返回
/// 终态且本人有历史参与或管理覆盖负责人时返回 `History`；本人负责返回 `Mine`；开放且管理覆盖负责人返回 `Managed`。
///
/// # 错误
/// 以上都不满足时返回 `Error::Forbidden`。
pub(super) fn detail_scope(item: &WorkItem, actor_id: &str, access: &ActorAccess) -> Result<WorkItemScope> {
    if item.is_terminal()
        && (has_personal_history_access(item, actor_id)
            || (access.can_manage && covers_owner(access, item.owner_user_id.as_deref())))
    {
        return Ok(WorkItemScope::History);
    }
    if item.is_owned_by(actor_id) {
        return Ok(WorkItemScope::Mine);
    }
    if item.status == WorkItemStatus::Open
        && access.can_manage
        && covers_owner(access, item.owner_user_id.as_deref())
    {
        return Ok(WorkItemScope::Managed);
    }
    Err(Error::Forbidden("当前账号无权查看该任务".to_string()))
}

/// 判断账号是否出现在该任务的历史责任或完结记录中。
///
/// # 参数
/// * `item` - 工作项。
/// * `actor_id` - 当前账号 ID。
///
/// # 返回
/// 责任人、完成人或关闭人包含该账号时返回 `true`。
///
/// # 错误
/// 不返回错误。
pub(super) fn has_personal_history_access(item: &WorkItem, actor_id: &str) -> bool {
    item.responsibility_actor_ids.iter().any(|id| id == actor_id)
        || item.completed_by.as_deref() == Some(actor_id)
        || item.closed_by.as_deref() == Some(actor_id)
}

/// 判断管理范围是否覆盖指定负责人。
///
/// # 参数
/// * `access` - 授权快照。
/// * `owner` - 任务负责人；无负责人时不能被名单覆盖。
///
/// # 返回
/// `managed_owner_ids` 为 `None` 时不限定名单，返回 `true`；否则负责人在名单中返回 `true`。
///
/// # 错误
/// 不返回错误。
pub(super) fn covers_owner(access: &ActorAccess, owner: Option<&str>) -> bool {
    access
        .managed_owner_ids
        .as_ref()
        .is_none_or(|ids| owner.is_some_and(|owner| ids.iter().any(|id| id == owner)))
}

/// 判断工作项投影是否属于 W29 可受控关闭关系。
///
/// # 参数
/// * `item` - 已授权的工作项投影字段
///
/// # 返回
/// 非审批的集成异常或对账差异任务返回 `true`。
///
/// # 错误
/// 无。
fn is_w29_fields_closable(item: &dto::WorkItemFields) -> bool {
    item.work_item_type.is_w29_closable(&item.business_object_type, item.approval_node_execution_id.is_some())
}

#[cfg(test)]
mod queue_access_tests {
    use std::sync::atomic::Ordering;

    use erp_core::AccountKind;
    use erp_workflow::FailClosedWorkflowAuthorizationPort;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::workbench::test_auth::TestAuth;

    /// 构造动作权、参与关系和管理负责人分别存在的只读身份事实。
    fn queue_facts(version: Option<&str>) -> WorkflowQueueAccessFact {
        WorkflowQueueAccessFact {
            permission_codes: vec!["work_item:read".into(), "work_item:manage".into()],
            participant_document_ids: vec!["document".into(), "document".into()],
            managed_owner_ids: Some(vec!["owner".into()]),
            can_manage: true,
            identity_version: version.map(str::to_string),
        }
    }

    /// 批量端口直接透传认证身份与首拍要求，成功事实不触发逐角色或独立范围读取。
    #[tokio::test]
    async fn optimized_access_transfers_identity_and_keeps_authorization_dimensions_separate() {
        let auth = TestAuth::default();
        *auth.queue.lock().unwrap() = Some(queue_facts(Some("identity-v1")));
        let actor = AuditActor::new("actor".to_string(), "login".to_string(), AccountKind::Admin);
        let mut executor = NoTransaction;
        let (access, version) = optimized_access(&auth, &actor, true, &mut executor).await.unwrap().unwrap();
        assert_eq!(access.actor_id, "actor");
        assert_eq!(version.as_deref(), Some("identity-v1"));
        assert_eq!(access.participant_document_ids, HashSet::from(["document".into()]));
        assert_eq!(access.managed_owner_ids, Some(vec!["owner".into()]));
        assert!(access.can_manage);
        assert!(has_permission(&access, "work_item:read"));
        assert!(!has_permission(&access, "sales_order:detail"));
        assert_eq!(*auth.trace.lock().unwrap(), vec!["queue:actor:true"]);
    }

    /// 未装配优化端口保留原路径；首拍缺失和端口错误不得回退成成功。
    #[tokio::test]
    async fn optimized_access_fallback_and_failures_preserve_explicit_boundaries() {
        let actor = AuditActor::new("actor".to_string(), "login".to_string(), AccountKind::Admin);
        let mut executor = NoTransaction;
        assert!(
            optimized_access(&FailClosedWorkflowAuthorizationPort, &actor, true, &mut executor)
                .await
                .unwrap()
                .is_none()
        );
        let auth = TestAuth::default();
        *auth.queue.lock().unwrap() = Some(queue_facts(None));
        assert!(
            matches!(optimized_access(&auth, &actor, true, &mut executor).await, Err(Error::Internal(message)) if message == "工作台首拍授权版本缺失")
        );
        assert!(optimized_access(&auth, &actor, false, &mut executor).await.unwrap().is_some());
        auth.fail_queue.store(true, Ordering::SeqCst);
        assert!(
            matches!(optimized_access(&auth, &actor, true, &mut executor).await, Err(Error::Rbac(message)) if message == "queue failed")
        );
    }
}

#[cfg(test)]
mod approval_admission_tests {
    use super::*;

    /// 仅有审批读取资格即可进入精确任务授权候选，不附带销售或采购权限。
    #[test]
    fn approval_candidates_use_approval_permission_without_granting_business_shapes() {
        let access = ActorAccess::new("approver".into())
            .with_permissions(vec![Permission::parse("approval_instance:read").unwrap()]);
        let shapes = object_access_shapes(&access);
        assert!(!shapes.is_empty());
        assert!(shapes.iter().all(|(kind, _)| kind.is_document_approval()));
        let ordinary = ActorAccess::new("sales".into())
            .with_permissions(vec![Permission::parse("sales_order:detail").unwrap()]);
        assert!(object_access_shapes(&ordinary).iter().all(|(kind, _)| !kind.is_document_approval()));
        assert!(object_access_shapes(&ActorAccess::new("none".into())).is_empty());
    }
}
