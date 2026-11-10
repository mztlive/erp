//! 责任队列授权快照、范围过滤与允许动作。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{CompiledAccess, Permission, PermissionSet};
use persistence_core::Executor;

use super::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemFilter, WorkItemReadService,
    WorkItemScope, dto,
};
use crate::entity::work_item::{
    WorkItem, WorkItemActionAccess, WorkItemActionRequirement, WorkItemStatus, WorkItemType,
};
use crate::error::{Error, Result};
use crate::service::work_item::access::required_execution_permissions;

pub const MANAGE_PERMISSION: &str = "work_item:manage";

#[derive(Clone)]
pub struct ActorAccess {
    pub actor_id: String,
    pub permissions: Vec<Permission>,
    pub(super) manage: Option<Arc<CompiledAccess>>,
    pub(super) managed_task_ids: BTreeSet<String>,
    pub can_manage: bool,
    pub read_action: &'static str,
    pub(super) read: Option<Arc<CompiledAccess>>,
}

impl ActorAccess {
    /// 把列表查询和授权快照收成仓储过滤条件。
    ///
    /// # 参数
    /// * `query` - 工作项列表查询。
    ///
    /// # 返回
    /// 所有视图先应用 list 授权；Mine 和 History 只追加本人及历史筛选。
    ///
    /// # 错误
    /// 缺少列表权限时返回 `Error::Forbidden`。
    pub(super) fn scope_filter(&self, query: &dto::WorkItemListQuery) -> Result<WorkItemFilter> {
        if !has_permission(self, "work_item:list") {
            return Err(Error::Forbidden("没有待办列表权限".into()));
        }
        let mut filter = WorkItemFilter {
            sales_order_ids: query.sales_order_ids.clone(),
            purchase_order_ids: query.purchase_order_ids.clone(),
            work_item_types: query.work_item_types.clone(),
            statuses: query.statuses.clone(),
            priorities: query.priorities.clone(),
            query: query.query.clone(),
            authorization: Some(self.read_access()?.clone()),
            page: query.page,
            page_size: query.page_size,
            sort_by: Some(query.sort_by.to_string()),
            sort_ascending: query.sort_ascending,
            ..WorkItemFilter::default()
        };
        match query.scope {
            WorkItemScope::Mine => filter.owner_user_id = Some(self.actor_id.clone()),
            WorkItemScope::Managed => {},
            WorkItemScope::History => filter.history_actor_id = Some(self.actor_id.clone()),
        }
        Ok(filter)
    }

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
    pub fn new(actor_id: String) -> Self {
        Self {
            actor_id,
            permissions: Vec::new(),
            manage: None,
            managed_task_ids: BTreeSet::new(),
            can_manage: false,
            read: None,
            read_action: "detail",
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
    pub fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
        self
    }

    /// 取得已编译的任务读取约束。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前读取动作绑定的完整授权结果。
    /// # 错误
    /// 未经授权构造的上下文拒绝查询。
    pub fn read_access(&self) -> Result<&Arc<CompiledAccess>> {
        self.read.as_ref().ok_or_else(|| Error::Forbidden("缺少任务授权查询".into()))
    }

    /// 读取包含管理动作依赖的队列版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前读取和可用管理授权的完整版本。
    /// # 错误
    /// 缺少读取授权时返回错误。
    pub fn version(&self) -> Result<String> {
        Ok(format!(
            "{}:{}",
            self.read_access()?.version(),
            self.manage.as_ref().map(|value| value.version()).unwrap_or_default()
        ))
    }
}

impl WorkItemReadService {
    /// 分别编译读取和管理动作的授权条件，操作权限缺失不影响读取。
    /// # 参数
    /// `actor` 为真实账号，`action` 为当前读取动作，`executor` 为同一执行器。
    /// # 返回
    /// 返回相互独立的读取范围和管理范围。
    /// # 错误
    /// 当前读取动作拒绝或基础设施故障时失败关闭。
    pub async fn task_access(
        &self,
        actor: &AuditActor,
        action: &'static str,
        executor: &mut dyn Executor,
    ) -> Result<ActorAccess> {
        let read = self.task_query(actor, action, executor).await?;
        let mut access = ActorAccess::new(actor.id().to_owned());
        access.permissions = self.task_permissions(actor, executor).await?;
        access.read = Some(read);
        access.read_action = action;
        if has_permission(&access, MANAGE_PERMISSION) {
            access.manage = Some(self.task_query(actor, "manage", executor).await?);
            access.can_manage = true;
        }
        Ok(access)
    }

    /// 首末授权版本保护跨页和当前请求的数据范围一致性。
    /// # 参数
    /// `actor` 为当前账号，`executor` 为读取执行器。
    /// # 返回
    /// 列表访问范围及身份版本。
    /// # 错误
    /// 权限、范围或版本读取失败时返回错误。
    pub async fn queue_access_with_version(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(ActorAccess, String)> {
        let access = self.task_access(actor, "list", executor).await?;
        let version = access.version()?;
        Ok((access, version))
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

pub fn has_permission(access: &ActorAccess, permission: &str) -> bool {
    let required = Permission::parse(permission).expect("对象注册表权限必须合法");
    PermissionSet::new(access.permissions.clone()).covers_one(&required)
}

pub struct ViewAccess {
    pub processing_state: ProcessingState,
    pub processing_blocker: Option<ProcessingBlockerView>,
    pub allowed_actions: Vec<WorkItemAllowedAction>,
    pub action_blockers: Vec<ProcessingBlockerView>,
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
    pub fn ready(allowed_actions: Vec<WorkItemAllowedAction>) -> Self {
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
    pub fn blocked(blocker: ProcessingBlockerView) -> Self {
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
    let can_control = has_permission(access, MANAGE_PERMISSION)
        && access.can_manage
        && access.managed_task_ids.contains(&item.id);
    let mut actions = item.action_policy().allowed_actions(
        WorkItemActionAccess {
            actor_id,
            managed_queue: scope != WorkItemScope::History,
            can_manage: can_control,
            managed_owner_ids: None,
        },
        |requirement| match requirement {
            WorkItemActionRequirement::Execute => {
                required_execution_permissions(item.work_item_type, &item.business_object_type)
                    .is_some_and(|codes| codes.iter().all(|code| has_permission(access, code)))
            },
            WorkItemActionRequirement::Decide => has_permission(access, "approval_instance:decide"),
            WorkItemActionRequirement::Reassign | WorkItemActionRequirement::Close => can_control,
        },
    );
    actions.retain(|action| *action != WorkItemAllowedAction::AbandonRework);
    if item.work_item_type == WorkItemType::ApprovalRework
        && item.status == WorkItemStatus::Open
        && item.owner_user_id.as_deref() == Some(actor_id)
        && can_control
    {
        actions.push(WorkItemAllowedAction::AbandonRework);
    }
    actions
}

/// 为已经通过对象授权的任务选择展示范围，不执行对象授权。
///
/// # 参数
/// * `item` - 工作项。
/// * `actor_id` - 当前账号 ID。
/// * `access` - 授权快照。
///
/// # 返回
/// 调用方通过当前任务 require 后，终态返回 `History`，本人负责返回 `Mine`，其他返回 `Managed`。
///
/// # 错误
/// 缺少当前动作权限时返回 `Error::Forbidden`。
pub(super) fn detail_scope(item: &WorkItem, actor_id: &str, access: &ActorAccess) -> Result<WorkItemScope> {
    if !has_permission(access, &format!("work_item:{}", access.read_action)) {
        return Err(Error::Forbidden("当前账号无权查看该任务".into()));
    }
    Ok(if item.is_terminal() {
        WorkItemScope::History
    } else if item.is_owned_by(actor_id) {
        WorkItemScope::Mine
    } else {
        WorkItemScope::Managed
    })
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority};

    fn item() -> WorkItem {
        WorkItem::new(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".into(),
                business_object_id: "account".into(),
                subject_version: "v1".into(),
                owner_role: "finance".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "reader".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn function_permission_alone_cannot_generate_a_list_filter() {
        let access = ActorAccess::new("reader".into())
            .with_permissions(vec![Permission::parse("work_item:list").unwrap()]);
        for scope in ["mine", "managed", "history"] {
            let params: dto::WorkItemListParams =
                serde_json::from_value(serde_json::json!({"scope": scope})).unwrap();
            assert!(access.scope_filter(&params.normalized().unwrap()).is_err());
        }
        assert!(access.read_access().is_err());
        assert!(access.version().is_err());
    }

    #[test]
    fn management_actions_require_authorization_for_the_exact_task() {
        let fields = dto::WorkItemFields::from(item());
        let mut access = ActorAccess::new("reader".into())
            .with_permissions(vec![Permission::parse(MANAGE_PERMISSION).unwrap()]);
        access.can_manage = true;
        access.managed_task_ids.insert("other-task".into());
        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "reader", &access)
                .contains(&WorkItemAllowedAction::Reassign)
        );
        access.managed_task_ids.insert(fields.id.clone());
        assert!(
            allowed_actions(&fields, WorkItemScope::Mine, "reader", &access)
                .contains(&WorkItemAllowedAction::Reassign)
        );
        access.permissions.clear();
        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "reader", &access)
                .contains(&WorkItemAllowedAction::Reassign)
        );
    }

    #[test]
    fn authorized_detail_and_manage_results_keep_separate_entry_permissions() {
        let task = item();
        let mut access = ActorAccess::new("reader".into())
            .with_permissions(vec![Permission::parse("work_item:manage").unwrap()]);
        assert!(detail_scope(&task, "reader", &access).is_err());
        access.read_action = "manage";
        assert_eq!(detail_scope(&task, "reader", &access).unwrap(), WorkItemScope::Mine);
        let mut completed = task;
        completed.status = WorkItemStatus::Completed;
        assert_eq!(detail_scope(&completed, "reader", &access).unwrap(), WorkItemScope::History);
    }
}
