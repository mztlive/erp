//! W29 强命令的规范化 Prepared DTO（INT-E22）。
//!
//! wire 契约保持字符串版本；本模块经 `TryFrom` 一次性完成边界校验、去首尾空白与
//! 十进制解析，持有 typed `u64` 版本与规范化字段。Service 只消费 typed 结果，
//! 不得对版本字符串二次解析。

use super::task_decision::{
    DirectReconciliationCommand, DirectReconciliationConclusion, IntegrationNonTerminalTaskAction,
    IntegrationTaskActionCommand, IntegrationTaskActionKind, IntegrationTaskCompletionCommand,
};
use crate::Result;

/// W29 任务命令共用的规范化目标身份与版本。
///
/// 由任务动作命令与任务完成命令经 `TryFrom` 生成；任务版本为已解析的 `u64`，
/// 任务 ID 与业务主题版本已去首尾空白。幂等键与操作 ID 不在此规范化，
/// 完整命令原文仍参与指纹计算，指纹语义不变。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWorkItemTarget {
    /// 去首尾空白后的正式任务 ID。
    pub work_item_id: String,
    /// 已解析的任务版本（十进制 `u64`，不允许为 0）。
    pub task_version: u64,
    /// 去首尾空白后的业务主题版本。
    pub subject_version: String,
}

impl PreparedWorkItemTarget {
    // 从已校验字段与解析后的版本构造规范化目标。
    //
    // 仅规范化身份字段，不重复解析已校验版本；wire 仍保持字符串合同。
    //
    // 返回持有 typed 版本与规范化字段的 `PreparedWorkItemTarget`。版本约束由命令边界校验保证。
    //
    // - `work_item_id`：原始正式任务 ID（允许首尾空白）。
    // - `subject_version`：原始业务主题版本（允许首尾空白）。
    fn new(work_item_id: &str, task_version: u64, subject_version: &str) -> Self {
        Self {
            work_item_id: work_item_id.trim().to_string(),
            task_version,
            subject_version: subject_version.trim().to_string(),
        }
    }
}

impl TryFrom<&IntegrationTaskActionCommand> for PreparedWorkItemTarget {
    type Error = crate::Error;

    // 从任务动作命令生成规范化目标。
    //
    // 命令边界校验保留解析后的版本；Service 不得二次解析。
    //
    // 返回持有 typed 任务版本与规范化身份的目标。命令边界校验失败，或任务版本为 0、非十进制、溢出时返回 `ValidationError`。
    fn try_from(command: &IntegrationTaskActionCommand) -> Result<Self> {
        let version = command.validated_version()?;
        Ok(Self::new(&command.work_item_id, version, &command.expected_subject_version))
    }
}

impl TryFrom<&IntegrationTaskCompletionCommand> for PreparedWorkItemTarget {
    type Error = crate::Error;

    // 从任务完成命令生成规范化目标。
    //
    // 命令边界校验保留解析后的版本；Service 不得二次解析。
    //
    // 返回持有 typed 任务版本与规范化身份的目标。命令边界校验失败，或任务版本为 0、非十进制、溢出时返回 `ValidationError`。
    fn try_from(command: &IntegrationTaskCompletionCommand) -> Result<Self> {
        let version = command.validated_version()?;
        Ok(Self::new(&command.work_item_id, version, &command.expected_subject_version))
    }
}

/// 直接对账命令的规范化目标身份与版本。
///
/// 差异版本允许为 0（尚无决定）；解析规则与差异决定版本检查同源。
/// 幂等键与操作 ID 不在此规范化，完整命令原文仍参与指纹计算。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedDirectDecisionTarget {
    /// 去首尾空白后的差异 ID。
    pub difference_id: String,
    /// 已解析的差异版本（十进制 `u64`，允许为 0）。
    pub difference_version: u64,
}

impl TryFrom<&DirectReconciliationCommand> for PreparedDirectDecisionTarget {
    type Error = crate::Error;

    // 从直接对账命令生成规范化目标。
    //
    // 命令边界校验保留解析后的版本；Service 不得二次解析。
    //
    // 返回持有 typed 差异版本与规范化身份的目标。命令边界校验失败，或差异版本非十进制、溢出时返回 `ValidationError`。
    fn try_from(command: &DirectReconciliationCommand) -> Result<Self> {
        let difference_version = command.validated_version()?;
        Ok(Self { difference_id: command.difference_id.trim().to_string(), difference_version })
    }
}

impl IntegrationTaskCompletionCommand {
    /// 将任务完成决定适配为非终结任务动作（用于任务绑定与责任校验）。
    ///
    /// 适配形状由 Prepared DTO 拥有，Service 不再维护镜像转换。
    ///
    /// 返回携带完成决定业务身份的 `AddEvidence` 动作；原因码取完成原因，备注与证据引用原样透传。
    pub fn as_non_terminal_action(&self) -> IntegrationNonTerminalTaskAction {
        IntegrationNonTerminalTaskAction {
            item_type: self.decision.item_type,
            item_id: self.decision.item_id.clone(),
            kind: IntegrationTaskActionKind::AddEvidence,
            operation_id: self.decision.operation_id.clone(),
            reason_code: Some(self.decision.reason_code.as_str().to_string()),
            comment: self.decision.comment.clone(),
            evidence_refs: self.decision.evidence_refs.clone(),
        }
    }
}

/// 从服务 DTO 结论转换领域终态结论（机械映射，动作与状态派生归领域）。
impl From<DirectReconciliationConclusion> for crate::entity::integration_ops::DirectConclusion {
    // 将服务 DTO 结论转换为领域终态结论。返回一一对应的领域终态结论；wire 代码仍由服务 DTO 拥有。
    fn from(conclusion: DirectReconciliationConclusion) -> Self {
        match conclusion {
            DirectReconciliationConclusion::ConfirmNoError => Self::ConfirmNoError,
            DirectReconciliationConclusion::ConfirmValidDifference => Self::ConfirmValidDifference,
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::task_decision::{
        DirectReconciliationCommand, IntegrationTaskActionCommand, IntegrationTaskCompletionCommand,
    };
    use super::{PreparedDirectDecisionTarget, PreparedWorkItemTarget};
    use crate::Error;

    fn action_command(version: &str) -> IntegrationTaskActionCommand {
        serde_json::from_value(json!({
            "work_item_id": " wi-1 ",
            "expected_task_version": version,
            "expected_subject_version": " 3 ",
            "action": {
                "item_type": "ERROR_TASK",
                "item_id": "task-1",
                "kind": "QUERY_ORIGINAL_RESULT",
                "operation_id": "op-1"
            },
            "idempotency_key": " request-1 "
        }))
        .unwrap()
    }

    fn completion_command(version: &str) -> IntegrationTaskCompletionCommand {
        serde_json::from_value(json!({
            "work_item_id": "wi-9",
            "expected_task_version": version,
            "expected_subject_version": "2",
            "decision": {
                "item_type": "ERROR_TASK",
                "item_id": "task-9",
                "kind": "RESOLVE",
                "operation_id": "op-9",
                "reason_code": "TERMINAL_EVIDENCE_VERIFIED",
                "evidence_policy_id": "w29-error-terminal-evidence",
                "evidence_policy_version": 1,
                "policy_key": {"error_type": "transient_failure", "funds_impact": "NONE"},
                "evidence_refs": [
                    {"kind": "EXTERNAL_CASE_RESULT", "record_id": "case-1", "label": "外部结果"}
                ]
            },
            "idempotency_key": "request-9"
        }))
        .unwrap()
    }

    fn direct_command(version: &str) -> DirectReconciliationCommand {
        serde_json::from_value(json!({
            "difference_id": " diff-1 ",
            "expected_difference_version": version,
            "decision": {
                "kind": "NON_TERMINAL_ACTION",
                "action": "QUERY_ORIGINAL_RESULT",
                "comment": "query"
            },
            "operation_id": " op-2 ",
            "idempotency_key": " request-2 "
        }))
        .unwrap()
    }

    #[test]
    fn task_target_trims_fields_and_holds_typed_version() {
        let prepared = PreparedWorkItemTarget::try_from(&action_command(" 7 ")).unwrap();
        assert_eq!(prepared.work_item_id, "wi-1");
        assert_eq!(prepared.task_version, 7);
        assert_eq!(prepared.subject_version, "3");

        let prepared = PreparedWorkItemTarget::try_from(&completion_command("2")).unwrap();
        assert_eq!(prepared.task_version, 2);
    }

    #[test]
    fn task_target_rejects_zero_non_decimal_and_overflow() {
        assert!(PreparedWorkItemTarget::try_from(&action_command("0")).is_err());
        assert!(PreparedWorkItemTarget::try_from(&action_command("bad")).is_err());
        assert!(PreparedWorkItemTarget::try_from(&action_command("1.5")).is_err());
        assert!(PreparedWorkItemTarget::try_from(&action_command("-1")).is_err());
        assert!(PreparedWorkItemTarget::try_from(&action_command("18446744073709551616")).is_err());
    }

    #[test]
    fn task_target_accepts_max_u64() {
        let prepared = PreparedWorkItemTarget::try_from(&action_command("18446744073709551615")).unwrap();
        assert_eq!(prepared.task_version, u64::MAX);
    }

    #[test]
    fn direct_target_allows_zero_and_trims() {
        let prepared = PreparedDirectDecisionTarget::try_from(&direct_command("0")).unwrap();
        assert_eq!(prepared.difference_id, "diff-1");
        assert_eq!(prepared.difference_version, 0);

        let prepared = PreparedDirectDecisionTarget::try_from(&direct_command(" 12 ")).unwrap();
        assert_eq!(prepared.difference_version, 12);
    }

    #[test]
    fn direct_target_rejects_non_decimal() {
        assert!(PreparedDirectDecisionTarget::try_from(&direct_command("bad")).is_err());
        assert!(PreparedDirectDecisionTarget::try_from(&direct_command("")).is_err());
        assert!(PreparedDirectDecisionTarget::try_from(&direct_command("18446744073709551616")).is_err());
    }

    #[test]
    fn prepared_targets_preserve_existing_integer_syntax() {
        let action = action_command(" +0007 ");
        action.validate().unwrap();
        assert_eq!(PreparedWorkItemTarget::try_from(&action).unwrap().task_version, 7);

        let completion = completion_command(" +0007 ");
        completion.validate().unwrap();
        assert_eq!(PreparedWorkItemTarget::try_from(&completion).unwrap().task_version, 7);

        let direct = direct_command(" +0000 ");
        direct.validate().unwrap();
        assert_eq!(PreparedDirectDecisionTarget::try_from(&direct).unwrap().difference_version, 0);
    }

    #[test]
    fn action_preparation_preserves_validation_error_priority() {
        let mut command = action_command("bad");
        command.work_item_id = " ".into();
        command.action.item_id = " ".into();

        for expected in ["任务 ID不能为空", "任务版本必须为十进制整数字符串", "业务项 ID不能为空"]
        {
            assert_validation_error(command.validate().unwrap_err(), expected);
            assert_validation_error(PreparedWorkItemTarget::try_from(&command).unwrap_err(), expected);
            if command.work_item_id.trim().is_empty() {
                command.work_item_id = "wi-1".into();
            } else {
                command.expected_task_version = "1".into();
            }
        }
    }

    #[test]
    fn completion_preparation_preserves_policy_before_empty_evidence_error() {
        let mut command = completion_command("bad");
        command.decision.evidence_policy_version = 0;
        command.decision.evidence_refs.clear();

        for expected in
            ["任务版本必须为十进制整数字符串", "证据策略版本必须大于 0", "完成任务必须提供终态证据引用"]
        {
            assert_validation_error(command.validate().unwrap_err(), expected);
            assert_validation_error(PreparedWorkItemTarget::try_from(&command).unwrap_err(), expected);
            if command.expected_task_version == "bad" {
                command.expected_task_version = "1".into();
            } else {
                command.decision.evidence_policy_version = 1;
            }
        }
    }

    #[test]
    fn direct_preparation_preserves_identity_before_version_error() {
        let mut command = direct_command("bad");
        command.difference_id = " ".into();
        command.operation_id = " ".into();

        for expected in ["差异 ID不能为空", "差异版本必须为十进制整数字符串", "操作 ID不能为空"]
        {
            assert_validation_error(command.validate().unwrap_err(), expected);
            assert_validation_error(PreparedDirectDecisionTarget::try_from(&command).unwrap_err(), expected);
            if command.difference_id.trim().is_empty() {
                command.difference_id = "diff-1".into();
            } else {
                command.expected_difference_version = "0".into();
            }
        }
    }

    fn assert_validation_error(error: Error, expected: &str) {
        assert!(matches!(error, Error::ValidationError(message) if message == expected));
    }

    #[test]
    fn completion_adapts_to_non_terminal_action() {
        use super::super::task_decision::IntegrationTaskActionKind;

        let action = completion_command("5").as_non_terminal_action();
        assert_eq!(action.kind, IntegrationTaskActionKind::AddEvidence);
        assert_eq!(action.operation_id, "op-9");
        assert_eq!(action.reason_code.as_deref(), Some("TERMINAL_EVIDENCE_VERIFIED"));
        assert_eq!(action.evidence_refs.len(), 1);
    }

    #[test]
    fn dto_conclusion_converts_to_typed_domain_conclusion() {
        use super::super::task_decision::DirectReconciliationConclusion;
        use crate::entity::integration_ops::{DirectConclusion, ResolutionAction};

        let typed = DirectConclusion::from(DirectReconciliationConclusion::ConfirmNoError);
        assert_eq!(typed.resolution_action(), ResolutionAction::ConfirmNoError);
        let typed = DirectConclusion::from(DirectReconciliationConclusion::ConfirmValidDifference);
        assert_eq!(typed.resolution_action(), ResolutionAction::ConfirmValidDifference);
    }
}
