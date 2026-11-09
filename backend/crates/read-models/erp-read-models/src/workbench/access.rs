//! 责任队列授权快照、范围过滤与允许动作。

use application_core::AuditActor;
use erp_identity::{Permission, PermissionSet};
use erp_workflow::entity::work_item::{
    WorkItem, WorkItemActionAccess, WorkItemActionRequirement, WorkItemStatus, WorkItemType,
};
use erp_workflow::service::work_item::access::required_execution_permissions;
use persistence_core::Executor;

use super::{
    ProcessingBlockerView, ProcessingState, WorkItemAllowedAction, WorkItemFilter, WorkItemScope,
    WorkbenchReadService, dto,
};
use crate::errors::{Error, Result};

pub(super) const MANAGE_PERMISSION: &str = "work_item:manage";

pub(super) struct ActorAccess {
    pub(super) actor_id: String,
    pub(super) permissions: Vec<Permission>,
    pub(super) managed_owner_ids: Option<Vec<String>>,
    pub(super) can_manage: bool,
    pub(super) read_action: &'static str,
    pub(super) readable_owner_ids: Option<Vec<String>>,
}

impl ActorAccess {
    /// 把列表查询和授权快照收成仓储过滤条件。
    ///
    /// # 参数
    /// * `query` - 工作项列表查询。
    ///
    /// # 返回
    /// 所有视图先应用 list DataScope；Mine 和 History 只追加本人及历史筛选。
    ///
    /// # 错误
    /// 缺少列表权限时返回 `Error::Forbidden`。
    pub(super) fn scope_filter(&self, query: &dto::WorkItemListQuery) -> Result<WorkItemFilter> {
        if !has_permission(self, "work_item:list") {
            return Err(Error::Forbidden("没有待办列表权限".into()));
        }
        let mut filter = WorkItemFilter {
            work_item_types: query.work_item_types.clone(),
            statuses: query.statuses.clone(),
            priorities: query.priorities.clone(),
            query: query.query.clone(),
            managed_owner_ids: self.readable_owner_ids.clone(),
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
    pub(super) fn new(actor_id: String) -> Self {
        Self {
            actor_id,
            permissions: Vec::new(),
            managed_owner_ids: None,
            can_manage: false,
            readable_owner_ids: Some(Vec::new()),
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
    pub(super) fn with_permissions(mut self, permissions: Vec<Permission>) -> Self {
        self.permissions = permissions;
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
    #[cfg(test)]
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

impl WorkbenchReadService {
    /// 分别解析读取和操作的 DataScope，操作权限缺失不影响读取。
    /// # 参数
    /// `actor` 为真实账号，`action` 为当前读取动作，`executor` 为同一执行器。
    /// # 返回
    /// 返回相互独立的读取范围和管理范围。
    /// # 错误
    /// 当前读取动作拒绝或基础设施故障时失败关闭。
    pub(super) async fn task_access(
        &self,
        actor: &AuditActor,
        action: &'static str,
        executor: &mut dyn Executor,
    ) -> Result<ActorAccess> {
        let readable_owner_ids = self.auth.task_scope_owners(actor, action, executor).await?;
        let permissions = self
            .auth
            .permission_codes(actor.kind(), actor.id())
            .await?
            .into_iter()
            .map(|code| Permission::parse(&code).map_err(|error| Error::Internal(error.to_string())))
            .collect::<Result<Vec<_>>>()?;
        let mut access = ActorAccess::new(actor.id().to_owned()).with_permissions(permissions);
        access.readable_owner_ids = readable_owner_ids;
        access.read_action = action;
        if has_permission(&access, MANAGE_PERMISSION) {
            match self.auth.task_scope_owners(actor, "manage", executor).await {
                Ok(owners) => {
                    access.managed_owner_ids = owners;
                    access.can_manage = true;
                },
                Err(erp_workflow::Error::Forbidden(_)) => {},
                Err(error) => return Err(error.into()),
            }
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
    pub(super) async fn queue_access_with_version(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(ActorAccess, String)> {
        let version = self.auth.queue_scope_version(actor, executor).await?;
        let access = self.task_access(actor, "list", executor).await?;
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

pub(super) fn has_permission(access: &ActorAccess, permission: &str) -> bool {
    let required = Permission::parse(permission).expect("对象注册表权限必须合法");
    PermissionSet::new(access.permissions.clone()).covers_one(&required)
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
    let can_control = has_permission(access, MANAGE_PERMISSION)
        && access.can_manage
        && covers_owner(access, item.owner_user_id.as_deref());
    let mut actions = item.action_policy().allowed_actions(
        WorkItemActionAccess {
            actor_id,
            managed_queue: scope != WorkItemScope::History,
            can_manage: can_control,
            managed_owner_ids: access.managed_owner_ids.as_deref(),
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

/// 判断当前账号查看该任务时应使用的队列范围。
///
/// # 参数
/// * `item` - 工作项。
/// * `actor_id` - 当前账号 ID。
/// * `access` - 授权快照。
///
/// # 返回
/// 通过当前动作 DataScope 后，终态返回 `History`，本人负责返回 `Mine`，其他返回 `Managed`。
///
/// # 错误
/// 当前动作权限或 DataScope 不覆盖任务时返回 `Error::Forbidden`。
pub(super) fn detail_scope(item: &WorkItem, actor_id: &str, access: &ActorAccess) -> Result<WorkItemScope> {
    if !has_permission(access, &format!("work_item:{}", access.read_action))
        || !access
            .readable_owner_ids
            .as_ref()
            .is_none_or(|ids| item.owner_user_id.as_ref().is_some_and(|owner| ids.contains(owner)))
    {
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

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority, WorkItemType};
    use persistence_core::QueryFilter;

    use super::*;

    fn item() -> WorkItem {
        WorkItem::new(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::CardFundsReview,
                business_object_type: "receivable_account".into(),
                business_object_id: "account".into(),
                subject_version: "version-7".into(),
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

    fn access(permission: &str, owners: Option<Vec<String>>) -> ActorAccess {
        let mut access =
            ActorAccess::new("reader".into()).with_permissions(vec![Permission::parse(permission).unwrap()]);
        access.readable_owner_ids = owners;
        access
    }

    #[test]
    fn every_list_view_intersects_datascope_without_business_or_manage_permissions() {
        let access = access("work_item:list", Some(vec!["reader".into()]));
        for scope in ["mine", "managed", "history"] {
            let params: dto::WorkItemListParams =
                serde_json::from_value(serde_json::json!({"scope": scope})).unwrap();
            let filter = access.scope_filter(&params.normalized().unwrap()).unwrap();
            assert_eq!(filter.managed_owner_ids, Some(vec!["reader".into()]));
            assert!(filter.object_access_shapes.is_none());
            let document = filter.to_doc();
            assert!(
                document.get_array("$and").unwrap().iter().any(|value| {
                    value.as_document().is_some_and(|part| part.contains_key("owner_user_id"))
                })
            );
        }
    }

    #[test]
    fn missing_list_permission_or_empty_scope_never_grants_personal_tasks() {
        let params: dto::WorkItemListParams =
            serde_json::from_value(serde_json::json!({"scope":"mine"})).unwrap();
        let query = params.normalized().unwrap();
        assert!(access("sales_order:detail", None).scope_filter(&query).is_err());
        let filter = access("work_item:list", Some(vec![])).scope_filter(&query).unwrap();
        assert_eq!(filter.managed_owner_ids, Some(vec![]));
        assert_eq!(filter.owner_user_id.as_deref(), Some("reader"));
    }

    #[test]
    fn detail_requires_its_own_scope_even_for_owner_or_historical_actor() {
        let mut item = item();
        let access = access("work_item:detail", Some(vec!["reader".into()]));
        assert_eq!(detail_scope(&item, "reader", &access).unwrap(), WorkItemScope::Mine);
        item.owner_user_id = Some("other".into());
        item.responsibility_actor_ids.push("reader".into());
        assert!(detail_scope(&item, "reader", &access).is_err());
        item.status = WorkItemStatus::Completed;
        item.completed_by = Some("reader".into());
        assert!(detail_scope(&item, "reader", &access).is_err());
        assert!(detail_scope(&item, "reader", &self::access("work_item:detail", None)).is_ok());
        assert!(detail_scope(&item, "reader", &self::access("work_item:list", None)).is_err());
    }

    #[test]
    fn actions_require_manage_scope_independently_of_read_and_business_permissions() {
        let fields = dto::WorkItemFields::from(item());
        let mut access = access("work_item:manage", None);
        access.can_manage = true;
        access.managed_owner_ids = Some(vec!["reader".into()]);
        assert!(
            allowed_actions(&fields, WorkItemScope::Mine, "reader", &access)
                .contains(&WorkItemAllowedAction::Reassign)
        );
        access.managed_owner_ids = Some(vec![]);
        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "reader", &access)
                .contains(&WorkItemAllowedAction::Reassign)
        );
        access.managed_owner_ids = None;
        access.permissions = vec![Permission::parse("work_item:detail").unwrap()];
        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "reader", &access)
                .contains(&WorkItemAllowedAction::Reassign)
        );
    }

    #[test]
    fn mutation_result_does_not_require_detail_or_list() {
        let mut access = access("work_item:manage", Some(vec!["reader".into()]));
        access.read_action = "manage";
        assert!(detail_scope(&item(), "reader", &access).is_ok());
        access.read_action = "detail";
        assert!(detail_scope(&item(), "reader", &access).is_err());
    }
    /// 管理权限不能代替业务执行权限，放弃处理也不依赖原单的业务权限。
    #[test]
    fn manage_does_not_grant_business_execution_but_allows_abandonment() {
        let mut task = item();
        task.work_item_type = WorkItemType::ApprovalRework;
        task.business_object_type = "sales_order".into();
        let fields = dto::WorkItemFields::from(task);
        let mut access = access("work_item:manage", None);
        access.can_manage = true;
        let actions = allowed_actions(&fields, WorkItemScope::Mine, "reader", &access);
        assert!(actions.contains(&WorkItemAllowedAction::AbandonRework));
        assert!(!actions.contains(&WorkItemAllowedAction::Process));
        access.managed_owner_ids = Some(vec![]);
        assert!(
            !allowed_actions(&fields, WorkItemScope::Mine, "reader", &access)
                .contains(&WorkItemAllowedAction::AbandonRework)
        );
    }

    /// 即使内部投影被附加资料，任务线协议仍只输出关联身份与版本。
    #[test]
    fn task_wire_projection_never_serializes_business_materials() {
        let mut task = item();
        task.work_item_type = WorkItemType::ApprovalRework;
        task.business_object_type = "sales_order".into();
        let mut view = super::super::WorkItemView::from_fields(task.into(), "queue".into()).unwrap();
        view.business_object_label = "secret document".into();
        view.counterparty_label = Some("secret customer".into());
        view.list_summary = Some("secret amount".into());
        view.summary_sections
            .push(super::super::dto::WorkItemSummarySection::new("amount".into(), "100".into()));
        let json = serde_json::to_value(view).unwrap();
        assert_eq!(json["business_object_id"], "account");
        assert_eq!(json["subject_version"], "version-7");
        for key in [
            "business_object_label",
            "counterparty_label",
            "summary_sections",
            "brief_lines",
            "delivery_lines",
            "fulfillment_status",
            "brief_more_count",
            "list_summary",
        ] {
            assert!(json.get(key).is_none(), "unexpected material: {key}");
        }
    }
}
