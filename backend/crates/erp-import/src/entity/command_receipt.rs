//! W18 导入命令的独立强类型回执；展示审计不承担结果恢复。

use application_core::StructuredCommandReceipt;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use crate::{
    ConfirmationDecision, ConfirmationScope, Error, ImportBusinessConfirmationNextStep,
    ImportBusinessConfirmationResultStatus, ImportExecutionAction, ImportExecutionNextStep,
    ImportExecutionResultStatus, ImportJobStatus, LegacyImportBatchStatus, Result,
};

/// 确认完成命令的原结果，后续批次推进不得改写。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConfirmationCompletionReceipt {
    pub result_status: ImportBusinessConfirmationResultStatus,
    pub task_version: u64,
    pub batch_version: u64,
    pub next_step: ImportBusinessConfirmationNextStep,
}

/// 执行命令的原结果，后台进度独立继续推进。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportExecutionReceipt {
    pub action: ImportExecutionAction,
    pub result_status: ImportExecutionResultStatus,
    pub batch_version: u64,
    pub batch_status: LegacyImportBatchStatus,
    pub trial_version: Option<u32>,
    pub job_version: u64,
    pub job_status: ImportJobStatus,
    pub affected_items: u64,
    pub next_step: ImportExecutionNextStep,
}

/// 确认完成的正式事实和任务终态关联。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ImportConfirmationOutcome {
    pub confirmation_id: String,
    pub confirmation_version: u64,
    pub batch_id: String,
    pub work_item_id: String,
    pub subject_version: String,
    pub confirmation_scope: String,
    pub decision: ConfirmationDecision,
    pub decided_at: Instant,
    pub receipt: ConfirmationCompletionReceipt,
}

/// 每个命令只允许恢复本动作的强类型结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ImportCommandResult {
    Confirmation(Box<ImportConfirmationOutcome>),
    Execution { batch_id: String, background_job_id: String, receipt: ImportExecutionReceipt },
}

/// 与导入事实、任务及成功事件同事务提交的不可变回执。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct ImportCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u16,
    pub identity: StructuredCommandReceipt,
    pub result: ImportCommandResult,
    pub audit_event_id: String,
}

impl ImportCommandReceipt {
    /// 形成拥有领域的不可变命令结果。
    /// # 参数
    /// * `identity` - 当前规范化命令的完整结构化身份。
    /// * `result` - 动作专属正式结果。
    /// * `audit_event_id` - 本次成功事件的随机关联号。
    /// # 返回
    /// 返回以稳定命令 ID 唯一定位的回执。
    /// # 错误
    /// 身份或强类型结果非法时拒绝构造。
    pub fn new(
        identity: StructuredCommandReceipt,
        result: ImportCommandResult,
        audit_event_id: String,
    ) -> Result<Self> {
        let fact = Self {
            base: BaseModel::new(identity.command_id.clone()),
            schema_version: 1,
            identity,
            result,
            audit_event_id,
        };
        fact.validate()?;
        Ok(fact)
    }

    /// 校验持久化回执，损坏的新事实不得退回展示审计。
    /// # 返回
    /// 完整身份及合法动作结果返回空值。
    /// # 错误
    /// schema、命令或结果不一致时返回冲突。
    pub fn validate(&self) -> Result<()> {
        self.identity.validate()?;
        if self.schema_version != 1
            || self.base.is_deleted()
            || self.base.id != self.identity.command_id
            || self.audit_event_id.trim().is_empty()
            || !self.valid_result()
        {
            return Err(Error::ConflictError("导入命令结构化回执损坏".into()));
        }
        Ok(())
    }

    /// 精确核对当前命令身份和载荷，不查询旧别名。
    /// # 参数
    /// * `expected` - 当前请求按原稳定算法构造的身份。
    /// # 返回
    /// 完整身份和载荷一致时返回空值。
    /// # 错误
    /// 同键异参、损坏身份或结果时停止恢复。
    pub fn ensure_identity(&self, expected: &StructuredCommandReceipt) -> Result<()> {
        self.validate()?;
        if &self.identity != expected {
            return Err(Error::ConflictError("幂等键已用于不同的导入命令".into()));
        }
        Ok(())
    }

    /// 动作、资源作用域及强类型结果同时成立。
    fn valid_result(&self) -> bool {
        match &self.result {
            ImportCommandResult::Confirmation(result) => {
                self.identity.action == "legacy_import_confirmation.complete"
                    && self.identity.resource_type == "legacy_import_confirmation"
                    && self.identity.scope_id.as_deref() == Some(result.work_item_id.as_str())
                    && valid_confirmation(result)
            },
            ImportCommandResult::Execution { batch_id, background_job_id, receipt } => {
                self.identity.action == "legacy_import_batch.execute"
                    && self.identity.resource_type == "legacy_import_batch"
                    && self.identity.scope_id.as_deref() == Some(batch_id.as_str())
                    && !batch_id.trim().is_empty()
                    && !background_job_id.trim().is_empty()
                    && valid_execution(receipt)
            },
        }
    }
}

/// 确认决定和下一步必须保持原业务矩阵。
fn valid_confirmation(result: &ImportConfirmationOutcome) -> bool {
    let receipt = &result.receipt;
    [&result.confirmation_id, &result.batch_id, &result.work_item_id, &result.subject_version]
        .iter()
        .all(|value| !value.trim().is_empty())
        && result.confirmation_version > 0
        && receipt.task_version > 0
        && receipt.batch_version > 0
        && ConfirmationScope::parse(&result.confirmation_scope).is_ok()
        && match result.decision {
            ConfirmationDecision::ConfirmScope => {
                receipt.result_status == ImportBusinessConfirmationResultStatus::Confirmed
                    && matches!(
                        receipt.next_step,
                        ImportBusinessConfirmationNextStep::AwaitOtherConfirmations
                            | ImportBusinessConfirmationNextStep::StartApply
                    )
            },
            ConfirmationDecision::ReturnForFix => {
                receipt.result_status == ImportBusinessConfirmationResultStatus::Rejected
                    && receipt.next_step == ImportBusinessConfirmationNextStep::FixAndRevalidate
            },
        }
}

/// 执行状态仅表示命令原提交结果，不能记录 Unknown 作为成功回执。
fn valid_execution(receipt: &ImportExecutionReceipt) -> bool {
    receipt.batch_version > 0
        && receipt.job_version > 0
        && receipt.affected_items > 0
        && receipt.trial_version != Some(0)
        && match receipt.action {
            ImportExecutionAction::StartApply => {
                receipt.result_status == ImportExecutionResultStatus::Started
                    && receipt.batch_status == LegacyImportBatchStatus::Importing
                    && receipt.job_status == ImportJobStatus::Running
                    && receipt.next_step == ImportExecutionNextStep::MonitorProgress
                    && receipt.trial_version.is_some()
            },
            ImportExecutionAction::CancelPending => {
                receipt.result_status == ImportExecutionResultStatus::Cancelled
                    && matches!(
                        receipt.batch_status,
                        LegacyImportBatchStatus::Failed | LegacyImportBatchStatus::PartialFailed
                    )
                    && receipt.job_status == ImportJobStatus::Cancelled
                    && receipt.next_step == ImportExecutionNextStep::ReviewResult
            },
            ImportExecutionAction::RetryFailed => {
                receipt.result_status == ImportExecutionResultStatus::RetryPrepared
                    && receipt.batch_status == LegacyImportBatchStatus::ReadyToApply
                    && receipt.job_status == ImportJobStatus::Pending
                    && receipt.next_step == ImportExecutionNextStep::StartApply
                    && receipt.trial_version.is_some()
            },
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LegacyImportCommandIdentity;

    fn execution(action: ImportExecutionAction) -> ImportExecutionReceipt {
        let (result_status, batch_status, job_status, next_step) = match action {
            ImportExecutionAction::StartApply => (
                ImportExecutionResultStatus::Started,
                LegacyImportBatchStatus::Importing,
                ImportJobStatus::Running,
                ImportExecutionNextStep::MonitorProgress,
            ),
            ImportExecutionAction::CancelPending => (
                ImportExecutionResultStatus::Cancelled,
                LegacyImportBatchStatus::Failed,
                ImportJobStatus::Cancelled,
                ImportExecutionNextStep::ReviewResult,
            ),
            ImportExecutionAction::RetryFailed => (
                ImportExecutionResultStatus::RetryPrepared,
                LegacyImportBatchStatus::ReadyToApply,
                ImportJobStatus::Pending,
                ImportExecutionNextStep::StartApply,
            ),
        };
        ImportExecutionReceipt {
            action,
            result_status,
            batch_version: 5,
            batch_status,
            trial_version: Some(2),
            job_version: 2,
            job_status,
            affected_items: 3,
            next_step,
        }
    }

    fn fact(action: ImportExecutionAction) -> ImportCommandReceipt {
        let identity = LegacyImportCommandIdentity::new(
            "import-",
            "actor",
            "legacy_import_batch.execute",
            "batch",
            "private-key",
            &[action.as_str(), "4"],
        )
        .structured_receipt("legacy_import_batch")
        .unwrap();
        ImportCommandReceipt::new(
            identity,
            ImportCommandResult::Execution {
                batch_id: "batch".into(),
                background_job_id: "job".into(),
                receipt: execution(action),
            },
            "event".into(),
        )
        .unwrap()
    }

    #[test]
    fn execution_results_enforce_all_action_matrices_and_roundtrip() {
        for action in [
            ImportExecutionAction::StartApply,
            ImportExecutionAction::CancelPending,
            ImportExecutionAction::RetryFailed,
        ] {
            let original = fact(action);
            let encoded = serde_json::to_string(&original).unwrap();
            assert!(!encoded.contains("private-key"));
            let decoded: ImportCommandReceipt = serde_json::from_str(&encoded).unwrap();
            assert_eq!(decoded, original);
            assert!(decoded.ensure_identity(&original.identity).is_ok());
            let mut damaged = original.clone();
            let ImportCommandResult::Execution { receipt, .. } = &mut damaged.result else { unreachable!() };
            receipt.result_status = ImportExecutionResultStatus::Unknown;
            assert!(damaged.validate().is_err());
            let mut damaged = original;
            damaged.schema_version = 2;
            assert!(damaged.validate().is_err());
        }
    }

    #[test]
    fn original_stable_id_is_preserved_and_changed_payload_is_rejected() {
        let original = fact(ImportExecutionAction::StartApply);
        let changed = fact(ImportExecutionAction::RetryFailed);
        assert_eq!(original.base.id, changed.base.id);
        assert!(original.ensure_identity(&changed.identity).is_err());
        let mut foreign = original.identity.clone();
        foreign.scope_id = Some("foreign".into());
        assert!(original.ensure_identity(&foreign).is_err());
        let mut damaged = original;
        damaged.identity.schema_version = 2;
        assert!(damaged.validate().is_err());
    }
}
