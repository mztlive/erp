//! 责任队列授权快照、范围过滤与允许动作。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_identity::{Permission, PermissionSet};
use erp_workflow::DocumentRegistryExt;
use erp_workflow::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
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

impl<A: erp_workflow::WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 按账号类型与稳定 ID 构造责任队列授权快照。
    ///
    /// # 参数
    /// * `account_kind` - 账号类型
    /// * `actor_id` - 账号稳定 ID
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
        Ok(ActorAccess {
            actor_id: actor_id.to_string(),
            permissions,
            participant_document_ids,
            can_manage: !manage_role_ids.is_empty(),
            managed_owner_ids,
        })
    }

    /// 定位实际授予指定权限的角色，使管理数据范围与权限来源关联。
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

/// 根据实体注册关系和当前权限形成仓储候选对象形状。
///
/// # 参数
/// * `access` - 当前账号的权限与参与范围事实
///
/// # 返回
/// 返回当前账号具备读取权限的工作项类型和业务对象类型组合。
///
/// # 错误
/// 无；注册关系中的固定权限必须可解析。
pub(super) fn object_access_shapes(access: &ActorAccess) -> Vec<(WorkItemType, String)> {
    WorkItemType::registered_brief_relations()
        .iter()
        .filter(|policy| has_permission(access, policy.read_permission))
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
/// 无；固定权限代码无效属于程序错误并触发断言。
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
    pub(super) fn ready(allowed_actions: Vec<WorkItemAllowedAction>) -> Self {
        Self {
            processing_state: ProcessingState::Ready,
            processing_blocker: None,
            allowed_actions,
            action_blockers: Vec::new(),
        }
    }

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
    // 单据读权）。
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

pub(super) fn ensure_managed_access(access: &ActorAccess) -> Result<()> {
    if !access.can_manage || access.managed_owner_ids.as_ref().is_some_and(Vec::is_empty) {
        return Err(Error::Forbidden("当前账号没有任务责任管理范围".to_string()));
    }
    Ok(())
}

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

pub(super) fn has_personal_history_access(item: &WorkItem, actor_id: &str) -> bool {
    item.responsibility_actor_ids.iter().any(|id| id == actor_id)
        || item.completed_by.as_deref() == Some(actor_id)
        || item.closed_by.as_deref() == Some(actor_id)
}

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
