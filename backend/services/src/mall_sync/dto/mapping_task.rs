use entities::common::time::Instant;
use entities::ids::MallSalesOrderSnapshotId;
use entities::mall_sync::{
    MallSnapshotReapplyOperation, MappingTaskStatus, MappingTaskType, MasterMappingTask,
    ReapplyOperationStatus,
};
use entities::source_registry::{MallSyncStage, RelationRole};
use entities::work_item::{AssignmentMode, WorkItemStatus, WorkItemType};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 映射任务列表允许的排序字段白名单。
pub(crate) const MASTER_MAPPING_TASK_SORT_FIELDS: &[&str] = &["created_at", "resolved_at"];

/// 映射任务创建请求。
///
/// 责任角色和用户不属于客户端输入；Service 必须按映射类型解析唯一责任路由。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateMasterMappingTaskRequest {
    /// 待处理快照。
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
}

/// 映射任务响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MasterMappingTaskView {
    /// 实体主键。
    pub id: String,
    /// 待处理快照。
    pub source_snapshot_id: String,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 任务状态。
    pub status: MappingTaskStatus,
    /// 业务责任角色；路由未配置时为空。
    pub owner_role: Option<String>,
    /// 业务责任用户 ID。
    pub owner_user_id: Option<String>,
    /// 处理结论。
    pub resolution: Option<String>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
    /// 责任路由强判别状态。
    pub owner_routing_state: OwnerRoutingState,
    /// 当前正式任务；路由缺失时必须为空。
    pub work_item: Option<MappingTaskWorkItemView>,
    /// 当前角色可见的来源白名单证据。
    pub source_evidence: Vec<MappingSourceEvidenceView>,
    /// 当前角色有权确认的 ERP 规范候选。
    pub candidate_targets: Vec<MappingCandidateTargetView>,
    /// 当前来源身份的完整目标谱系。
    pub current_targets: Vec<MappingCurrentTargetView>,
    /// 当前谱系身份；尚未建立时为空。
    pub external_identity_map_id: Option<String>,
    /// 业务影响摘要。
    pub impact_summary: String,
    /// 不可变处理历史。
    pub resolution_history: Vec<MappingResolutionHistoryView>,
    /// 当前 actor 的领域动作白名单。
    pub allowed_actions: Vec<String>,
    /// 当前 actor 的动作阻断原因。
    pub action_blockers: Vec<MappingActionBlockerView>,
    /// 最近一次独立重新归集操作。
    pub reapply_operation: Option<ReapplyOperationView>,
    /// 领域对象乐观锁版本。
    pub lock_version: u64,
}

/// 映射责任路由状态。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OwnerRoutingState {
    /// 映射类型尚无唯一责任角色，不得形成可执行任务。
    Missing,
    /// 已配置唯一责任角色与正式任务。
    Configured,
}

/// W17 映射任务的正式责任投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MappingTaskWorkItemView {
    pub work_item_id: String,
    pub task_version: String,
    pub work_item_type: WorkItemType,
    pub business_object_type: String,
    pub business_object_id: String,
    pub subject_version: String,
    pub status: WorkItemStatus,
    pub assignment_mode: AssignmentMode,
    pub owner_user_id: Option<String>,
    /// 当前 actor 的通用责任动作白名单。
    pub allowed_actions: Vec<String>,
}

/// 来源白名单证据字段。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MappingSourceEvidenceView {
    pub field: String,
    pub label: String,
    pub value: String,
    pub sensitive: bool,
}

/// ERP 规范候选目标。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MappingCandidateTargetView {
    pub object_type: String,
    pub object_id: String,
    pub stable_no: String,
    pub label: String,
    pub current_revision_id: String,
    pub eligibility: String,
    pub reason: String,
}

/// 当前映射目标谱系。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MappingCurrentTargetView {
    pub mapping_target_id: String,
    pub object_type: String,
    pub object_id: String,
    pub relation_role: RelationRole,
    pub valid_from: u64,
    pub valid_to: Option<u64>,
    pub status: String,
}

/// 映射处理审计投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MappingResolutionHistoryView {
    pub action: String,
    pub result: String,
    pub handled_by: String,
    pub handled_at: u64,
    pub evidence_reference: Option<String>,
}

/// Actor-specific 领域动作阻断。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MappingActionBlockerView {
    pub action: String,
    pub code: String,
    pub message: String,
}

/// 独立重新归集操作投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ReapplyOperationView {
    pub operation_id: String,
    pub mapping_task_id: String,
    pub source_snapshot_id: String,
    pub status: ReapplyOperationStatus,
    pub sales_order_id: Option<String>,
    pub sales_order_revision_id: Option<String>,
    pub receivable_result_reference: Option<String>,
    pub failure_code: Option<String>,
    pub failure_message: Option<String>,
    pub requested_at: Instant,
    pub last_updated_at: Instant,
}

impl From<MallSnapshotReapplyOperation> for ReapplyOperationView {
    fn from(operation: MallSnapshotReapplyOperation) -> Self {
        Self {
            operation_id: operation.base.id,
            mapping_task_id: operation.mapping_task_id.to_string(),
            source_snapshot_id: operation.source_snapshot_id.to_string(),
            status: operation.status,
            sales_order_id: operation.sales_order_id.map(|id| id.to_string()),
            sales_order_revision_id: operation.sales_order_revision_id.map(|id| id.to_string()),
            receivable_result_reference: operation.receivable_result_reference,
            failure_code: operation.failure_code,
            failure_message: operation.failure_message,
            requested_at: operation.requested_at,
            last_updated_at: operation.last_updated_at,
        }
    }
}

/// W17 治理动作正式结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct GovernanceActionResult {
    pub action_id: String,
    pub status: String,
    pub background_job_id: Option<String>,
    pub reapply_operation_status: Option<ReapplyOperationStatus>,
    pub sales_order_id: Option<String>,
    pub sales_order_revision_id: Option<String>,
    pub receivable_result_reference: Option<String>,
    pub recorded_at: Instant,
    pub next_actions: Vec<String>,
}

impl From<MallSnapshotReapplyOperation> for GovernanceActionResult {
    fn from(operation: MallSnapshotReapplyOperation) -> Self {
        let status = match operation.status {
            ReapplyOperationStatus::Queued | ReapplyOperationStatus::Running => "ACCEPTED",
            ReapplyOperationStatus::Succeeded => "SUCCEEDED",
            ReapplyOperationStatus::Failed => "FAILED",
            ReapplyOperationStatus::Unknown => "UNKNOWN",
        }
        .to_string();
        let next_actions = match operation.status {
            ReapplyOperationStatus::Failed | ReapplyOperationStatus::Unknown => {
                vec!["QUERY_REAPPLY_RESULT".to_string()]
            }
            ReapplyOperationStatus::Succeeded => vec!["OPEN_SALES_ORDER".to_string()],
            ReapplyOperationStatus::Queued | ReapplyOperationStatus::Running => {
                vec!["QUERY_REAPPLY_RESULT".to_string()]
            }
        };
        Self {
            action_id: operation.base.id,
            status,
            background_job_id: None,
            reapply_operation_status: Some(operation.status),
            sales_order_id: operation.sales_order_id.map(|id| id.to_string()),
            sales_order_revision_id: operation.sales_order_revision_id.map(|id| id.to_string()),
            receivable_result_reference: operation.receivable_result_reference,
            recorded_at: operation.last_updated_at,
            next_actions,
        }
    }
}

impl From<MasterMappingTask> for MasterMappingTaskView {
    /// 从实体构造响应视图。
    fn from(task: MasterMappingTask) -> Self {
        let owner_routing_state = if task.owner_role.is_some() {
            OwnerRoutingState::Configured
        } else {
            OwnerRoutingState::Missing
        };
        Self {
            id: task.base.id,
            source_snapshot_id: task.source_snapshot_id.to_string(),
            mapping_type: task.mapping_type,
            status: task.status,
            owner_role: task.owner_role,
            owner_user_id: task.owner_user_id,
            resolution: task.resolution,
            resolved_at: task.resolved_at,
            version: task.base.version,
            created_at: task.base.created_at,
            owner_routing_state,
            work_item: None,
            source_evidence: Vec::new(),
            candidate_targets: Vec::new(),
            current_targets: Vec::new(),
            external_identity_map_id: None,
            impact_summary: task.mapping_type.label().to_string(),
            resolution_history: Vec::new(),
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            reapply_operation: None,
            lock_version: task.base.version,
        }
    }
}

/// 映射任务列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MasterMappingTaskListParams {
    /// 待处理快照筛选。
    pub source_snapshot_id: Option<MallSalesOrderSnapshotId>,
    /// 映射类型筛选。
    pub mapping_type: Option<MappingTaskType>,
    /// 任务状态筛选。
    pub status: Option<MappingTaskStatus>,
    /// 责任角色筛选。
    pub owner_role: Option<String>,
    /// 责任用户 ID 筛选。
    pub owner_user_id: Option<String>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`resolved_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 映射任务详情定位参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MasterMappingTaskDetailParams {
    /// 从正式队列进入时必须携带的任务身份；若与映射任务不一致则失败关闭。
    pub work_item_id: Option<String>,
}

/// 归一化后的映射任务列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MasterMappingTaskListQuery {
    pub source_snapshot_id: Option<MallSalesOrderSnapshotId>,
    pub mapping_type: Option<MappingTaskType>,
    pub status: Option<MappingTaskStatus>,
    pub owner_role: Option<String>,
    pub owner_user_id: Option<String>,
    pub paging: PageParams,
}

impl MasterMappingTaskListParams {
    /// 归一化映射任务列表查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MasterMappingTaskListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, MASTER_MAPPING_TASK_SORT_FIELDS)?;
        Ok(MasterMappingTaskListQuery {
            source_snapshot_id: self.source_snapshot_id.clone(),
            mapping_type: self.mapping_type,
            status: self.status,
            owner_role: normalized_text(self.owner_role.as_deref()),
            owner_user_id: normalized_text(self.owner_user_id.as_deref()),
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 映射目标确认类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ConfirmMappingResolutionType {
    /// 确认来源身份到 ERP 规范对象的目标关系。
    ConfirmTarget,
}

/// 映射目标确认内容。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConfirmMappingResolution {
    /// 固定为 `CONFIRM_TARGET`。
    #[serde(rename = "type")]
    pub kind: ConfirmMappingResolutionType,
    /// 目标对象类型稳定代码。
    #[validate(custom(function = "non_blank", message = "目标对象类型不能为空"))]
    pub object_type: String,
    /// 目标对象 ID。
    #[validate(custom(function = "non_blank", message = "目标对象ID不能为空"))]
    pub object_id: String,
    /// 谱系关系角色。
    pub relation_role: RelationRole,
}

/// 确认映射的领域决策。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConfirmMappingDecision {
    pub mapping_task_id: String,
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    pub external_identity_map_id: Option<String>,
    #[validate(range(min = 1, message = "映射任务版本必须大于0"))]
    pub expected_mapping_task_version: u64,
    #[validate(custom(function = "non_blank", message = "映射操作ID不能为空"))]
    pub mapping_operation_id: String,
    pub execution_stage: MallSyncStage,
    #[validate(nested)]
    pub resolution: ConfirmMappingResolution,
    #[validate(custom(function = "non_blank", message = "确认依据不能为空"))]
    pub evidence_note: String,
}

/// W17 确认映射强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConfirmMappingCommand {
    pub work_item_id: String,
    #[validate(custom(function = "non_blank", message = "待办版本不能为空"))]
    pub expected_task_version: String,
    #[validate(custom(function = "non_blank", message = "对象版本不能为空"))]
    pub expected_subject_version: String,
    #[validate(nested)]
    pub decision: ConfirmMappingDecision,
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 来源修复动作类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SourceFixActionType {
    /// 只追加内部来源修复证据。
    RequestSourceFix,
}

/// 来源修复动作内容。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RequestSourceFixAction {
    #[serde(rename = "type")]
    pub kind: SourceFixActionType,
    pub mapping_task_id: String,
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    #[validate(range(min = 1, message = "映射任务版本必须大于0"))]
    pub expected_mapping_task_version: u64,
    #[validate(custom(function = "non_blank", message = "请求操作ID不能为空"))]
    pub request_operation_id: String,
    #[validate(custom(function = "non_blank", message = "来源修复原因代码不能为空"))]
    pub reason_code: String,
    #[validate(custom(function = "non_blank", message = "来源修复说明不能为空"))]
    pub reason_text: String,
    #[validate(length(min = 1, max = 20, message = "所需证据必须在1-20项之间"))]
    pub requested_evidence: Vec<String>,
}

/// W17 来源修复强类型非终结命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct RequestSourceFixCommand {
    pub work_item_id: String,
    #[validate(custom(function = "non_blank", message = "待办版本不能为空"))]
    pub expected_task_version: String,
    #[validate(custom(function = "non_blank", message = "对象版本不能为空"))]
    pub expected_subject_version: String,
    #[validate(nested)]
    pub action: RequestSourceFixAction,
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// W17 重新归集强类型命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReapplyMallSnapshotCommand {
    pub mapping_task_id: String,
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    #[validate(range(min = 1, message = "映射任务版本必须大于0"))]
    pub expected_mapping_version: u64,
    #[validate(custom(function = "non_blank", message = "重新归集操作ID不能为空"))]
    pub operation_id: String,
    pub execution_stage: MallSyncStage,
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 确认映射业务结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfirmMappingBusinessResult {
    pub mapping_task_id: String,
    pub mapping_task_status: MappingTaskStatus,
    pub external_identity_map_id: String,
    pub mapping_target_id: String,
    pub recorded_at: Instant,
    pub execution_stage: MallSyncStage,
}

/// 确认映射命令结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfirmMappingResult {
    pub work_item_id: String,
    pub work_item_status: entities::work_item::WorkItemStatus,
    pub business_result: ConfirmMappingBusinessResult,
}

/// 请求来源修复命令结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RequestSourceFixResult {
    pub work_item_id: String,
    pub work_item_status: entities::work_item::WorkItemStatus,
    pub task_version: String,
    pub mapping_task_id: String,
    pub mapping_task_status: MappingTaskStatus,
    pub mapping_evidence_entry_id: String,
    pub recorded_at: Instant,
}

#[cfg(test)]
mod tests {
    use super::{ConfirmMappingCommand, MasterMappingTaskListParams};
    use crate::mall_sync::dto::common::SortDir;

    #[test]
    fn mapping_query_normalizes_text_paging_and_sort_defaults() {
        let query = MasterMappingTaskListParams {
            source_snapshot_id: None,
            mapping_type: None,
            status: None,
            owner_role: Some(" owner ".to_string()),
            owner_user_id: Some("   ".to_string()),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        }
        .normalized()
        .unwrap();
        assert_eq!(query.owner_role.as_deref(), Some("owner"));
        assert_eq!(query.owner_user_id, None);
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn confirm_command_matches_strong_http_envelope() {
        let command: ConfirmMappingCommand = serde_json::from_value(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "2",
            "expected_subject_version": "1",
            "decision": {
                "mapping_task_id": "mt-1",
                "source_snapshot_id": "snap-1",
                "expected_mapping_task_version": 1,
                "mapping_operation_id": "op-1",
                "execution_stage": "FIRST_PHASE_MALL_OWNED",
                "resolution": {
                    "type": "CONFIRM_TARGET",
                    "object_type": "CUSTOMER",
                    "object_id": "customer-1",
                    "relation_role": "PRIMARY"
                },
                "evidence_note": "已核对客户主体"
            },
            "idempotency_key": "request-1"
        }))
        .unwrap();
        assert_eq!(command.expected_task_version, "2");
        assert_eq!(command.decision.mapping_task_id, "mt-1");
    }
}
