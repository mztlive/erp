//! 合同与计划第 3.1 节固定枚举，以及 BPM 模型领域错误。
//!
//! 错误分界：本模块 `ModelError` 用于状态机不变式失败
//! （字段、状态、连线、计数溢出）；边界值对象构造失败
//! （`SubjectRef`/`ParticipantId`/`Timestamp`/`ProcessKind` 编码）使用
//! `crate::error::Error`。两套 `Invalid*` 不混用，见 `error.rs` 对照。
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
    /// 输出定义生命周期的稳定代码，供调用方传递草稿、发布和退役状态，不依赖 Rust 变体名。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Published => "PUBLISHED",
            Self::Retired => "RETIRED",
        }
    }

    /// 判断定义状态是否允许编辑名称或入口；发布后不能再沿用草稿修改路径。
    pub fn is_draft(self) -> bool {
        matches!(self, Self::Draft)
    }

    /// 绑定前按定义自己的状态确认它仍已发布；仓储按状态筛选不能代替这一步。
    pub fn is_published(self) -> bool {
        matches!(self, Self::Published)
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
    /// 为节点类型提供与存储合同一致的 `USER_APPROVAL`，供定义展示或交换使用。
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
    /// 构造或比较决定连线时使用稳定事件码，以区分 `APPROVE` 与 `REJECT` 两条边。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
        }
    }
}

/// 流程终态结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalTerminalResult {
    /// 最终通过。
    Approved,
    /// 本次审批已驳回。
    Rejected,
}

impl ApprovalTerminalResult {
    /// 在比对冻结图的终态目标时取稳定代码，区分审批实例最终通过与被驳回。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// 节点决定。驳回终止本次审批实例。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalDecision {
    /// 通过当前节点。
    Approve,
    /// 驳回并终止本次审批。
    Reject,
}

impl ApprovalDecision {
    /// 在表示或编码审批决定时使用稳定代码，不依赖 Rust 枚举格式。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Approve => "APPROVE",
            Self::Reject => "REJECT",
        }
    }
}

/// 运行实例状态。驳回终止当前实例，重新提交创建新实例。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalProcessInstanceStatus {
    /// 运行中。
    Running,
    /// 最终通过。
    Approved,
    /// 最终驳回。
    Rejected,
    /// 已取消。
    Cancelled,
    /// 受阻。
    Blocked,
}

impl ApprovalProcessInstanceStatus {
    /// 给实例状态投影提供稳定代码；调用方不必把受阻解释为终态或从显示文案推断状态。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Approved => "APPROVED",
            Self::Rejected => "REJECTED",
            Self::Cancelled => "CANCELLED",
            Self::Blocked => "BLOCKED",
        }
    }

    /// 推进或取消实例前判断流程是否已经收结；通过、驳回和取消为终态，受阻不是。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Approved | Self::Rejected | Self::Cancelled)
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
    /// 为执行历史和待办理状态提供稳定代码，区分已结束、受阻和仍活动等状态。
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

    /// 修改执行前判断是否已结束，防止覆盖决定或重开旧记录；活动与受阻均尚未结束。
    pub fn is_ended(self) -> bool {
        !matches!(self, Self::Active | Self::Blocked)
    }

    /// 检查执行状态是否允许其作为当前令牌；活动和受阻都可以，令牌是否确属当前实例仍须对照引用。
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
    /// 表示实例审批人绑定的稳定来源码，始终为 `DEFINITION`，不与执行恢复来源混用。
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
    /// 展示或记录执行分派来源时使用稳定代码，区分按定义首次进入与原人恢复进入。
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
    /// 记录旧执行被原人恢复替换的固定结束原因，供历史记录消费，无需从事由猜测。
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
    /// 向事件或状态视图提供稳定阻塞码；旧合同如 `APPROVER_OUT_OF_DATA_SCOPE` 不随变体名变化。
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

    /// 为恢复和撤回分类识别可恢复的六类人员失效；定义、链接、任务及其他一致性问题不走原人恢复。
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

    /// 计算命令作用域与摘要、表示收据中的命令种类时使用稳定代码；改码会影响既有身份的回放匹配。
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

// 创建流程模型的持久化元数据时使用调用方已生成的 ID 和时间，将版本置为 1，创建与更新时间相同；负秒返回 `ModelError::InvalidTimestamp`。
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

// 实体变更时用调用方时间更新持久化元数据并推进 CAS 版本；负时间或版本溢出返回错误。
pub(crate) fn touch_base(base: &mut BaseModel, at: Timestamp) -> ModelResult<()> {
    base.updated_at = persistence_stamp(at)?;
    base.version = base.version.checked_add(1).ok_or(ModelError::Overflow("乐观锁版本"))?;
    Ok(())
}

// 给元数据写入无符号 Unix 秒前检查时间非负；负秒返回 `ModelError::InvalidTimestamp`，不能直接强制转换。
pub(crate) fn persistence_stamp(at: Timestamp) -> ModelResult<u64> {
    u64::try_from(at.unix_secs()).map_err(|_| ModelError::InvalidTimestamp)
}

// 保存必填名称、节点键或事由时，获取裁剪且符合 UTF-8 字节上限的文本；无需裁剪时复用原字符串，空值或超长返回 `ModelError::InvalidField`。
pub(crate) fn normalize_required(
    value: impl Into<String>,
    empty: &'static str,
    max_len: usize,
    too_long: &'static str,
) -> ModelResult<String> {
    let value = value.into();
    let trimmed = trim_required(&value, empty, max_len, too_long)?;
    if trimmed.len() == value.len() { Ok(value) } else { Ok(trimmed.to_string()) }
}

// 在键校验或名称快照只需读取时借用裁剪后的文本；用调用方给定的字节上限与错误说明拒绝空值或超长。
pub(crate) fn trim_required<'a>(
    value: &'a str,
    empty: &'static str,
    max_len: usize,
    too_long: &'static str,
) -> ModelResult<&'a str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(ModelError::InvalidField(empty));
    }
    if trimmed.len() > max_len {
        return Err(ModelError::InvalidField(too_long));
    }
    Ok(trimmed)
}

// 写入可选用途或意见时将缺省、空串和纯空白统一作 `None`，裁剪后依 UTF-8 字节上限拦截超长输入；无需裁剪时复用原字符串。
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
    if trimmed.len() == value.len() { Ok(Some(value)) } else { Ok(Some(trimmed.to_string())) }
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovalAssigneeBindingSource, ApprovalBlockerCode, ApprovalCommandKind, ApprovalDecision,
        ApprovalDefinitionStatus, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
        ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus, ApprovalTerminalResult,
        ApprovalTransitionEvent,
    };

    // 锁定绑定、执行和旧执行结束原因各自的编码，确保原人恢复只出现在执行历史中。
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
        assert_eq!(ApprovalExecutionAssignmentSource::AssigneeRecovery.as_str(), "ASSIGNEE_RECOVERY");
        assert_eq!(ApprovalExecutionEndReason::AssigneeRecovered.as_str(), "ASSIGNEE_RECOVERED");
    }

    // 验证开发期移除的管理员改派代码不再被旧别名反序列化，覆盖绑定、执行、结束原因和命令收据。
    #[test]
    fn removed_reassign_codes_are_rejected() {
        assert!(serde_json::from_str::<ApprovalAssigneeBindingSource>("\"ADMIN_REASSIGN\"").is_err());
        assert!(serde_json::from_str::<ApprovalExecutionAssignmentSource>("\"ADMIN_REASSIGN\"").is_err());
        assert!(serde_json::from_str::<ApprovalExecutionEndReason>("\"ADMIN_REASSIGNED\"").is_err());
        assert!(serde_json::from_str::<ApprovalCommandKind>("\"REASSIGN_APPROVER\"").is_err());
    }

    // 检查移除改派后的九种命令代码齐全、有序且唯一，保护命令收据和身份计算的编码合同。
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

    // 抽查定义、连线、决定和运行状态的稳定编码及分类，防止枚举调整改变既有合同。
    #[test]
    fn fixed_enum_codes_match_contract() {
        assert_eq!(ApprovalDefinitionStatus::Draft.as_str(), "DRAFT");
        assert!(ApprovalDefinitionStatus::Draft.is_draft());
        assert!(!ApprovalDefinitionStatus::Draft.is_published());
        assert!(ApprovalDefinitionStatus::Published.is_published());
        assert!(!ApprovalDefinitionStatus::Retired.is_published());

        assert_eq!(ApprovalTransitionEvent::Reject.as_str(), "REJECT");
        assert_eq!(ApprovalTerminalResult::Approved.as_str(), "APPROVED");
        assert_eq!(ApprovalDecision::Approve.as_str(), "APPROVE");

        assert_eq!(ApprovalProcessInstanceStatus::Blocked.as_str(), "BLOCKED");
        assert!(!ApprovalProcessInstanceStatus::Running.is_terminal());
        assert!(ApprovalProcessInstanceStatus::Approved.is_terminal());

        assert!(ApprovalNodeExecutionStatus::Approved.is_ended());
        assert!(!ApprovalNodeExecutionStatus::Blocked.is_ended());
        assert_eq!(ApprovalExecutionEndReason::AssigneeRecovered.as_str(), "ASSIGNEE_RECOVERED");
    }

    // 以人员阻塞类别的首尾及各结构性阻塞做对照，确保非人员异常不能走原人恢复。
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
