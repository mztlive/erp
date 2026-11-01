//! W29 强类型命令回执及原结果，不读取审计消息。
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::IntegrationCommandIdentity;
use crate::dto::{ControlledEvidenceRef, DirectReconciliationStatus, IntegrationActionOutcome};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ActionReceiptResult {
    pub outcome: IntegrationActionOutcome,
    pub business_result_reference: Option<String>,
    pub verified_evidence: Vec<ControlledEvidenceRef>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionReceiptResult {
    pub terminal_evidence_reference: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DirectReceiptResult {
    pub resulting_status: DirectReconciliationStatus,
    pub is_terminal: bool,
    pub outcome: IntegrationActionOutcome,
    pub business_result_reference: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "result", rename_all = "snake_case")]
pub enum IntegrationCommandResult {
    TaskAction(ActionReceiptResult),
    TaskCompletion(CompletionReceiptResult),
    DirectDecision(DirectReceiptResult),
}

/// 仅领域登记的三类强类型结果可以进入回执集合。
pub trait IntegrationReceiptPayload: Clone + Send + Sync {
    /// 将领域强类型结果放入明确的持久化枚举。
    /// # 参数
    /// 无；消费本次结果。
    /// # 返回
    /// 返回领域登记的结果种类。
    /// # 错误
    /// 不返回错误。
    fn into_result(self) -> IntegrationCommandResult;
    /// 从领域枚举恢复相应种类的完整结果。
    /// # 参数
    /// * `result` - 已保存的结果枚举。
    /// # 返回
    /// 返回原结果。
    /// # 错误
    /// 结果种类不一致时返回内部错误。
    fn from_result(result: IntegrationCommandResult) -> Result<Self>;
}
impl IntegrationReceiptPayload for ActionReceiptResult {
    fn into_result(self) -> IntegrationCommandResult {
        IntegrationCommandResult::TaskAction(self)
    }
    fn from_result(result: IntegrationCommandResult) -> Result<Self> {
        match result {
            IntegrationCommandResult::TaskAction(value) => Ok(value),
            _ => Err(Error::Internal("W29 回执结果类型不一致".to_string())),
        }
    }
}
impl IntegrationReceiptPayload for CompletionReceiptResult {
    fn into_result(self) -> IntegrationCommandResult {
        IntegrationCommandResult::TaskCompletion(self)
    }
    fn from_result(result: IntegrationCommandResult) -> Result<Self> {
        match result {
            IntegrationCommandResult::TaskCompletion(value) => Ok(value),
            _ => Err(Error::Internal("W29 回执结果类型不一致".to_string())),
        }
    }
}
impl IntegrationReceiptPayload for DirectReceiptResult {
    fn into_result(self) -> IntegrationCommandResult {
        IntegrationCommandResult::DirectDecision(self)
    }
    fn from_result(result: IntegrationCommandResult) -> Result<Self> {
        match result {
            IntegrationCommandResult::DirectDecision(value) => Ok(value),
            _ => Err(Error::Internal("W29 回执结果类型不一致".to_string())),
        }
    }
}

/// 集成领域拥有的不可变命令身份、指纹和结果 schema。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct IntegrationCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u16,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub idempotency_key_hash: String,
    pub fingerprint_version: u16,
    pub fingerprint: String,
    pub result: IntegrationCommandResult,
    pub audit_event_id: String,
}
impl IntegrationCommandReceipt {
    /// 从已成功执行的命令及强类型结果构造回执。
    /// # 参数
    /// * `identity` - 原稳定命令身份。
    /// * `actor_id` - 当前认证操作人。
    /// * `result` - 原执行结果。
    /// * `audit_event_id` - 同事务业务事件关联。
    /// # 返回
    /// 返回已校验的领域回执。
    /// # 错误
    /// 身份或schema不合法时返回错误。
    pub fn new(
        identity: &IntegrationCommandIdentity,
        actor_id: &str,
        result: IntegrationCommandResult,
        audit_event_id: String,
    ) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(identity.receipt_id().to_string()),
            schema_version: 1,
            actor_id: actor_id.to_string(),
            action: identity.action().to_string(),
            resource_type: identity.resource_type().to_string(),
            resource_id: identity.resource_id().to_string(),
            idempotency_key_hash: identity.idempotency_key_hash().to_string(),
            fingerprint_version: 1,
            fingerprint: identity.fingerprint().to_string(),
            result,
            audit_event_id,
        };
        value.validate()?;
        if !identity.matches_receipt(
            &value.actor_id,
            &value.action,
            &value.resource_type,
            Some(&value.resource_id),
        ) {
            return Err(Error::ConflictError("幂等键已用于不同命令".to_string()));
        }
        Ok(value)
    }
    /// 回执版本、动作与结果种类、指纹和必填字段必须同时成立，否则失败关闭。
    fn validate(&self) -> Result<()> {
        let expected_action = match &self.result {
            IntegrationCommandResult::TaskAction(_) => "integration.task_action",
            IntegrationCommandResult::TaskCompletion(result) => {
                if result.terminal_evidence_reference.trim().is_empty() {
                    return Err(corrupted());
                }
                "integration.task_completion"
            },
            IntegrationCommandResult::DirectDecision(result) => {
                let terminal = matches!(
                    result.resulting_status,
                    DirectReconciliationStatus::ConfirmedNoError
                        | DirectReconciliationStatus::ConfirmedValidDifference
                );
                if result.is_terminal != terminal {
                    return Err(corrupted());
                }
                "integration.direct_reconciliation"
            },
        };
        if self.schema_version != 1
            || self.fingerprint_version != 1
            || self.base.is_deleted()
            || self.action != expected_action
            || self.fingerprint.len() != 64
            || self.idempotency_key_hash.len() != 64
            || [&self.base.id, &self.actor_id, &self.resource_type, &self.resource_id, &self.audit_event_id]
                .into_iter()
                .any(|s| s.trim().is_empty())
            || !self
                .fingerprint
                .bytes()
                .chain(self.idempotency_key_hash.bytes())
                .all(|b| b.is_ascii_hexdigit())
        {
            return Err(corrupted());
        }
        Ok(())
    }
    /// 校验原身份后恢复完整强类型结果，不读当前任务状态。
    /// # 参数
    /// * `identity` - 当前请求身份及载荷。
    /// * `actor_id` - 当前认证操作人。
    /// # 返回
    /// 返回原命令结果。
    /// # 错误
    /// 身份/载荷冲突优先于结果错误；损坏schema明确失败。
    pub fn recover<T: IntegrationReceiptPayload>(
        &self,
        identity: &IntegrationCommandIdentity,
        actor_id: &str,
    ) -> Result<T> {
        if self.base.id != identity.receipt_id()
            || actor_id != self.actor_id
            || !identity.matches_receipt(
                &self.actor_id,
                &self.action,
                &self.resource_type,
                Some(&self.resource_id),
            )
        {
            return Err(Error::ConflictError("幂等键已用于不同命令".to_string()));
        }
        self.validate()?;
        if self.fingerprint != identity.fingerprint()
            || self.idempotency_key_hash != identity.idempotency_key_hash()
        {
            return Err(Error::ConflictError("幂等键已用于不同命令".to_string()));
        }
        T::from_result(self.result.clone())
    }
}
fn corrupted() -> Error {
    Error::Internal("W29 幂等收据结果无效".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn identity(payload: &[u8]) -> IntegrationCommandIdentity {
        IntegrationCommandIdentity::new(
            "actor",
            "integration.task_action",
            "work_item",
            "wi-1",
            "same-key",
            payload,
        )
    }
    fn receipt() -> IntegrationCommandReceipt {
        IntegrationCommandReceipt::new(
            &identity(b"payload"),
            "actor",
            ActionReceiptResult {
                outcome: IntegrationActionOutcome::ResultUnknown,
                business_result_reference: Some("business-result".to_string()),
                verified_evidence: Vec::new(),
            }
            .into_result(),
            "audit-1".to_string(),
        )
        .unwrap()
    }
    #[test]
    fn typed_result_restores_unknown_business_outcome_without_json_message() {
        let value = receipt();
        let result: ActionReceiptResult = value.recover(&identity(b"payload"), "actor").unwrap();
        assert_eq!(result.outcome, IntegrationActionOutcome::ResultUnknown);
        assert_eq!(result.business_result_reference.as_deref(), Some("business-result"));
        let wire = serde_json::to_string(&value).unwrap();
        assert!(!wire.contains("\"same-key\""));
        let restored: IntegrationCommandReceipt = serde_json::from_str(&wire).unwrap();
        assert!(restored.recover::<ActionReceiptResult>(&identity(b"payload"), "actor").is_ok());
        assert!(matches!(
            value.recover::<ActionReceiptResult>(&identity(b"other"), "actor"),
            Err(Error::ConflictError(_))
        ));
    }
    #[test]
    fn identity_precedes_schema_and_mismatched_result_kind_is_rejected() {
        let mut value = receipt();
        value.schema_version = 2;
        value.actor_id = "other".to_string();
        assert!(matches!(
            value.recover::<ActionReceiptResult>(&identity(b"payload"), "actor"),
            Err(Error::ConflictError(_))
        ));
        value.actor_id = "actor".to_string();
        assert!(matches!(
            value.recover::<ActionReceiptResult>(&identity(b"payload"), "actor"),
            Err(Error::Internal(_))
        ));
        let value = receipt();
        assert!(matches!(
            value.recover::<CompletionReceiptResult>(&identity(b"payload"), "actor"),
            Err(Error::Internal(_))
        ));
    }
    #[test]
    fn completion_and_direct_results_keep_terminal_semantics() {
        let identity = IntegrationCommandIdentity::new(
            "actor",
            "integration.task_completion",
            "work_item",
            "wi-1",
            "key",
            b"payload",
        );
        let value = IntegrationCommandReceipt::new(
            &identity,
            "actor",
            CompletionReceiptResult { terminal_evidence_reference: "terminal:fact".to_string() }
                .into_result(),
            "audit".to_string(),
        )
        .unwrap();
        assert_eq!(
            value.recover::<CompletionReceiptResult>(&identity, "actor").unwrap().terminal_evidence_reference,
            "terminal:fact"
        );
        let identity = IntegrationCommandIdentity::new(
            "actor",
            "integration.direct_reconciliation",
            "reconciliation_difference",
            "difference",
            "key",
            b"payload",
        );
        let result = DirectReceiptResult {
            resulting_status: DirectReconciliationStatus::ConfirmedNoError,
            is_terminal: true,
            outcome: IntegrationActionOutcome::ConfirmedNoError,
            business_result_reference: None,
        };
        let value = IntegrationCommandReceipt::new(
            &identity,
            "actor",
            result.clone().into_result(),
            "audit".to_string(),
        )
        .unwrap();
        assert!(value.recover::<DirectReceiptResult>(&identity, "actor").unwrap().is_terminal);
        let mut invalid = result;
        invalid.is_terminal = false;
        assert!(
            IntegrationCommandReceipt::new(&identity, "actor", invalid.into_result(), "audit".to_string())
                .is_err()
        );
    }
}
