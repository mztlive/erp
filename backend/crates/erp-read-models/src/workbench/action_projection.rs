//! 统计与页面共用的最小任务动作事实，不装配名称、摘要或明细。

use std::collections::{HashMap, HashSet};

use bpm::ids::ApprovalNodeExecutionId;
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use erp_workflow::{BpmExt, WorkflowAuthorizationPort};
use persistence_core::Executor;

use super::access::ViewAccess;
use super::query::remove_approval_decision_actions;
use super::{WorkItemView, WorkbenchReadService, dto};
use crate::Result;

/// 资格重验仅消费任务身份、责任和动作状态。
pub(super) struct ActionProjection {
    pub work_item_type: WorkItemType,
    pub status: WorkItemStatus,
    pub business_object_type: String,
    pub business_object_id: String,
    pub owner_user_id: Option<String>,
    pub approval_node_execution_id: Option<String>,
    pub access: ViewAccess,
}

impl ActionProjection {
    /// 从已授权字段转移最小事实，并保留原页面路由注册校验。
    ///
    /// # 参数
    /// `fields` 为已授权身份，`access` 为正式动作判断。
    /// # 返回
    /// 返回资格重验所需的最小投影。
    /// # 错误
    /// 业务对象未注册页面路由时保持正式视图错误。
    pub(super) fn from_fields(fields: dto::WorkItemFields, access: ViewAccess) -> Result<Self> {
        dto::work_item_destination(fields.work_item_type, &fields.business_object_type, &fields.owner_role)?;
        Ok(Self {
            work_item_type: fields.work_item_type,
            status: fields.status,
            business_object_type: fields.business_object_type.clone(),
            business_object_id: fields.business_object_id.clone(),
            owner_user_id: fields.owner_user_id.clone(),
            approval_node_execution_id: fields.approval_node_execution_id.clone(),
            access,
        })
    }

    /// 页面仅复制资格所需的小字段，展示内容保留在原视图。
    ///
    /// # 参数
    /// `view` 为已授权页面视图。
    /// # 返回
    /// 返回与视图身份及动作一致的最小事实。
    /// # 错误
    /// 无。
    pub(super) fn from_view(view: &WorkItemView) -> Self {
        Self {
            work_item_type: view.work_item_type,
            status: view.status,
            business_object_type: view.business_object_type.clone(),
            business_object_id: view.business_object_id.clone(),
            owner_user_id: view.owner_user_id.clone(),
            approval_node_execution_id: view.approval_node_execution_id.clone(),
            access: ViewAccess {
                processing_state: view.processing_state,
                processing_blocker: view.processing_blocker.clone(),
                allowed_actions: view.allowed_actions.clone(),
                action_blockers: view.action_blockers.clone(),
            },
        }
    }

    /// 仅回填资格状态与动作，不覆盖责任、版本和业务展示。
    ///
    /// # 参数
    /// `view` 为同一任务的原始页面视图。
    /// # 返回
    /// 原地写入最终资格状态与动作。
    /// # 错误
    /// 无。
    pub(super) fn apply(self, view: &mut WorkItemView) {
        view.processing_state = self.access.processing_state;
        view.processing_blocker = self.access.processing_blocker;
        view.allowed_actions = self.access.allowed_actions;
        view.action_blockers = self.access.action_blockers;
    }
}

impl<A: WorkflowAuthorizationPort + Clone + Send + Sync + 'static> WorkbenchReadService<A> {
    /// 统计仅需要与正式页面相同的审批上下文存在性，不装配节点文案和驳回摘要。
    ///
    /// # 参数
    /// `items` 为本批已授权最小动作投影；`executor` 沿用候选读取快照。
    /// # 返回
    /// 精确执行或实例上下文缺失时移除审批决定动作。
    /// # 错误
    /// 批量节点或实例读取失败时保持原错误。
    pub(super) async fn apply_stat_approval_contexts(
        &self,
        items: &mut [ActionProjection],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let execution_ids = items
            .iter()
            .filter_map(|item| item.approval_node_execution_id.as_deref())
            .map(ApprovalNodeExecutionId::new)
            .collect::<Vec<_>>();
        if execution_ids.is_empty() {
            return Ok(());
        }
        let executions = self.db.bpm_workflow().list_executions_by_ids(&execution_ids, executor).await?;
        let instance_ids =
            executions.iter().map(|execution| execution.process_instance_id.clone()).collect::<Vec<_>>();
        let summaries =
            self.db.bpm_workflow().list_instance_summaries_by_ids(&instance_ids, executor).await?;
        let instances = summaries.into_iter().map(|summary| summary.id).collect::<HashSet<_>>();
        let executions = executions
            .into_iter()
            .map(|execution| (execution.base.id.clone(), execution.process_instance_id.to_string()))
            .collect::<HashMap<_, _>>();
        for item in items {
            retain_available_approval_actions(item, &executions, &instances);
        }
        Ok(())
    }
}

/// 只在任务的精确节点执行及其实例都存在时保留决定动作。
fn retain_available_approval_actions(
    item: &mut ActionProjection,
    executions: &HashMap<String, String>,
    instances: &HashSet<String>,
) {
    let Some(id) = item.approval_node_execution_id.as_deref() else {
        return;
    };
    if !executions.get(id).is_some_and(|instance| instances.contains(instance)) {
        remove_approval_decision_actions(&mut item.access.allowed_actions);
    }
}

#[cfg(test)]
mod tests {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItem, WorkItemData, WorkItemPriority};

    use super::*;
    use crate::Error;
    use crate::workbench::{ProcessingState, WorkItemAllowedAction};

    /// 构造一个正式注册的履约任务，复用实体和视图生产转换。
    fn fields() -> dto::WorkItemFields {
        WorkItem::new_with_responsibility_key(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::FulfillmentOperation,
                business_object_type: "purchase_receipt".into(),
                business_object_id: "receipt".into(),
                subject_version: "1".into(),
                owner_role: "warehouse_inbound_handler".into(),
                owner_organization_id: "warehouse".into(),
                owner_user_id: "actor".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "purchase_order:order",
        )
        .unwrap()
        .into()
    }

    /// 最小统计转换与正式页面保留同一身份和动作，回填不覆盖展示和版本。
    #[test]
    fn minimal_actions_preserve_view_identity_and_qualification_results() {
        let fields = fields();
        let access = ViewAccess::ready(vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Process]);
        let mut projection = ActionProjection::from_fields(fields.clone(), access).unwrap();
        let mut view = WorkItemView::from_fields(fields, "queue".into()).unwrap();
        let before = view.clone();
        projection.access.processing_state = ProcessingState::ExecutionBlocked;
        projection.access.allowed_actions = vec![WorkItemAllowedAction::View];
        projection.apply(&mut view);
        assert_eq!(view.id, before.id);
        assert_eq!(view.owner_user_id, before.owner_user_id);
        assert_eq!(view.owner_organization_id, before.owner_organization_id);
        assert_eq!(view.task_version, before.task_version);
        assert_eq!(view.subject_version, before.subject_version);
        assert_eq!(view.summary_sections, before.summary_sections);
        assert_eq!(view.processing_state, ProcessingState::ExecutionBlocked);
        assert_eq!(view.allowed_actions, vec![WorkItemAllowedAction::View]);
    }

    /// 不装配显示的统计仍执行正式路由校验，未注册审批对象继续失败关闭。
    #[test]
    fn minimal_actions_keep_unmapped_route_failure() {
        let mut fields = fields();
        fields.work_item_type = WorkItemType::DocumentApproval;
        fields.business_object_type = "unknown_approval_document".into();
        assert!(
            matches!(ActionProjection::from_fields(fields.clone(), ViewAccess::ready(Vec::new())), Err(Error::ValidationError(message)) if message == "APPROVAL_DOCUMENT_ROUTE_UNMAPPED")
        );
        assert!(
            matches!(WorkItemView::from_fields(fields, "queue".into()), Err(Error::ValidationError(message)) if message == "APPROVAL_DOCUMENT_ROUTE_UNMAPPED")
        );
    }

    /// 上下文缺失只移除审批决定动作，不把其他任务动作和处理状态一起抹掉。
    #[test]
    fn approval_decisions_require_exact_execution_and_existing_instance() {
        let mut item = ActionProjection::from_fields(
            fields(),
            ViewAccess::ready(vec![
                WorkItemAllowedAction::View,
                WorkItemAllowedAction::Approve,
                WorkItemAllowedAction::Reject,
                WorkItemAllowedAction::Process,
            ]),
        )
        .unwrap();
        item.approval_node_execution_id = Some("execution".into());
        let executions = HashMap::from([("execution".into(), "instance".into())]);
        let instances = HashSet::from(["instance".into()]);
        retain_available_approval_actions(&mut item, &executions, &instances);
        assert!(item.access.allowed_actions.contains(&WorkItemAllowedAction::Approve));
        retain_available_approval_actions(&mut item, &executions, &HashSet::from(["other-instance".into()]));
        assert_eq!(
            item.access.allowed_actions,
            vec![WorkItemAllowedAction::View, WorkItemAllowedAction::Process]
        );
        assert_eq!(item.access.processing_state, ProcessingState::Ready);
        item.access.allowed_actions.push(WorkItemAllowedAction::Approve);
        item.approval_node_execution_id = Some("different-execution".into());
        retain_available_approval_actions(&mut item, &executions, &instances);
        assert!(!item.access.allowed_actions.contains(&WorkItemAllowedAction::Approve));
    }
}
