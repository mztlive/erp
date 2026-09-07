//! 导入确认的组合响应；任务类型与状态直接采用工作流唯一类型。

use erp_import::{
    ConfirmationDecision, ConfirmationStatus, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, LegacyImportConfirmation,
};
use erp_workflow::entity::work_item::{WorkItemStatus, WorkItemType};
use serde::Serialize;

/// 导入确认任务的服务端真实投影。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ImportBusinessConfirmationWorkItemView {
    /// 任务稳定 ID。
    pub work_item_id: String,
    /// 固定任务类型。
    pub work_item_type: WorkItemType,
    /// 任务乐观锁版本。
    pub task_version: String,
    /// 任务冻结的主体版本。
    pub subject_version: String,
    /// 任务生命周期状态。
    pub status: WorkItemStatus,
    /// 固定责任角色。
    pub owner_role: String,
    /// 固定责任组织。
    pub owner_organization_id: String,
    /// 当前个人责任人。
    pub owner_user_id: Option<String>,
    /// 导入确认不依赖审批步骤，固定为 `READY`。
    pub processing_state: String,
    /// 当前查询无操作人上下文，动作必须失败关闭。
    pub allowed_actions: Vec<String>,
    /// 当前投影无额外审批阻断事实。
    pub action_blockers: Vec<String>,
    /// 服务端任务处理器键。
    pub handler_key: String,
    /// 服务端目标工作面。
    pub destination_workspace_id: String,
}

/// 导入确认响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct LegacyImportConfirmationView {
    /// 实体主键。
    pub id: String,
    /// 所属导入批次。
    pub batch_id: String,
    /// 责任范围。
    pub confirmation_scope: String,
    /// 责任角色。
    pub owner_role: String,
    /// 本次确认针对的批次版本。
    pub batch_version: u32,
    /// 本次确认针对的试算版本。
    pub trial_version: u32,
    /// 确认状态。
    pub status: ConfirmationStatus,
    /// 确认决策；待确认/失效时为空。
    pub decision: Option<ConfirmationDecision>,
    /// 退回原因代码。
    pub reason_code: Option<String>,
    /// 意见说明。
    pub comment: Option<String>,
    /// 对应 `IMPORT_BUSINESS_CONFIRMATION` 正式任务。
    pub work_item_id: String,
    /// 任务实时投影；关联任务缺失时为空并由客户端失败关闭。
    pub work_item: Option<ImportBusinessConfirmationWorkItemView>,
    /// 实际确认或退回人。
    pub decided_by: Option<String>,
    /// 实际确认或退回时间（秒级时间戳）。
    pub decided_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<LegacyImportConfirmation> for LegacyImportConfirmationView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `confirmation` - 导入确认事实实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(confirmation: LegacyImportConfirmation) -> Self {
        Self {
            id: confirmation.base.id,
            batch_id: confirmation.batch_id.to_string(),
            confirmation_scope: confirmation.confirmation_scope,
            owner_role: confirmation.owner_role,
            batch_version: confirmation.batch_version,
            trial_version: confirmation.trial_version,
            status: confirmation.status,
            decision: confirmation.decision,
            reason_code: confirmation.reason_code,
            comment: confirmation.comment,
            work_item_id: confirmation.work_item_id.to_string(),
            work_item: None,
            decided_by: confirmation.decided_by,
            decided_at: confirmation.decided_at.map(|at| at.unix_secs()),
            version: confirmation.base.version,
            created_at: confirmation.base.created_at,
        }
    }
}

/// `CompleteImportBusinessConfirmation` 稳定结果信封。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CompleteImportBusinessConfirmationResult {
    /// 领域结论状态。
    pub result_status: ImportBusinessConfirmationResultStatus,
    /// 不可变的确认或退回事实。
    pub confirmation: LegacyImportConfirmationView,
    /// 已完成的正式任务投影。
    pub work_item: ImportBusinessConfirmationWorkItemView,
    /// 事务提交后的批次乐观锁版本。
    pub batch_version: u64,
    /// 服务端确定的下一步。
    pub next_step: ImportBusinessConfirmationNextStep,
    /// 不含原始幂等键的稳定审计收据 ID。
    pub audit_receipt: String,
}
