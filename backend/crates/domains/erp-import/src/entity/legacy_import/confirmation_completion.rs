//! 导入确认的待决定资格及完成回放事实，不依赖任务或后续批次状态。

use crate::entity::command_receipt::ImportConfirmationOutcome;
use crate::{
    ConfirmationDecision, ConfirmationScope, ConfirmationStatus, LegacyImportConfirmation,
    PreparedConfirmationCompletion,
};

impl LegacyImportConfirmation {
    /// 判断待决定确认是否与规范化命令及已解析责任范围一致。
    ///
    /// # 参数
    /// * `command` - 当前规范化确认命令。
    /// * `scope` - 在原校验顺序中解析的责任范围。
    /// # 返回
    /// 任务、批次、范围、角色及待确认状态一致时返回 `true`。
    /// # 错误
    /// 不返回错误；不验证外部任务或批次事实。
    pub fn matches_pending_completion(
        &self,
        command: &PreparedConfirmationCompletion,
        scope: ConfirmationScope,
    ) -> bool {
        self.work_item_id == command.work_item_id
            && self.batch_id == command.batch_id
            && self.confirmation_scope == command.confirmation_scope
            && self.owner_role == scope.owner_role()
            && self.is_pending()
    }

    /// 用确认自身事实证明回执冻结的原决定。
    ///
    /// # 参数
    /// * `outcome` - 原命令冻结的正式结果。
    /// * `command` - 当前规范化原命令。
    /// * `actor_id` - 当前认证操作人。
    /// * `scope` - 已解析的责任范围。
    /// # 返回
    /// 确认身份、版本、决定、操作人与时间全部一致时返回 `true`。
    /// # 错误
    /// 不返回错误；不要求确认仍待处理，也不依赖后续批次推进。
    pub fn matches_completed_decision(
        &self,
        outcome: &ImportConfirmationOutcome,
        command: &PreparedConfirmationCompletion,
        actor_id: &str,
        scope: ConfirmationScope,
    ) -> bool {
        let expected_status = match command.decision {
            ConfirmationDecision::ConfirmScope => ConfirmationStatus::Confirmed,
            ConfirmationDecision::ReturnForFix => ConfirmationStatus::Rejected,
        };
        outcome.confirmation_id == self.base.id
            && outcome.confirmation_version == self.base.version
            && outcome.batch_id == command.batch_id.as_ref()
            && self.batch_id == command.batch_id
            && !self.base.is_deleted()
            && self.trial_version == command.expected_trial_version
            && self.owner_role == scope.owner_role()
            && Self::subject_version(self.batch_version, self.trial_version, &self.import_rule_version)
                == outcome.subject_version
            && self.work_item_id == command.work_item_id
            && outcome.subject_version == command.expected_subject_version
            && outcome.confirmation_scope == command.confirmation_scope
            && self.confirmation_scope == command.confirmation_scope
            && outcome.decision == command.decision
            && self.decision == Some(command.decision)
            && self.status == expected_status
            && self.decided_by.as_deref() == Some(actor_id)
            && self.decided_at == Some(outcome.decided_at)
            && self.reason_code == command.reason_code
            && self.comment == command.comment
            && Some(outcome.receipt.batch_version) == command.expected_batch_version.checked_add(1)
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{LegacyImportBatchId, LegacyImportConfirmationId, WorkItemId};

    use super::*;
    use crate::entity::command_receipt::ConfirmationCompletionReceipt;
    use crate::{
        ImportBusinessConfirmationNextStep, ImportBusinessConfirmationResultStatus,
        LegacyImportConfirmationData,
    };

    fn command() -> PreparedConfirmationCompletion {
        PreparedConfirmationCompletion {
            work_item_id: WorkItemId::new("task-1"),
            batch_id: LegacyImportBatchId::new("batch-1"),
            expected_task_version: 3,
            expected_subject_version: LegacyImportConfirmation::subject_version(1, 2, "rule-1"),
            expected_batch_version: 4,
            expected_trial_version: 2,
            confirmation_scope: "SALES".into(),
            decision: ConfirmationDecision::ConfirmScope,
            reason_code: None,
            comment: Some("确认".into()),
            idempotency_key: "request-1".into(),
        }
    }

    fn confirmation() -> LegacyImportConfirmation {
        LegacyImportConfirmation::new(
            LegacyImportConfirmationId::new("confirmation-1"),
            LegacyImportConfirmationData {
                batch_id: LegacyImportBatchId::new("batch-1"),
                confirmation_scope: "SALES".into(),
                owner_role: "role-sales".into(),
                batch_version: 1,
                trial_version: 2,
                import_rule_version: "rule-1".into(),
                work_item_id: WorkItemId::new("task-1"),
            },
        )
        .unwrap()
    }

    fn outcome(confirmation: &LegacyImportConfirmation) -> ImportConfirmationOutcome {
        ImportConfirmationOutcome {
            confirmation_id: confirmation.base.id.clone(),
            confirmation_version: confirmation.base.version,
            batch_id: confirmation.batch_id.to_string(),
            work_item_id: confirmation.work_item_id.to_string(),
            subject_version: LegacyImportConfirmation::subject_version(1, 2, "rule-1"),
            confirmation_scope: confirmation.confirmation_scope.clone(),
            decision: confirmation.decision.unwrap(),
            decided_at: confirmation.decided_at.unwrap(),
            receipt: ConfirmationCompletionReceipt {
                result_status: ImportBusinessConfirmationResultStatus::Confirmed,
                task_version: 4,
                batch_version: 5,
                next_step: ImportBusinessConfirmationNextStep::AwaitOtherConfirmations,
            },
        }
    }

    #[test]
    fn pending_completion_requires_own_task_batch_scope_role_and_pending_state() {
        let command = command();
        let fact = confirmation();
        assert!(fact.matches_pending_completion(&command, ConfirmationScope::Sales));
        for case in 0..5 {
            let mut wrong = fact.clone();
            match case {
                0 => wrong.work_item_id = WorkItemId::new("other"),
                1 => wrong.batch_id = LegacyImportBatchId::new("other"),
                2 => wrong.confirmation_scope = "FINANCE".into(),
                3 => wrong.owner_role = "role-finance".into(),
                _ => wrong.status = ConfirmationStatus::Confirmed,
            }
            assert!(!wrong.matches_pending_completion(&command, ConfirmationScope::Sales));
        }
    }

    #[test]
    fn completed_decision_proves_both_decisions_and_rejects_changed_original_facts() {
        for decision in [ConfirmationDecision::ConfirmScope, ConfirmationDecision::ReturnForFix] {
            let mut command = command();
            command.decision = decision;
            command.reason_code = (decision == ConfirmationDecision::ReturnForFix).then(|| "REWORK".into());
            let mut fact = confirmation();
            fact.decide(
                decision,
                "actor-1",
                Instant::from_unix_secs(20),
                command.reason_code.clone(),
                command.comment.clone(),
            )
            .unwrap();
            fact.base.version += 1;
            let outcome = outcome(&fact);
            assert!(fact.matches_completed_decision(&outcome, &command, "actor-1", ConfirmationScope::Sales));
            for case in 0..7 {
                let mut wrong = fact.clone();
                match case {
                    0 => wrong.base.version += 1,
                    1 => wrong.status = ConfirmationStatus::Pending,
                    2 => wrong.decided_by = Some("other".into()),
                    3 => wrong.decided_at = Some(Instant::from_unix_secs(21)),
                    4 => wrong.reason_code = Some("CHANGED".into()),
                    5 => wrong.comment = Some("更改".into()),
                    _ => wrong.owner_role = "role-finance".into(),
                }
                assert!(!wrong.matches_completed_decision(
                    &outcome,
                    &command,
                    "actor-1",
                    ConfirmationScope::Sales
                ));
            }
            command.expected_batch_version = u64::MAX;
            assert!(!fact.matches_completed_decision(
                &outcome,
                &command,
                "actor-1",
                ConfirmationScope::Sales
            ));
        }
    }
}
