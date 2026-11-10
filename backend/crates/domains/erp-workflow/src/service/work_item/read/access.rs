//! 责任队列授权快照、范围过滤与允许动作。

use std::sync::Arc;

use application_core::AuditActor;
#[cfg(test)]
use erp_identity::Permission;
use erp_identity::{CompiledAccess, PermissionSet};
use persistence_core::Executor;

#[cfg(test)]
use super::WorkItemAllowedAction;
use super::{WorkItemFilter, WorkItemReadService, WorkItemScope, dto};
use crate::entity::work_item::{TaskAccess, WorkItem};
#[cfg(test)]
use crate::entity::work_item::{WorkItemStatus, WorkItemType};
use crate::error::{Error, Result};

pub const MANAGE_PERMISSION: &str = "work_item:manage";

/// 队列读取凭证与当前展示批次的动作事实；不能代替业务办理资格。
#[derive(Clone)]
pub struct QueueAccess {
    pub task: TaskAccess,
    pub(super) manage: Option<Arc<CompiledAccess>>,
    pub read_action: &'static str,
    pub(super) read: Option<Arc<CompiledAccess>>,
}

impl QueueAccess {
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
        if !self.task.permits("work_item:list") {
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
            WorkItemScope::Mine => filter.owner_user_id = Some(self.task.actor_id.clone()),
            WorkItemScope::Managed => {},
            WorkItemScope::History => filter.history_actor_id = Some(self.task.actor_id.clone()),
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
        Self { task: TaskAccess::new(actor_id), manage: None, read: None, read_action: "detail" }
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
    pub fn with_permissions(mut self, permissions: PermissionSet) -> Self {
        self.task.permissions = permissions;
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
    ) -> Result<QueueAccess> {
        let read = self.authorization.query(actor, action, executor).await?;
        let mut access = QueueAccess::new(actor.id().to_owned());
        access.task.permissions = self.task_permissions(actor, executor).await?;
        access.read = Some(read);
        access.read_action = action;
        if access.task.permits(MANAGE_PERMISSION) {
            access.manage = Some(self.authorization.query(actor, "manage", executor).await?);
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
    ) -> Result<(QueueAccess, String)> {
        let access = self.task_access(actor, "list", executor).await?;
        let version = access.version()?;
        Ok((access, version))
    }
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
pub(super) fn detail_scope(item: &WorkItem, actor_id: &str, access: &QueueAccess) -> Result<WorkItemScope> {
    if !access.task.permits(&format!("work_item:{}", access.read_action)) {
        return Err(Error::Forbidden("当前账号无权查看该任务".into()));
    }
    Ok(WorkItemScope::for_task(item, actor_id))
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
        let access = QueueAccess::new("reader".into())
            .with_permissions(PermissionSet::new([Permission::parse("work_item:list").unwrap()]));
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
        let mut access = QueueAccess::new("reader".into())
            .with_permissions(PermissionSet::new([Permission::parse(MANAGE_PERMISSION).unwrap()]));
        access.task.managed_task_ids.insert("other-task".into());
        assert!(
            !fields
                .allowed_actions(WorkItemScope::Mine, "reader", &access.task)
                .contains(&WorkItemAllowedAction::Reassign)
        );
        access.task.managed_task_ids.insert(fields.id.clone());
        assert!(
            fields
                .allowed_actions(WorkItemScope::Mine, "reader", &access.task)
                .contains(&WorkItemAllowedAction::Reassign)
        );
        access.task.permissions = PermissionSet::default();
        assert!(
            !fields
                .allowed_actions(WorkItemScope::Mine, "reader", &access.task)
                .contains(&WorkItemAllowedAction::Reassign)
        );
    }

    #[test]
    fn authorized_detail_and_manage_results_keep_separate_entry_permissions() {
        let task = item();
        let mut access = QueueAccess::new("reader".into())
            .with_permissions(PermissionSet::new([Permission::parse("work_item:manage").unwrap()]));
        assert!(detail_scope(&task, "reader", &access).is_err());
        access.read_action = "manage";
        assert_eq!(detail_scope(&task, "reader", &access).unwrap(), WorkItemScope::Mine);
        let mut completed = task;
        completed.status = WorkItemStatus::Completed;
        assert_eq!(detail_scope(&completed, "reader", &access).unwrap(), WorkItemScope::History);
    }
}
