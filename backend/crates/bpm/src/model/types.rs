//! 合同与计划第 3.1 节固定枚举，以及 BPM 模型领域错误。
//!
//! 实例审批人绑定来源与节点执行分派来源必须是两个独立枚举。
//! 目标公开 API 不得导出旧符号。

use entity_core::{BaseModel, NOT_DELETED_TIMESTAMP};
use serde::{Deserialize, Serialize};

use super::Timestamp;

/// BPM 模型操作结果。
pub type ModelResult<T> = std::result::Result<T, ModelError>;

/// BPM 模型失败关闭错误。不含 HTTP、仓储或 ERP 业务语义。
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum ModelError {
    /// 名称、节点键、原因等字段为空、超长或形态非法。
    #[error("字段无效: {0}")]
    InvalidField(&'static str),
    /// 定义、实例或执行处于不允许该动作的状态。
    #[error("状态非法: {0}")]
    InvalidStatus(&'static str),
    /// 连线目标不满足「节点与终态恰有一个」或事件约束。
    #[error("连线非法: {0}")]
    InvalidTransition(&'static str),
    /// 已结束的节点执行不得重开或覆盖决定。
    #[error("节点执行已结束，不得重开或覆盖")]
    ExecutionAlreadyEnded,
    /// 轮次或版本 checked add 溢出。
    #[error("计数溢出: {0}")]
    Overflow(&'static str),
    /// 相同幂等键但载荷摘要不同。
    #[error("命令收据载荷冲突")]
    CommandReceiptConflict,
    /// 调用方时间不能表示为持久化秒。
    #[error("时间戳无效")]
    InvalidTimestamp,
}

/// 流程定义状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalDefinitionStatus {
    /// 草稿。
    Draft,
    /// 已发布。
    Published,
    /// 已退役。
    Retired,
}

impl ApprovalDefinitionStatus {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `DRAFT`、`PUBLISHED` 或 `RETIRED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Published => "PUBLISHED",
            Self::Retired => "RETIRED",
        }
    }

    /// 判断定义是否仍可修改草稿字段。
    ///
    /// # 返回
    /// 仅草稿返回 `true`。
    pub fn is_draft(self) -> bool {
        matches!(self, Self::Draft)
    }
}

/// 第一阶段仅允许的人工审批节点类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalNodeType {
    /// 指定到人的人工审批。
    UserApproval,
}

impl ApprovalNodeType {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `USER_APPROVAL`。
    pub fn as_str(self) -> &'static str {
        "USER_APPROVAL"
    }
}

/// 连线事件。第一阶段只允许通过与驳回。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalTransitionEvent {
    /// 通过。
    Approve,
    /// 驳回。
    Reject,
}

impl ApprovalTransitionEvent {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `APPROVE` 或 `REJECT`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
        }
    }
}

/// 流程终态结果。第一阶段只有通过。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalTerminalResult {
    /// 最终通过。
    Approved,
}

impl ApprovalTerminalResult {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `APPROVED`。
    pub fn as_str(self) -> &'static str {
        "APPROVED"
    }
}

/// 节点决定。不得包含退回申请人或终止审批。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalDecision {
    /// 通过当前节点。
    Approve,
    /// 驳回并进入下一轮入口。
    Reject,
}

impl ApprovalDecision {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `APPROVE` 或 `REJECT`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
        }
    }
}

/// 运行实例状态。驳回不是实例终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalProcessInstanceStatus {
    /// 运行中。
    Running,
    /// 最终通过。
    Approved,
    /// 已取消。
    Cancelled,
    /// 受阻。
    Blocked,
}

impl ApprovalProcessInstanceStatus {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `RUNNING`、`APPROVED`、`CANCELLED` 或 `BLOCKED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Approved => "APPROVED",
            Self::Cancelled => "CANCELLED",
            Self::Blocked => "BLOCKED",
        }
    }

    /// 判断实例是否已进入不可再推进的终态。
    ///
    /// # 返回
    /// 最终通过或取消时返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Approved | Self::Cancelled)
    }
}

/// 节点执行状态。不得包含等待预创建。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalNodeExecutionStatus {
    /// 当前活动。
    Active,
    /// 本节点已通过。
    Approved,
    /// 本节点已驳回。
    Rejected,
    /// 随实例取消。
    Cancelled,
    /// 人员或结构校验失败。
    Blocked,
    /// 已被原审批人恢复替换。
    Superseded,
}

impl ApprovalNodeExecutionStatus {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回固定执行状态代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Cancelled => "CANCELLED",
            Self::Blocked => "BLOCKED",
            Self::Superseded => "SUPERSEDED",
        }
    }

    /// 判断执行是否已结束。
    ///
    /// # 返回
    /// `ACTIVE` 与 `BLOCKED` 以外的状态返回 `true`。
    pub fn is_ended(self) -> bool {
        !matches!(self, Self::Active | Self::Blocked)
    }

    /// 判断执行是否可作为实例当前令牌。
    ///
    /// # 返回
    /// 仅 `ACTIVE` 与 `BLOCKED` 返回 `true`。
    pub fn is_current(self) -> bool {
        matches!(self, Self::Active | Self::Blocked)
    }
}

/// 实例审批人绑定来源。运行时绑定只能从已发布定义冻结。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalAssigneeBindingSource {
    /// 从已发布定义复制。
    Definition,
}

impl ApprovalAssigneeBindingSource {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `DEFINITION`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Definition => "DEFINITION",
        }
    }
}

/// 节点执行分派来源。仅执行允许记录人员恢复。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalExecutionAssignmentSource {
    /// 从已发布定义冻结的绑定进入。
    Definition,
    /// 原审批人恢复后的当次重建执行。
    AssigneeRecovery,
}

impl ApprovalExecutionAssignmentSource {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `DEFINITION` 或 `ASSIGNEE_RECOVERY`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Definition => "DEFINITION",
            Self::AssigneeRecovery => "ASSIGNEE_RECOVERY",
        }
    }
}

/// 被替换执行的固定结束原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalExecutionEndReason {
    /// 原审批人恢复结束旧执行。
    AssigneeRecovered,
}

impl ApprovalExecutionEndReason {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回 `ASSIGNEE_RECOVERED`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AssigneeRecovered => "ASSIGNEE_RECOVERED",
        }
    }
}

/// 结构化阻塞原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalBlockerCode {
    /// 审批人账号停用。
    ApproverAccountInactive,
    /// 审批人任职失效。
    ApproverEmploymentInvalid,
    /// 审批人不再具备资格。
    ApproverNotEligible,
    /// 审批人超出授权数据范围。
    #[serde(rename = "APPROVER_OUT_OF_DATA_SCOPE")]
    ApproverOutOfAuthorizedScope,
    /// 审批人不能读取被审对象。
    ApproverCannotReadSubject,
    /// 岗位分离冲突。
    SeparationOfDutiesViolation,
    /// 定义图损坏。
    DefinitionGraphCorrupted,
    /// 实例关联损坏。
    InstanceLinkCorrupted,
    /// 开放任务冲突。
    OpenTaskConflict,
    /// 提交版本冲突。
    SubjectVersionConflict,
    /// 内部不变量损坏。
    InternalInvariantBroken,
}

impl ApprovalBlockerCode {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回合同第 12.2 节固定 blocker 代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ApproverAccountInactive => "APPROVER_ACCOUNT_INACTIVE",
            Self::ApproverEmploymentInvalid => "APPROVER_EMPLOYMENT_INVALID",
            Self::ApproverNotEligible => "APPROVER_NOT_ELIGIBLE",
            Self::ApproverOutOfAuthorizedScope => "APPROVER_OUT_OF_DATA_SCOPE",
            Self::ApproverCannotReadSubject => "APPROVER_CANNOT_READ_SUBJECT",
            Self::SeparationOfDutiesViolation => "SEPARATION_OF_DUTIES_VIOLATION",
            Self::DefinitionGraphCorrupted => "DEFINITION_GRAPH_CORRUPTED",
            Self::InstanceLinkCorrupted => "INSTANCE_LINK_CORRUPTED",
            Self::OpenTaskConflict => "OPEN_TASK_CONFLICT",
            Self::SubjectVersionConflict => "SUBJECT_VERSION_CONFLICT",
            Self::InternalInvariantBroken => "INTERNAL_INVARIANT_BROKEN",
        }
    }

    /// 判断是否属于允许恢复原审批人的人员失效类别。
    ///
    /// # 返回
    /// 前六类人员资格阻塞返回 `true`，其余结构或一致性阻塞返回 `false`。
    pub fn allows_assignee_recovery(self) -> bool {
        matches!(
            self,
            Self::ApproverAccountInactive
                | Self::ApproverEmploymentInvalid
                | Self::ApproverNotEligible
                | Self::ApproverOutOfAuthorizedScope
                | Self::ApproverCannotReadSubject
                | Self::SeparationOfDutiesViolation
        )
    }
}

/// 命令收据记录的命令种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalCommandKind {
    /// 创建或编辑定义草稿。
    DefinitionWrite,
    /// 发布定义。
    PublishDefinition,
    /// 退役定义。
    RetireDefinition,
    /// 升级未提交单据绑定。
    UpgradeBinding,
    /// 启动审批。
    StartApproval,
    /// 提交决定。
    SubmitDecision,
    /// 业务撤回或应急撤回。
    CancelApproval,
    /// 恢复原审批人。
    ResumeApprover,
    /// 受阻取消。
    CancelBlocked,
}

impl ApprovalCommandKind {
    /// 审批命令收据允许持久化的完整权威集合。
    pub const ALL: [Self; 9] = [
        Self::DefinitionWrite,
        Self::PublishDefinition,
        Self::RetireDefinition,
        Self::UpgradeBinding,
        Self::StartApproval,
        Self::SubmitDecision,
        Self::CancelApproval,
        Self::ResumeApprover,
        Self::CancelBlocked,
    ];

    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回命令种类代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DefinitionWrite => "DEFINITION_WRITE",
            Self::PublishDefinition => "PUBLISH_DEFINITION",
            Self::RetireDefinition => "RETIRE_DEFINITION",
            Self::UpgradeBinding => "UPGRADE_BINDING",
            Self::StartApproval => "START_APPROVAL",
            Self::SubmitDecision => "SUBMIT_DECISION",
            Self::CancelApproval => "CANCEL_APPROVAL",
            Self::ResumeApprover => "RESUME_APPROVER",
            Self::CancelBlocked => "CANCEL_BLOCKED",
        }
    }
}

/// 名称、节点键等短文本的最大字节长度。
pub const NAME_MAX_LEN: usize = 256;
/// 节点键最大字节长度。
pub const NODE_KEY_MAX_LEN: usize = 64;
/// 原因、意见最大字节长度。
pub const REASON_MAX_LEN: usize = 512;
/// 显示名快照最大字节长度。
pub const LABEL_MAX_LEN: usize = 128;
/// 不透明用途键最大字节长度。
pub const PURPOSE_MAX_LEN: usize = 64;
/// 作用域 ID 与幂等键最大字节长度。
pub const SCOPE_MAX_LEN: usize = 128;
/// 载荷摘要最大字节长度。
pub const DIGEST_MAX_LEN: usize = 128;

/// 由调用方时间构造持久化元数据，禁止读取系统时钟。
///
/// # 参数
/// * `id` - 调用方已经生成的主键
/// * `at` - 调用方提供的时间
///
/// # 返回
/// 版本为 1、创建与更新时间均为 `at` 的元数据。
///
/// # 错误
/// 时间为负秒时返回 [`ModelError::InvalidTimestamp`]。
pub(crate) fn base_model_at(id: impl Into<String>, at: Timestamp) -> ModelResult<BaseModel> {
    let stamp = persistence_stamp(at)?;
    Ok(BaseModel {
        id: id.into(),
        version: 1,
        created_at: stamp,
        updated_at: stamp,
        deleted_at: NOT_DELETED_TIMESTAMP,
    })
}

/// 用调用方时间推进乐观锁版本与更新时间。
///
/// # 参数
/// * `base` - 持久化元数据
/// * `at` - 调用方提供的时间
///
/// # 错误
/// 时间为负或版本溢出时返回错误。
pub(crate) fn touch_base(base: &mut BaseModel, at: Timestamp) -> ModelResult<()> {
    base.updated_at = persistence_stamp(at)?;
    base.version = base
        .version
        .checked_add(1)
        .ok_or(ModelError::Overflow("乐观锁版本"))?;
    Ok(())
}

/// 把调用方时间转为非负秒。
///
/// # 错误
/// 负秒返回 [`ModelError::InvalidTimestamp`]。
pub(crate) fn persistence_stamp(at: Timestamp) -> ModelResult<u64> {
    u64::try_from(at.unix_secs()).map_err(|_| ModelError::InvalidTimestamp)
}

/// 规范化必填短文本。
///
/// # 错误
/// 空值或超长返回 [`ModelError::InvalidField`]。
pub(crate) fn normalize_required(
    value: impl Into<String>,
    empty: &'static str,
    max_len: usize,
    too_long: &'static str,
) -> ModelResult<String> {
    let trimmed = value.into();
    let trimmed = trimmed.trim();
    if trimmed.is_empty() {
        return Err(ModelError::InvalidField(empty));
    }
    if trimmed.len() > max_len {
        return Err(ModelError::InvalidField(too_long));
    }
    Ok(trimmed.to_string())
}

/// 规范化可选短文本。
///
/// # 错误
/// 去空白后超长返回 [`ModelError::InvalidField`]。
pub(crate) fn normalize_optional(
    value: Option<String>,
    max_len: usize,
    too_long: &'static str,
) -> ModelResult<Option<String>> {
    let Some(value) = value else {
        return Ok(None);
    };
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if trimmed.len() > max_len {
        return Err(ModelError::InvalidField(too_long));
    }
    Ok(Some(trimmed.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovalAssigneeBindingSource, ApprovalBlockerCode, ApprovalCommandKind, ApprovalDecision,
        ApprovalDefinitionStatus, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
        ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus, ApprovalTerminalResult,
        ApprovalTransitionEvent,
    };

    /// 绑定来源固定为定义，执行来源只增加原审批人恢复。
    #[test]
    fn assignment_sources_only_allow_definition_and_assignee_recovery() {
        let binding_sources = [ApprovalAssigneeBindingSource::Definition];
        let execution_sources = [
            ApprovalExecutionAssignmentSource::Definition,
            ApprovalExecutionAssignmentSource::AssigneeRecovery,
        ];
        let end_reasons = [ApprovalExecutionEndReason::AssigneeRecovered];
        assert_eq!(binding_sources.len(), 1);
        assert_eq!(execution_sources.len(), 2);
        assert_eq!(end_reasons.len(), 1);
        assert_eq!(ApprovalAssigneeBindingSource::Definition.as_str(), "DEFINITION");
        assert_eq!(
            ApprovalExecutionAssignmentSource::AssigneeRecovery.as_str(),
            "ASSIGNEE_RECOVERY"
        );
        assert_eq!(
            ApprovalExecutionEndReason::AssigneeRecovered.as_str(),
            "ASSIGNEE_RECOVERED"
        );
    }

    /// 开发期硬切换不得为已删除的审批改派枚举保留反序列化别名。
    #[test]
    fn removed_reassign_codes_are_rejected() {
        assert!(serde_json::from_str::<ApprovalAssigneeBindingSource>("\"ADMIN_REASSIGN\"").is_err());
        assert!(serde_json::from_str::<ApprovalExecutionAssignmentSource>("\"ADMIN_REASSIGN\"").is_err());
        assert!(serde_json::from_str::<ApprovalExecutionEndReason>("\"ADMIN_REASSIGNED\"").is_err());
        assert!(serde_json::from_str::<ApprovalCommandKind>("\"REASSIGN_APPROVER\"").is_err());
    }

    /// 命令收据权威集合固定为删除审批改派后的九种稳定代码。
    #[test]
    fn command_kind_all_is_complete_and_unique() {
        assert_eq!(ApprovalCommandKind::ALL.len(), 9);
        assert_eq!(
            ApprovalCommandKind::ALL.map(ApprovalCommandKind::as_str),
            [
                "DEFINITION_WRITE",
                "PUBLISH_DEFINITION",
                "RETIRE_DEFINITION",
                "UPGRADE_BINDING",
                "START_APPROVAL",
                "SUBMIT_DECISION",
                "CANCEL_APPROVAL",
                "RESUME_APPROVER",
                "CANCEL_BLOCKED",
            ]
        );
        let mut codes = ApprovalCommandKind::ALL.map(ApprovalCommandKind::as_str);
        codes.sort_unstable();
        assert!(codes.windows(2).all(|pair| pair[0] != pair[1]));
    }

    /// 固定枚举代码与合同取值一致。
    #[test]
    fn fixed_enum_codes_match_contract() {
        assert_eq!(ApprovalDefinitionStatus::Draft.as_str(), "DRAFT");
        assert_eq!(ApprovalTransitionEvent::Reject.as_str(), "REJECT");
        assert_eq!(ApprovalTerminalResult::Approved.as_str(), "APPROVED");
        assert_eq!(ApprovalDecision::Approve.as_str(), "APPROVE");
        assert_eq!(ApprovalProcessInstanceStatus::Blocked.as_str(), "BLOCKED");
        assert!(!ApprovalProcessInstanceStatus::Running.is_terminal());
        assert!(ApprovalProcessInstanceStatus::Approved.is_terminal());
        assert!(ApprovalNodeExecutionStatus::Approved.is_ended());
        assert!(!ApprovalNodeExecutionStatus::Blocked.is_ended());
        assert_eq!(
            ApprovalExecutionEndReason::AssigneeRecovered.as_str(),
            "ASSIGNEE_RECOVERED"
        );
    }

    /// 仅前六类人员阻塞允许恢复原审批人。
    #[test]
    fn only_personnel_blockers_allow_assignee_recovery() {
        assert!(ApprovalBlockerCode::ApproverAccountInactive.allows_assignee_recovery());
        assert!(ApprovalBlockerCode::SeparationOfDutiesViolation.allows_assignee_recovery());
        assert!(!ApprovalBlockerCode::DefinitionGraphCorrupted.allows_assignee_recovery());
        assert!(!ApprovalBlockerCode::InstanceLinkCorrupted.allows_assignee_recovery());
        assert!(!ApprovalBlockerCode::OpenTaskConflict.allows_assignee_recovery());
        assert!(!ApprovalBlockerCode::SubjectVersionConflict.allows_assignee_recovery());
        assert!(!ApprovalBlockerCode::InternalInvariantBroken.allows_assignee_recovery());
    }
}
