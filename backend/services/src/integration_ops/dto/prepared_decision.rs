//! W29 强命令的规范化 Prepared DTO（INT-E22）。
//!
//! wire 契约保持字符串版本；本模块经 `TryFrom` 一次性完成边界校验、去首尾空白与
//! 十进制解析，持有 typed `u64` 版本与规范化字段。Service 只消费 typed 结果，
//! 不得对版本字符串二次解析。

use super::task_decision::{
    decimal_version, DirectReconciliationCommand, DirectReconciliationConclusion,
    IntegrationNonTerminalTaskAction, IntegrationTaskActionCommand, IntegrationTaskActionKind,
    IntegrationTaskCompletionCommand,
};
use crate::errors::Result;

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
    /// 从原始字段解析规范化目标身份与版本。
    ///
    /// # 参数
    /// * `work_item_id` - 原始正式任务 ID（允许首尾空白）
    /// * `task_version` - 原始任务版本字符串（十进制，不允许为 0）
    /// * `subject_version` - 原始业务主题版本（允许首尾空白）
    ///
    /// # 返回
    /// 返回持有 typed 版本与规范化字段的 `PreparedWorkItemTarget`。
    ///
    /// # 错误
    /// 任务版本非十进制整数、为 0 或超出 `u64` 上限时返回 `ValidationError`。
    ///
    /// # 约束
    /// 解析规则与 [`decimal_version`] 同源；wire 仍保持字符串合同。
    fn parse(work_item_id: &str, task_version: &str, subject_version: &str) -> Result<Self> {
        let task_version = decimal_version(task_version, false, "任务版本")?;
        Ok(Self {
            work_item_id: work_item_id.trim().to_string(),
            task_version,
            subject_version: subject_version.trim().to_string(),
        })
    }
}

impl TryFrom<&IntegrationTaskActionCommand> for PreparedWorkItemTarget {
    type Error = crate::errors::Error;

    /// 从任务动作命令生成规范化目标。
    ///
    /// # 参数
    /// * `command` - 已反序列化的任务动作命令
    ///
    /// # 返回
    /// 返回持有 typed 任务版本与规范化身份的目标。
    ///
    /// # 错误
    /// 命令边界校验失败，或任务版本为 0、非十进制、溢出时返回 `ValidationError`。
    ///
    /// # 约束
    /// 先执行命令自校验，再单次解析版本；Service 不得二次解析。
    fn try_from(command: &IntegrationTaskActionCommand) -> Result<Self> {
        command.validate()?;
        Self::parse(
            &command.work_item_id,
            &command.expected_task_version,
            &command.expected_subject_version,
        )
    }
}

impl TryFrom<&IntegrationTaskCompletionCommand> for PreparedWorkItemTarget {
    type Error = crate::errors::Error;

    /// 从任务完成命令生成规范化目标。
    ///
    /// # 参数
    /// * `command` - 已反序列化的任务完成命令
    ///
    /// # 返回
    /// 返回持有 typed 任务版本与规范化身份的目标。
    ///
    /// # 错误
    /// 命令边界校验失败，或任务版本为 0、非十进制、溢出时返回 `ValidationError`。
    ///
    /// # 约束
    /// 先执行命令自校验，再单次解析版本；Service 不得二次解析。
    fn try_from(command: &IntegrationTaskCompletionCommand) -> Result<Self> {
        command.validate()?;
        Self::parse(
            &command.work_item_id,
            &command.expected_task_version,
            &command.expected_subject_version,
        )
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
    type Error = crate::errors::Error;

    /// 从直接对账命令生成规范化目标。
    ///
    /// # 参数
    /// * `command` - 已反序列化的直接对账命令
    ///
    /// # 返回
    /// 返回持有 typed 差异版本与规范化身份的目标。
    ///
    /// # 错误
    /// 命令边界校验失败，或差异版本非十进制、溢出时返回 `ValidationError`。
    ///
    /// # 约束
    /// 先执行命令自校验，再单次解析版本；Service 不得二次解析。
    fn try_from(command: &DirectReconciliationCommand) -> Result<Self> {
        command.validate()?;
        let difference_version = decimal_version(&command.expected_difference_version, true, "差异版本")?;
        Ok(Self {
            difference_id: command.difference_id.trim().to_string(),
            difference_version,
        })
    }
}

impl IntegrationTaskCompletionCommand {
    /// 将任务完成决定适配为非终结任务动作（用于任务绑定与责任校验）。
    ///
    /// # 返回
    /// 返回携带完成决定业务身份的 `AddEvidence` 动作；原因码取完成原因，
    /// 备注与证据引用原样透传。
    ///
    /// # 约束
    /// 适配形状由 Prepared DTO 拥有，Service 不再维护镜像转换。
    pub(crate) fn as_non_terminal_action(&self) -> IntegrationNonTerminalTaskAction {
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
impl From<DirectReconciliationConclusion> for entities::integration_ops::DirectConclusion {
    /// 将服务 DTO 结论转换为领域终态结论。
    ///
    /// # 参数
    /// * `conclusion` - 服务 DTO 的直接对账终态结论
    ///
    /// # 返回
    /// 返回一一对应的领域终态结论；wire 代码仍由服务 DTO 拥有。
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
        use entities::integration_ops::{DirectConclusion, ResolutionAction};

        let typed = DirectConclusion::from(DirectReconciliationConclusion::ConfirmNoError);
        assert_eq!(typed.resolution_action(), ResolutionAction::ConfirmNoError);
        let typed = DirectConclusion::from(DirectReconciliationConclusion::ConfirmValidDifference);
        assert_eq!(
            typed.resolution_action(),
            ResolutionAction::ConfirmValidDifference
        );
    }
}
