use serde::Serialize;

use crate::entity::work_item::{AssignmentSource, WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType};

/// 用户或组织安全摘要。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemPartyView {
    /// 稳定身份。
    pub id: String,
    /// 权限安全的展示名。
    pub display_name: String,
}

/// 非审批任务转交的合格具体账号。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemReassignCandidateView {
    /// 账号稳定 ID。
    pub user_id: String,
    /// 账号显示名称。
    pub display_name: String,
    /// 用户可识别的登录账号。
    pub account: String,
}

/// 工作台审批任务的有界运行上下文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WorkItemApprovalContextView {
    /// 审批运行实例主键；只作运行查询键，不承担单据识别。
    pub instance_id: String,
    /// 审批运行状态。
    pub status: String,
    /// 当前审批轮次。
    pub current_round_no: u32,
    /// 当前节点冻结名称。
    pub current_node_label: String,
    /// 当前审批人冻结名称。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub current_assignee_label: Option<String>,
    /// 最近一次驳回原因摘要。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latest_rejection_reason: Option<String>,
    /// 当前实例绑定的流程定义业务版本。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub process_version: Option<u32>,
}

#[derive(Debug, Clone)]
pub struct WorkItemFields {
    pub id: String,
    pub work_item_type: WorkItemType,
    pub approval_node_execution_id: Option<String>,
    pub business_object_type: String,
    pub business_object_id: String,
    pub root_business_object_id: String,
    pub business_object_label: String,
    pub counterparty_label: Option<String>,
    pub subject_version: String,
    pub status: WorkItemStatus,
    pub owner_role: String,
    pub owner_organization_id: String,
    pub owner_user_id: Option<String>,
    pub assignment_source: AssignmentSource,
    pub assigned_at: Option<erp_core::common::time::Instant>,
    pub started_at: Option<erp_core::common::time::Instant>,
    pub current_assignment_at: Option<erp_core::common::time::Instant>,
    pub last_activity_at: Option<erp_core::common::time::Instant>,
    pub priority: WorkItemPriority,
    pub due_at: Option<erp_core::common::time::Instant>,
    pub reason_code: Option<String>,
    pub impact_summary: Option<String>,
    pub completed_at: Option<erp_core::common::time::Instant>,
    pub completed_by: Option<String>,
    pub closed_at: Option<erp_core::common::time::Instant>,
    pub closed_by: Option<String>,
    pub close_reason: Option<String>,
    pub task_version: u64,
    pub created_at: u64,
}

impl From<WorkItem> for WorkItemFields {
    fn from(item: WorkItem) -> Self {
        let root_business_object_id = item.business_object_id.clone();
        Self {
            id: item.base.id,
            work_item_type: item.work_item_type,
            approval_node_execution_id: item
                .approval_node_execution_id
                .as_ref()
                .map(|id| id.as_ref().to_string()),
            business_object_type: item.business_object_type,
            business_object_id: item.business_object_id,
            root_business_object_id,
            business_object_label: item.work_item_type.label().to_string(),
            counterparty_label: None,
            subject_version: item.subject_version,
            status: item.status,
            owner_role: item.owner_role,
            owner_organization_id: item.owner_organization_id,
            owner_user_id: item.owner_user_id,
            assignment_source: item.assignment_source,
            assigned_at: item.assigned_at,
            started_at: item.started_at,
            current_assignment_at: item.current_assignment_at,
            last_activity_at: item.last_activity_at,
            priority: item.priority,
            due_at: item.due_at,
            reason_code: item.reason_code,
            impact_summary: item.impact_summary,
            completed_at: item.completed_at,
            completed_by: item.completed_by,
            closed_at: item.closed_at,
            closed_by: item.closed_by,
            close_reason: item.close_reason,
            task_version: item.base.version,
            created_at: item.base.created_at,
        }
    }
}

impl From<crate::repository::WorkItemRow> for WorkItemFields {
    fn from(item: crate::repository::WorkItemRow) -> Self {
        let root_business_object_id = item.business_object_id.clone();
        Self {
            id: item.id,
            work_item_type: item.work_item_type,
            approval_node_execution_id: item.approval_node_execution_id,
            business_object_type: item.business_object_type,
            business_object_id: item.business_object_id,
            root_business_object_id,
            business_object_label: item.work_item_type.label().to_string(),
            counterparty_label: None,
            subject_version: item.subject_version,
            status: item.status,
            owner_role: item.owner_role,
            owner_organization_id: item.owner_organization_id,
            owner_user_id: item.owner_user_id,
            assignment_source: item.assignment_source,
            assigned_at: item.assigned_at,
            started_at: item.started_at,
            current_assignment_at: item.current_assignment_at,
            last_activity_at: item.last_activity_at,
            priority: item.priority,
            due_at: item.due_at,
            reason_code: item.reason_code,
            completed_at: item.completed_at,
            completed_by: item.completed_by,
            closed_at: item.closed_at,
            closed_by: item.closed_by,
            close_reason: item.close_reason,
            impact_summary: item.impact_summary,
            task_version: item.version,
            created_at: item.created_at,
        }
    }
}
