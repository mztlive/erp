//! W29 终态证据策略、原因注册表与动作推导（领域类型，INT-E21）。
//!
//! 固定策略身份、资金影响、证据类型要求与动作/阻断推导属于领域政策，
//! 由本模块独占并返回 typed 结果；RBAC 执行与响应 view 映射仍归服务；
//! ERP 错误分类与财务影响不得进入通用 BPM。

use super::{DirectConclusion, IntegrationErrorTask, ReconciliationDifference};

/// 错误任务终态证据策略 ID。
pub const ERROR_POLICY_ID: &str = "w29-error-terminal-evidence";
/// 对账差异终态证据策略 ID。
pub const DIFFERENCE_POLICY_ID: &str = "w29-difference-terminal-evidence";
/// 证据策略版本。
pub const EVIDENCE_POLICY_VERSION: u64 = 1;
/// 对账原因注册表 ID。
pub const REASON_REGISTRY_ID: &str = "w29-reconciliation-reasons";
/// 对账原因注册表版本。
pub const REASON_REGISTRY_VERSION: u64 = 1;

/// 终态证据类型（领域词汇；wire 代码映射归服务）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequiredEvidenceKind {
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

/// 资金影响（领域词汇）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FundsImpact {
    /// 无资金影响。
    None,
    /// 潜在资金影响。
    Potential,
}

impl FundsImpact {
    /// 返回资金影响的稳定代码。
    ///
    /// # 返回
    /// 无影响返回 `NONE`，潜在影响返回 `POTENTIAL`。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::Potential => "POTENTIAL",
        }
    }
}

/// 终态证据策略（typed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TerminalEvidencePolicy {
    /// 策略稳定 ID。
    pub policy_id: &'static str,
    /// 策略版本。
    pub version: u64,
    /// 策略错误类型（错误分类代码或差异分类）。
    pub error_type: String,
    /// 资金影响。
    pub funds_impact: FundsImpact,
    /// 完成任务所需的全部证据类型。
    pub required: &'static [RequiredEvidenceKind],
}

impl TerminalEvidencePolicy {
    /// 判断已发现证据是否满足策略的类型集合。
    ///
    /// # 参数
    /// * `present` - 已发现的证据类型集合
    ///
    /// # 返回
    /// 全部必需类型均已发现时返回 `true`。
    pub fn satisfied_by(&self, present: &[RequiredEvidenceKind]) -> bool {
        self.required.iter().all(|kind| present.contains(kind))
    }
}

/// 错误任务终态证据策略：有关联消息取外部结果，否则取业务对象核验。
///
/// # 参数
/// * `task` - 集成错误任务
///
/// # 返回
/// 返回 typed 终态证据策略；资金影响恒为无。
pub fn error_terminal_policy(task: &IntegrationErrorTask) -> TerminalEvidencePolicy {
    TerminalEvidencePolicy {
        policy_id: ERROR_POLICY_ID,
        version: EVIDENCE_POLICY_VERSION,
        error_type: task.error_class.as_str().to_string(),
        funds_impact: FundsImpact::None,
        required: if task.message_id.is_some() {
            &[RequiredEvidenceKind::ExternalCaseResult]
        } else {
            &[RequiredEvidenceKind::BusinessObjectVerification]
        },
    }
}

/// 对账差异终态证据策略：资金影响取补偿与财务对账，否则取业务对象核验。
///
/// # 参数
/// * `difference` - 对账差异
///
/// # 返回
/// 返回 typed 终态证据策略。
pub fn difference_terminal_policy(difference: &ReconciliationDifference) -> TerminalEvidencePolicy {
    let financial = difference.has_financial_impact();
    TerminalEvidencePolicy {
        policy_id: DIFFERENCE_POLICY_ID,
        version: EVIDENCE_POLICY_VERSION,
        error_type: difference.difference_type.clone(),
        funds_impact: if financial {
            FundsImpact::Potential
        } else {
            FundsImpact::None
        },
        required: if financial {
            &[
                RequiredEvidenceKind::CompensationResult,
                RequiredEvidenceKind::FinancialReconciliation,
            ]
        } else {
            &[RequiredEvidenceKind::BusinessObjectVerification]
        },
    }
}

/// 注册对账原因（typed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegisteredReason {
    /// 注册原因稳定 ID。
    pub id: &'static str,
    /// 注册原因版本。
    pub version: u64,
    /// 唯一允许的结论。
    pub conclusion: DirectConclusion,
    /// 展示标签。
    pub label: &'static str,
    /// 形成结论所需的全部证据类型。
    pub required: &'static [RequiredEvidenceKind],
}

/// 无正式任务差异可用的固定原因注册表（typed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReasonRegistry {
    /// 注册表稳定 ID。
    pub id: &'static str,
    /// 注册表版本。
    pub version: u64,
    /// 注册原因列表。
    pub reasons: Vec<RegisteredReason>,
}

impl ReasonRegistry {
    /// 按注册原因 ID 查找原因。
    ///
    /// # 参数
    /// * `reason_id` - 注册原因稳定 ID
    ///
    /// # 返回
    /// 命中时返回注册原因，否则返回 `None`。
    pub fn find(&self, reason_id: &str) -> Option<&RegisteredReason> {
        self.reasons.iter().find(|reason| reason.id == reason_id)
    }
}

/// 返回无任务直接对账固定原因注册表。
///
/// # 返回
/// 返回来源更正、业务确认无误、补偿闭环三项注册原因。
pub fn reconciliation_reason_registry() -> ReasonRegistry {
    ReasonRegistry {
        id: REASON_REGISTRY_ID,
        version: REASON_REGISTRY_VERSION,
        reasons: vec![
            RegisteredReason {
                id: "SOURCE_CORRECTED_AND_REATTRIBUTED",
                version: REASON_REGISTRY_VERSION,
                conclusion: DirectConclusion::ConfirmValidDifference,
                label: "来源已更正并重新归集",
                required: &[RequiredEvidenceKind::BusinessObjectVerification],
            },
            RegisteredReason {
                id: "BUSINESS_CONFIRMED_NO_ERROR",
                version: REASON_REGISTRY_VERSION,
                conclusion: DirectConclusion::ConfirmNoError,
                label: "业务确认无误",
                required: &[
                    RequiredEvidenceKind::BusinessObjectVerification,
                    RequiredEvidenceKind::DistinctReview,
                ],
            },
            RegisteredReason {
                id: "COMPENSATION_CLOSED",
                version: REASON_REGISTRY_VERSION,
                conclusion: DirectConclusion::ConfirmValidDifference,
                label: "补偿已闭环",
                required: &[
                    RequiredEvidenceKind::CompensationResult,
                    RequiredEvidenceKind::FinancialReconciliation,
                ],
            },
        ],
    }
}

/// W29 固定动作（领域词汇）。
///
/// 阻断引用但不开放的动作用 `Process` 表达（仅出现在阻断中）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecidedAction {
    /// 查询原结果。
    QueryOriginalResult,
    /// 追加证据。
    AddEvidence,
    /// 重放原动作。
    ReplayOriginal,
    /// 重新归集。
    Reattribute,
    /// 关联正式补偿。
    LinkCompensation,
    /// 形成正式解决结论。
    Resolve,
    /// 确认无误。
    ConfirmNoError,
    /// 确认有效差异。
    ConfirmValidDifference,
    /// 处理错误（仅阻断引用，不开放）。
    Process,
}

impl DecidedAction {
    /// 返回动作的稳定代码。
    ///
    /// # 返回
    /// 返回服务 view 沿用的稳定动作代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::QueryOriginalResult => "QUERY_ORIGINAL_RESULT",
            Self::AddEvidence => "ADD_EVIDENCE",
            Self::ReplayOriginal => "REPLAY_ORIGINAL",
            Self::Reattribute => "REATTRIBUTE",
            Self::LinkCompensation => "LINK_COMPENSATION",
            Self::Resolve => "RESOLVE",
            Self::ConfirmNoError => "CONFIRM_NO_ERROR",
            Self::ConfirmValidDifference => "CONFIRM_VALID_DIFFERENCE",
            Self::Process => "PROCESS",
        }
    }
}

/// 动作阻断（typed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionBlocker {
    /// 被阻断的动作。
    pub action: DecidedAction,
    /// 阻断稳定代码。
    pub code: &'static str,
    /// 阻断说明。
    pub message: &'static str,
}

/// 错误任务动作推导输入（typed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ErrorActionProjection {
    /// 任务是否已终态。
    pub terminal: bool,
    /// 是否已建立正式任务。
    pub has_work_item: bool,
    /// 当前任务是否满足重放前置条件。
    pub can_replay: bool,
    /// 已发现的证据类型。
    pub present: Vec<RequiredEvidenceKind>,
    /// 当前固定终态证据策略。
    pub policy: TerminalEvidencePolicy,
}

/// 推导错误任务开放动作与阻断。
///
/// # 参数
/// * `input` - 终态、任务关联、重放条件、已发现证据与固定策略
///
/// # 返回
/// 返回开放动作与阻断；终态返回双空，无任务时只返回缺责任阻断。
pub fn project_error_actions(input: ErrorActionProjection) -> (Vec<DecidedAction>, Vec<ActionBlocker>) {
    if input.terminal {
        return (Vec::new(), Vec::new());
    }
    if !input.has_work_item {
        return (
            Vec::new(),
            vec![ActionBlocker {
                action: DecidedAction::Process,
                code: "FORMAL_WORK_ITEM_MISSING",
                message: "尚未建立 W29 处理责任，当前错误只能查看。",
            }],
        );
    }
    let mut actions = vec![DecidedAction::QueryOriginalResult, DecidedAction::AddEvidence];
    if input.can_replay {
        actions.push(DecidedAction::ReplayOriginal);
    }
    if input
        .present
        .contains(&RequiredEvidenceKind::BusinessObjectVerification)
    {
        actions.push(DecidedAction::Reattribute);
    }
    if input.present.contains(&RequiredEvidenceKind::CompensationResult) {
        actions.push(DecidedAction::LinkCompensation);
    }
    if input.policy.satisfied_by(&input.present) {
        actions.push(DecidedAction::Resolve);
        return (actions, Vec::new());
    }
    (
        actions,
        vec![ActionBlocker {
            action: DecidedAction::Resolve,
            code: "VERIFIED_RESULT_REQUIRED",
            message: "取得可验证结果后才能完成任务。",
        }],
    )
}

/// 对账差异动作推导输入（typed）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DifferenceActionProjection {
    /// 差异是否已形成正式结论。
    pub terminal: bool,
    /// 是否已建立正式任务。
    pub has_work_item: bool,
    /// 已发现的证据类型。
    pub present: Vec<RequiredEvidenceKind>,
    /// 当前固定终态证据策略。
    pub policy: TerminalEvidencePolicy,
}

/// 推导对账差异开放动作与阻断。
///
/// # 参数
/// * `input` - 终态、任务关联、已发现证据与固定策略
///
/// # 返回
/// 返回开放动作与阻断；终态返回双空，无任务时开放直接对账结论动作。
pub fn project_difference_actions(
    input: DifferenceActionProjection,
) -> (Vec<DecidedAction>, Vec<ActionBlocker>) {
    if input.terminal {
        return (Vec::new(), Vec::new());
    }
    let mut actions = vec![DecidedAction::QueryOriginalResult, DecidedAction::AddEvidence];
    if input
        .present
        .contains(&RequiredEvidenceKind::BusinessObjectVerification)
    {
        actions.push(DecidedAction::Reattribute);
    }
    if input.present.contains(&RequiredEvidenceKind::CompensationResult) {
        actions.push(DecidedAction::LinkCompensation);
    }
    if input.has_work_item {
        if input.policy.satisfied_by(&input.present) {
            actions.push(DecidedAction::Resolve);
            return (actions, Vec::new());
        }
        return (
            actions,
            vec![ActionBlocker {
                action: DecidedAction::Resolve,
                code: "VERIFIED_EVIDENCE_REQUIRED",
                message: "终态证据尚未满足当前固定策略。",
            }],
        );
    }
    actions.push(DecidedAction::ConfirmNoError);
    actions.push(DecidedAction::ConfirmValidDifference);
    (actions, Vec::new())
}

/// 动作后下一动作推导的业务项类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionSubject {
    /// 集成错误任务。
    ErrorTask,
    /// 对账差异。
    ReconciliationDifference,
}

/// 动作后下一动作推导的结果类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProjectionOutcome {
    /// 已找到可验证终态证据。
    TerminalEvidenceFound,
    /// 已明确确认原动作无结果。
    NoResultConfirmed,
    /// 其他结果。
    Other,
}

/// 推导单次非终结动作后的下一开放动作。
///
/// # 参数
/// * `subject` - 业务项类型
/// * `outcome` - 本次动作结果类型
///
/// # 返回
/// 返回查询与补证基础动作，及满足条件时开放的重放或完成动作。
pub fn next_actions_after_outcome(
    subject: ProjectionSubject,
    outcome: ProjectionOutcome,
) -> Vec<DecidedAction> {
    let mut actions = vec![DecidedAction::QueryOriginalResult, DecidedAction::AddEvidence];
    if subject == ProjectionSubject::ErrorTask && outcome == ProjectionOutcome::NoResultConfirmed {
        actions.push(DecidedAction::ReplayOriginal);
    }
    if outcome == ProjectionOutcome::TerminalEvidenceFound {
        actions.push(DecidedAction::Resolve);
    }
    actions
}

#[cfg(test)]
mod tests {
    use super::{
        difference_terminal_policy, error_terminal_policy, next_actions_after_outcome,
        project_difference_actions, project_error_actions, reconciliation_reason_registry,
        DifferenceActionProjection, DirectConclusion, ErrorActionProjection, FundsImpact, ProjectionOutcome,
        ProjectionSubject, RequiredEvidenceKind,
    };
    use crate::ids::{InboxMessageId, IntegrationErrorTaskId, ReconciliationDifferenceId};
    use crate::integration_ops::{
        ErrorClass, IntegrationErrorTask, IntegrationErrorTaskData, ReconciliationDifference,
        ReconciliationDifferenceData,
    };

    fn error_task(with_message: bool) -> IntegrationErrorTask {
        IntegrationErrorTask::new(
            IntegrationErrorTaskId::new("task-1"),
            IntegrationErrorTaskData {
                message_id: with_message.then(|| InboxMessageId::new("msg-1")),
                business_object_id: (!with_message).then(|| "so-1".to_string()),
                error_class: ErrorClass::TransientFailure,
                owner_role: None,
                owner_user_id: None,
            },
        )
        .unwrap()
    }

    fn difference(difference_type: &str) -> ReconciliationDifference {
        ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-1"),
            ReconciliationDifferenceData {
                business_object_type: "mall_order".to_string(),
                business_object_id: "MO-1".to_string(),
                difference_type: difference_type.to_string(),
                left_fact_reference: Some("mall_order_fact://f-1".to_string()),
                right_fact_reference: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn error_policy_matrix_covers_message_and_business_tasks() {
        let external = error_terminal_policy(&error_task(true));
        assert_eq!(external.policy_id, "w29-error-terminal-evidence");
        assert_eq!(external.version, 1);
        assert_eq!(external.error_type, "transient_failure");
        assert_eq!(external.funds_impact, FundsImpact::None);
        assert_eq!(external.required, &[RequiredEvidenceKind::ExternalCaseResult]);

        let repair = error_terminal_policy(&error_task(false));
        assert_eq!(
            repair.required,
            &[RequiredEvidenceKind::BusinessObjectVerification]
        );
    }

    #[test]
    fn difference_policy_matrix_covers_financial_impact() {
        let financial = difference_terminal_policy(&difference("amount_mismatch"));
        assert_eq!(financial.funds_impact, FundsImpact::Potential);
        assert_eq!(
            financial.required,
            &[
                RequiredEvidenceKind::CompensationResult,
                RequiredEvidenceKind::FinancialReconciliation
            ]
        );

        let operational = difference_terminal_policy(&difference("status_difference"));
        assert_eq!(operational.funds_impact, FundsImpact::None);
        assert_eq!(
            operational.required,
            &[RequiredEvidenceKind::BusinessObjectVerification]
        );
        assert!(!financial.satisfied_by(&[] as &[RequiredEvidenceKind]));
    }

    #[test]
    fn reason_registry_holds_three_registered_reasons() {
        let registry = reconciliation_reason_registry();
        assert_eq!(registry.id, "w29-reconciliation-reasons");
        assert_eq!(registry.version, 1);
        assert_eq!(registry.reasons.len(), 3);

        let corrected = registry.find("SOURCE_CORRECTED_AND_REATTRIBUTED").unwrap();
        assert_eq!(corrected.conclusion, DirectConclusion::ConfirmValidDifference);
        assert!(registry.find("free_form_reason").is_none());
    }

    #[test]
    fn error_projection_covers_terminal_no_task_and_evidence_states() {
        let policy = error_terminal_policy(&error_task(true));

        let (actions, blockers) = project_error_actions(ErrorActionProjection {
            terminal: true,
            has_work_item: true,
            can_replay: false,
            present: vec![],
            policy: policy.clone(),
        });
        assert!(actions.is_empty() && blockers.is_empty());

        let (actions, blockers) = project_error_actions(ErrorActionProjection {
            terminal: false,
            has_work_item: false,
            can_replay: false,
            present: vec![],
            policy: policy.clone(),
        });
        assert!(actions.is_empty());
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].code, "FORMAL_WORK_ITEM_MISSING");

        let (actions, blockers) = project_error_actions(ErrorActionProjection {
            terminal: false,
            has_work_item: true,
            can_replay: true,
            present: vec![RequiredEvidenceKind::ExternalCaseResult],
            policy,
        });
        assert!(actions.iter().any(|action| action.as_str() == "REPLAY_ORIGINAL"));
        assert!(actions.iter().any(|action| action.as_str() == "RESOLVE"));
        assert!(blockers.is_empty());
    }

    #[test]
    fn difference_projection_covers_task_and_direct_states() {
        let policy = difference_terminal_policy(&difference("status_difference"));

        let (actions, blockers) = project_difference_actions(DifferenceActionProjection {
            terminal: false,
            has_work_item: false,
            present: vec![],
            policy: policy.clone(),
        });
        assert!(actions.iter().any(|action| action.as_str() == "CONFIRM_NO_ERROR"));
        assert!(actions
            .iter()
            .any(|action| action.as_str() == "CONFIRM_VALID_DIFFERENCE"));
        assert!(blockers.is_empty());

        let (actions, blockers) = project_difference_actions(DifferenceActionProjection {
            terminal: false,
            has_work_item: true,
            present: vec![],
            policy,
        });
        assert!(!actions.iter().any(|action| action.as_str() == "RESOLVE"));
        assert_eq!(blockers.len(), 1);
        assert_eq!(blockers[0].code, "VERIFIED_EVIDENCE_REQUIRED");
    }

    #[test]
    fn next_actions_cover_replay_and_resolve_hints() {
        let actions =
            next_actions_after_outcome(ProjectionSubject::ErrorTask, ProjectionOutcome::NoResultConfirmed);
        assert!(actions.iter().any(|action| action.as_str() == "REPLAY_ORIGINAL"));

        let actions = next_actions_after_outcome(
            ProjectionSubject::ReconciliationDifference,
            ProjectionOutcome::NoResultConfirmed,
        );
        assert!(!actions.iter().any(|action| action.as_str() == "REPLAY_ORIGINAL"));

        let actions = next_actions_after_outcome(
            ProjectionSubject::ErrorTask,
            ProjectionOutcome::TerminalEvidenceFound,
        );
        assert!(actions.iter().any(|action| action.as_str() == "RESOLVE"));
    }
}
