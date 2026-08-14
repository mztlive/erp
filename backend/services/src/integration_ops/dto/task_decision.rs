//! W29 强类型任务动作、任务完成与直接对账决定合同。

use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};

const ID_MAX_LEN: usize = 128;
const OPERATION_ID_MAX_LEN: usize = 64;
const VERSION_MAX_LEN: usize = 128;
const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
const REASON_CODE_MAX_LEN: usize = 64;
const COMMENT_MAX_LEN: usize = 512;
const EVIDENCE_REF_MAX_COUNT: usize = 8;

/// W29 强命令支持的业务项类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationItemType {
    /// 集成错误任务。
    ErrorTask,
    /// 对账差异。
    ReconciliationDifference,
}

/// 非终结任务动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationTaskActionKind {
    /// 查询原结果。
    QueryOriginalResult,
    /// 重放原动作。
    ReplayOriginal,
    /// 重新归集。
    Reattribute,
    /// 关联正式补偿。
    LinkCompensation,
    /// 追加证据。
    AddEvidence,
}

impl IntegrationTaskActionKind {
    /// 返回审计与持久化使用的稳定代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::QueryOriginalResult => "QUERY_ORIGINAL_RESULT",
            Self::ReplayOriginal => "REPLAY_ORIGINAL",
            Self::Reattribute => "REATTRIBUTE",
            Self::LinkCompensation => "LINK_COMPENSATION",
            Self::AddEvidence => "ADD_EVIDENCE",
        }
    }
}

/// 受控证据类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ControlledEvidenceKind {
    /// 外部案例结果。
    ExternalCaseResult,
    /// 业务对象核验。
    BusinessObjectVerification,
    /// 财务对账。
    FinancialReconciliation,
    /// 补偿结果。
    CompensationResult,
    /// 独立复核。
    DistinctReview,
}

impl ControlledEvidenceKind {
    /// 返回稳定代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::ExternalCaseResult => "EXTERNAL_CASE_RESULT",
            Self::BusinessObjectVerification => "BUSINESS_OBJECT_VERIFICATION",
            Self::FinancialReconciliation => "FINANCIAL_RECONCILIATION",
            Self::CompensationResult => "COMPENSATION_RESULT",
            Self::DistinctReview => "DISTINCT_REVIEW",
        }
    }
}

/// 客户端提交的证据引用；服务端仍须验证被引用事实。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ControlledEvidenceRef {
    /// 证据类型。
    pub kind: ControlledEvidenceKind,
    /// 正式记录 ID。
    pub record_id: String,
    /// 展示标签，不参与事实判定。
    pub label: String,
}

/// 非终结业务动作内容。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationNonTerminalTaskAction {
    /// 业务项类型。
    pub item_type: IntegrationItemType,
    /// 业务项 ID。
    pub item_id: String,
    /// 固定动作。
    pub kind: IntegrationTaskActionKind,
    /// 本次正式操作 ID。
    pub operation_id: String,
    /// 可选固定原因代码。
    pub reason_code: Option<String>,
    /// 可选备注。
    pub comment: Option<String>,
    /// 待验证的正式证据引用。
    #[serde(default)]
    pub evidence_refs: Vec<ControlledEvidenceRef>,
}

/// W29 非终结任务动作命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationTaskActionCommand {
    /// 正式任务 ID。
    pub work_item_id: String,
    /// 期望任务版本（十进制字符串）。
    pub expected_task_version: String,
    /// 期望业务主题版本。
    pub expected_subject_version: String,
    /// 非终结动作。
    pub action: IntegrationNonTerminalTaskAction,
    /// 客户端请求幂等键。
    pub idempotency_key: String,
}

impl IntegrationTaskActionCommand {
    /// 校验命令边界与字段长度。
    pub(crate) fn validate(&self) -> Result<()> {
        validate_command_identity(
            &self.work_item_id,
            &self.expected_task_version,
            &self.expected_subject_version,
            &self.idempotency_key,
        )?;
        validate_action(&self.action)
    }
}

/// W29 任务完成决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationTaskCompletionDecision {
    /// 业务项类型。
    pub item_type: IntegrationItemType,
    /// 业务项 ID。
    pub item_id: String,
    /// 固定完成决定，必须为 `RESOLVE`。
    pub kind: IntegrationTaskCompletionKind,
    /// 本次正式操作 ID。
    pub operation_id: String,
    /// 固定原因代码。
    pub reason_code: IntegrationResolutionReasonCode,
    /// 可选备注。
    pub comment: Option<String>,
    /// 服务端投影的固定证据策略 ID。
    pub evidence_policy_id: String,
    /// 证据策略版本。
    pub evidence_policy_version: u64,
    /// 证据策略键。
    pub policy_key: EvidencePolicyKey,
    /// 待验证的正式证据引用。
    #[serde(default)]
    pub evidence_refs: Vec<ControlledEvidenceRef>,
}

/// W29 任务完成固定原因代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationResolutionReasonCode {
    /// 服务端已按固定策略重验全部终态证据。
    TerminalEvidenceVerified,
}

impl IntegrationResolutionReasonCode {
    /// 返回审计使用的稳定代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::TerminalEvidenceVerified => "TERMINAL_EVIDENCE_VERIFIED",
        }
    }
}

/// 唯一允许的任务完成决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationTaskCompletionKind {
    /// 形成正式解决结论并完成任务。
    Resolve,
}

/// 证据策略匹配键。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidencePolicyKey {
    /// 错误类型。
    pub error_type: String,
    /// 资金影响代码。
    pub funds_impact: String,
}

/// W29 任务完成强命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IntegrationTaskCompletionCommand {
    /// 正式任务 ID。
    pub work_item_id: String,
    /// 期望任务版本（十进制字符串）。
    pub expected_task_version: String,
    /// 期望业务主题版本。
    pub expected_subject_version: String,
    /// 完成决定。
    pub decision: IntegrationTaskCompletionDecision,
    /// 客户端请求幂等键。
    pub idempotency_key: String,
}

impl IntegrationTaskCompletionCommand {
    /// 校验命令边界与字段长度。
    pub(crate) fn validate(&self) -> Result<()> {
        validate_command_identity(
            &self.work_item_id,
            &self.expected_task_version,
            &self.expected_subject_version,
            &self.idempotency_key,
        )?;
        required(&self.decision.item_id, "业务项 ID", ID_MAX_LEN)?;
        required(&self.decision.operation_id, "操作 ID", OPERATION_ID_MAX_LEN)?;
        required(
            self.decision.reason_code.as_str(),
            "解决原因代码",
            REASON_CODE_MAX_LEN,
        )?;
        required(&self.decision.evidence_policy_id, "证据策略 ID", ID_MAX_LEN)?;
        if self.decision.evidence_policy_version == 0 {
            return Err(Error::ValidationError("证据策略版本必须大于 0".to_string()));
        }
        required(
            &self.decision.policy_key.error_type,
            "策略错误类型",
            REASON_CODE_MAX_LEN,
        )?;
        required(
            &self.decision.policy_key.funds_impact,
            "策略资金影响",
            REASON_CODE_MAX_LEN,
        )?;
        optional(&self.decision.comment, "备注", COMMENT_MAX_LEN)?;
        validate_evidence_refs(&self.decision.evidence_refs)?;
        if self.decision.evidence_refs.is_empty() {
            return Err(Error::ValidationError("完成任务必须提供终态证据引用".to_string()));
        }
        Ok(())
    }
}

/// 直接对账决定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum DirectReconciliationDecision {
    /// 只追加非终结证据动作。
    NonTerminalAction {
        /// 固定动作。
        action: IntegrationTaskActionKind,
        /// 待验证的证据引用。
        #[serde(default)]
        evidence_refs: Vec<ControlledEvidenceRef>,
        /// 可选备注。
        comment: Option<String>,
    },
    /// 形成正式终态结论。
    TerminalConclusion {
        /// 固定结论。
        conclusion: DirectReconciliationConclusion,
        /// 文档合同中的固定原因代码。
        reason_code: DifferenceReasonCode,
        /// 服务端投影的固定原因注册表身份。
        reason_registry_id: String,
        /// 原因注册表版本。
        reason_registry_version: u64,
        /// 注册原因 ID。
        registered_reason_id: String,
        /// 待验证的证据引用。
        #[serde(default)]
        evidence_refs: Vec<ControlledEvidenceRef>,
        /// 可选备注。
        comment: Option<String>,
    },
}

/// 直接对账终态结论。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectReconciliationConclusion {
    /// 确认无误。
    ConfirmNoError,
    /// 确认有效差异。
    ConfirmValidDifference,
}

/// 对账终结固定原因代码。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DifferenceReasonCode {
    /// 来源已更正并重新归集。
    SourceCorrectedAndReattributed,
    /// 业务确认无误。
    BusinessConfirmedNoError,
    /// 已补偿闭环。
    CompensationClosed,
}

impl DifferenceReasonCode {
    /// 返回固定原因代码。
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::SourceCorrectedAndReattributed => "SOURCE_CORRECTED_AND_REATTRIBUTED",
            Self::BusinessConfirmedNoError => "BUSINESS_CONFIRMED_NO_ERROR",
            Self::CompensationClosed => "COMPENSATION_CLOSED",
        }
    }
}

/// 证据策略要求的岗位分离规则。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ReviewerSeparation {
    /// 不要求额外复核人。
    None,
    /// 证据复核人与当前处理人必须不同。
    DistinctReviewer,
    /// 财务复核人与当前处理人必须不同。
    DistinctFinanceReviewer,
}

/// 服务端固定终态证据策略投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ResolutionEvidencePolicyView {
    /// 策略稳定 ID。
    pub evidence_policy_id: String,
    /// 策略版本。
    pub evidence_policy_version: u64,
    /// 服务端按当前业务项派生的策略键。
    pub key: EvidencePolicyKey,
    /// 完成任务所需的全部证据类型。
    pub required_evidence_kinds: Vec<ControlledEvidenceKind>,
    /// 岗位分离要求。
    pub reviewer_separation: ReviewerSeparation,
}

/// 固定直接对账原因投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RegisteredReconciliationReasonView {
    /// 注册原因稳定 ID。
    pub registered_reason_id: String,
    /// 注册原因版本。
    pub registered_reason_version: u64,
    /// 唯一允许的结论。
    pub conclusion: DirectReconciliationConclusion,
    /// 展示标签。
    pub label: String,
    /// 形成结论所需的全部证据类型。
    pub required_evidence_kinds: Vec<ControlledEvidenceKind>,
}

/// 无正式任务差异可用的固定原因注册表投影。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ReconciliationReasonRegistryView {
    /// 注册表稳定 ID。
    pub reason_registry_id: String,
    /// 注册表版本。
    pub reason_registry_version: u64,
    /// 注册原因列表。
    pub registered_reasons: Vec<RegisteredReconciliationReasonView>,
}

/// 无正式任务的直接对账命令。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DirectReconciliationCommand {
    /// 差异 ID；必须与路径一致。
    pub difference_id: String,
    /// 期望的最新决定序号（十进制字符串，初始为 `0`）。
    pub expected_difference_version: String,
    /// 强类型决定。
    pub decision: DirectReconciliationDecision,
    /// 本次正式操作 ID。
    pub operation_id: String,
    /// 客户端请求幂等键。
    pub idempotency_key: String,
}

impl DirectReconciliationCommand {
    /// 校验路径外命令边界与字段长度。
    pub(crate) fn validate(&self) -> Result<()> {
        required(&self.difference_id, "差异 ID", ID_MAX_LEN)?;
        decimal_version(&self.expected_difference_version, true, "差异版本")?;
        required(&self.operation_id, "操作 ID", OPERATION_ID_MAX_LEN)?;
        required(&self.idempotency_key, "幂等键", IDEMPOTENCY_KEY_MAX_LEN)?;
        match &self.decision {
            DirectReconciliationDecision::NonTerminalAction {
                evidence_refs,
                comment,
                ..
            } => {
                optional(comment, "备注", COMMENT_MAX_LEN)?;
                validate_evidence_refs(evidence_refs)
            }
            DirectReconciliationDecision::TerminalConclusion {
                evidence_refs,
                comment,
                reason_registry_id,
                reason_registry_version,
                registered_reason_id,
                ..
            } => {
                required(reason_registry_id, "原因注册表 ID", ID_MAX_LEN)?;
                if *reason_registry_version == 0 {
                    return Err(Error::ValidationError("原因注册表版本必须大于 0".to_string()));
                }
                required(registered_reason_id, "注册原因 ID", ID_MAX_LEN)?;
                optional(comment, "备注", COMMENT_MAX_LEN)?;
                validate_evidence_refs(evidence_refs)?;
                if evidence_refs.is_empty() {
                    return Err(Error::ValidationError("终态对账必须提供证据引用".to_string()));
                }
                Ok(())
            }
        }
    }
}

/// 非终结动作的服务端事实结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationActionOutcome {
    /// 已找到可验证终态证据。
    TerminalEvidenceFound,
    /// 已明确确认原动作无结果。
    NoResultConfirmed,
    /// 当前仍无法确认结果。
    ResultUnknown,
    /// 重放已被权威动作端受理。
    ReplayAccepted,
    /// 已重新归集。
    Reattributed,
    /// 已关联正式补偿。
    EvidenceLinked,
    /// 已追加证据。
    EvidenceAdded,
    /// 已确认无误。
    ConfirmedNoError,
    /// 已确认有效差异。
    ConfirmedValidDifference,
}

/// 单次非终结动作的证据结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationTaskActionEvidence {
    /// 操作 ID。
    pub operation_id: String,
    /// 服务端判定结果。
    pub outcome: IntegrationActionOutcome,
    /// 正式业务结果引用。
    pub business_result_reference: Option<String>,
    /// 本次追加证据记录引用。
    pub evidence_reference: Option<String>,
}

/// W29 非终结任务动作结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationTaskActionResult {
    /// 正式任务 ID。
    pub work_item_id: String,
    /// 恒为 `OPEN`。
    pub work_item_status: IntegrationWorkItemStatus,
    /// 本次证据结果。
    pub evidence: IntegrationTaskActionEvidence,
    /// 服务端按当前事实给出的下一动作。
    pub next_allowed_actions: Vec<String>,
}

/// W29 强命令返回的任务状态子集。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum IntegrationWorkItemStatus {
    /// 任务仍开放。
    Open,
    /// 任务已由强类型命令完成。
    Completed,
}

/// W29 任务完成结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct IntegrationTaskCompletionResult {
    /// 正式任务 ID。
    pub work_item_id: String,
    /// 恒为 `COMPLETED`。
    pub work_item_status: IntegrationWorkItemStatus,
    /// 操作 ID。
    pub operation_id: String,
    /// 正式解决记录 ID。
    pub resolution_record_id: String,
    /// 已由服务端验证的终态证据引用。
    pub terminal_evidence_reference: String,
}

/// 直接对账结果状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DirectReconciliationStatus {
    /// 仍开放。
    Open,
    /// 已追加证据，仍待正式结论。
    EvidencePending,
    /// 已确认无误。
    ConfirmedNoError,
    /// 已确认有效差异。
    ConfirmedValidDifference,
}

/// 无正式任务的直接对账结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DirectReconciliationResult {
    /// 差异 ID。
    pub difference_id: String,
    /// 操作 ID。
    pub operation_id: String,
    /// 追加式决定记录 ID。
    pub resolution_record_id: String,
    /// 服务端派生状态。
    pub resulting_status: DirectReconciliationStatus,
    /// 是否已形成正式结论。
    pub is_terminal: bool,
    /// 服务端判定结果。
    pub outcome: IntegrationActionOutcome,
    /// 正式业务结果引用。
    pub business_result_reference: Option<String>,
}

fn validate_command_identity(
    work_item_id: &str,
    task_version: &str,
    subject_version: &str,
    idempotency_key: &str,
) -> Result<()> {
    required(work_item_id, "任务 ID", ID_MAX_LEN)?;
    decimal_version(task_version, false, "任务版本")?;
    required(subject_version, "业务主题版本", VERSION_MAX_LEN)?;
    required(idempotency_key, "幂等键", IDEMPOTENCY_KEY_MAX_LEN)
}

fn validate_action(action: &IntegrationNonTerminalTaskAction) -> Result<()> {
    required(&action.item_id, "业务项 ID", ID_MAX_LEN)?;
    required(&action.operation_id, "操作 ID", OPERATION_ID_MAX_LEN)?;
    optional(&action.reason_code, "原因代码", REASON_CODE_MAX_LEN)?;
    optional(&action.comment, "备注", COMMENT_MAX_LEN)?;
    validate_evidence_refs(&action.evidence_refs)
}

fn validate_evidence_refs(refs: &[ControlledEvidenceRef]) -> Result<()> {
    if refs.len() > EVIDENCE_REF_MAX_COUNT {
        return Err(Error::ValidationError("证据引用数量超过上限".to_string()));
    }
    for evidence in refs {
        required(&evidence.record_id, "证据记录 ID", ID_MAX_LEN)?;
        required(&evidence.label, "证据标签", ID_MAX_LEN)?;
    }
    Ok(())
}

fn decimal_version(value: &str, allow_zero: bool, label: &str) -> Result<u64> {
    required(value, label, VERSION_MAX_LEN)?;
    let version = value
        .trim()
        .parse::<u64>()
        .map_err(|_| Error::ValidationError(format!("{label}必须为十进制整数字符串")))?;
    if !allow_zero && version == 0 {
        return Err(Error::ValidationError(format!("{label}必须大于 0")));
    }
    Ok(version)
}

fn required(value: &str, label: &str, max: usize) -> Result<()> {
    let value = value.trim();
    if value.is_empty() {
        return Err(Error::ValidationError(format!("{label}不能为空")));
    }
    if value.len() > max {
        return Err(Error::ValidationError(format!("{label}过长")));
    }
    Ok(())
}

fn optional(value: &Option<String>, label: &str, max: usize) -> Result<()> {
    if let Some(value) = value {
        if value.trim().is_empty() {
            return Err(Error::ValidationError(format!("{label}不能为空白")));
        }
        if value.len() > max {
            return Err(Error::ValidationError(format!("{label}过长")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{DirectReconciliationCommand, IntegrationTaskActionCommand, IntegrationTaskActionKind};

    #[test]
    fn task_action_rejects_raw_original_idempotency_key() {
        let value = json!({
            "work_item_id": "wi-1",
            "expected_task_version": "1",
            "expected_subject_version": "1",
            "action": {
                "item_type": "ERROR_TASK",
                "item_id": "task-1",
                "kind": "REPLAY_ORIGINAL",
                "operation_id": "op-1",
                "original_action_idempotency_key": "must-not-cross-boundary"
            },
            "idempotency_key": "request-1"
        });

        assert!(serde_json::from_value::<IntegrationTaskActionCommand>(value).is_err());
    }

    #[test]
    fn direct_decision_uses_decision_only_shape() {
        let command: DirectReconciliationCommand = serde_json::from_value(json!({
            "difference_id": "diff-1",
            "expected_difference_version": "0",
            "decision": {
                "kind": "NON_TERMINAL_ACTION",
                "action": "QUERY_ORIGINAL_RESULT",
                "comment": "query"
            },
            "operation_id": "op-1",
            "idempotency_key": "request-1"
        }))
        .unwrap();

        command.validate().unwrap();
        assert!(matches!(
            command.decision,
            super::DirectReconciliationDecision::NonTerminalAction {
                action: IntegrationTaskActionKind::QueryOriginalResult,
                ..
            }
        ));
    }
}
