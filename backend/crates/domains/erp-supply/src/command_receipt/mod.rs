//! 供应链拥有的独立成功命令回执，与审计展示分别存储。

pub mod repository;
mod result;

use application_core::CommandReceipt;
use entity_core::BaseModel;
use entity_macros::Entity;
pub use result::{
    CompletionReceipt, DifferenceDecisionReceipt, InvestigationReceipt, RefreshReceipt,
    ReviewDecisionReceipt, ReviewSubmissionReceipt, SupplyCommandResult, SupplyExceptionReceipt,
};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 供应链成功命令的不可变身份、原请求指纹与强类型结果。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SupplyCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub schema_version: u16,
    pub actor_id: String,
    pub action: String,
    pub resource_type: String,
    pub resource_id: String,
    pub scope_id: String,
    pub idempotency_key_hash: String,
    pub fingerprint: String,
    pub fingerprint_algorithm: String,
    pub result: SupplyCommandResult,
    pub audit_event_id: String,
}

impl SupplyCommandReceipt {
    /// 由共享规范化命令构造供应链领域的原结果回执。
    ///
    /// # 参数
    /// * `command` - 已规范化的稳定命令身份及原载荷。
    /// * `resource_id` - 本次命令的已提交结果对象。
    /// * `result` - 拥有领域声明的强类型结果。
    /// * `audit_event_id` - 同事务保存的单次业务事件。
    /// # 返回
    /// 返回已校验的独立领域回执。
    /// # 错误
    /// 身份、结果或指纹不合法时返回错误。
    pub fn from_command(
        command: &CommandReceipt,
        resource_id: String,
        result: SupplyCommandResult,
        audit_event_id: String,
    ) -> Result<Self> {
        let value = Self {
            base: BaseModel::new(command.id().to_string()),
            schema_version: 1,
            actor_id: command.actor_id().to_string(),
            action: command.action().to_string(),
            resource_type: command.resource_type().to_string(),
            scope_id: command.scope_id().unwrap_or(&resource_id).to_string(),
            resource_id,
            idempotency_key_hash: command.idempotency_key_hash().digest_hex().to_string(),
            fingerprint: command.fingerprint().as_str().to_string(),
            fingerprint_algorithm: "sha256-canonical-v1".to_string(),
            result,
            audit_event_id,
        };
        value.validate()?;
        Ok(value)
    }

    /// 验证共享命令的完整身份及原载荷，恢复结果定位。
    ///
    /// # 参数
    /// * `command` - 已认证调用方的原请求身份。
    /// # 返回
    /// 返回原结果对象编号。
    /// # 错误
    /// 异载荷返回原通用冲突，身份损坏失败关闭。
    pub fn committed_resource_id(&self, command: &CommandReceipt) -> Result<String> {
        self.validate()?;
        if self.base.id != command.id()
            || self.actor_id != command.actor_id()
            || self.action != command.action()
            || self.resource_type != command.resource_type()
            || self.idempotency_key_hash != command.idempotency_key_hash().digest_hex()
            || self.scope_id != command.scope_id().unwrap_or(&self.resource_id)
        {
            return Err(corrupted());
        }
        self.verify(command.fingerprint().as_str(), None, "同一操作号已用于不同提交，请重新发起操作")?;
        Ok(self.resource_id.clone())
    }

    /// 校验持久化回执，未知版本或不匹配的结果必须明确失败。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 合法时返回空结果。
    /// # 错误
    /// 身份缺失、schema非法、指纹损坏或结果与动作不匹配时返回内部错误。
    pub fn validate(&self) -> Result<()> {
        if self.schema_version != 1
            || self.base.version == 0
            || self.base.is_deleted()
            || [
                &self.base.id,
                &self.actor_id,
                &self.action,
                &self.resource_type,
                &self.resource_id,
                &self.scope_id,
                &self.audit_event_id,
            ]
            .into_iter()
            .any(|value| value.trim().is_empty())
            || !digest_is_valid(&self.idempotency_key_hash)
            || !digest_is_valid(self.fingerprint.strip_prefix("sha256-v1:").unwrap_or(&self.fingerprint))
            || !matches!(
                self.fingerprint_algorithm.as_str(),
                "sha256-serialized-v1" | "sha256-length-prefixed-v1" | "sha256-canonical-v1"
            )
        {
            return Err(corrupted());
        }
        self.validate_result()
    }

    /// 比对原载荷及结果定位，保持展示日志完全不参与恢复。
    ///
    /// # 参数
    /// * `fingerprint` - 原入口规范化指纹。
    /// * `resource_id` - 当前入口的目标对象。
    /// * `conflict_message` - 原入口的异载荷冲突文案。
    /// # 返回
    /// 相同载荷、相同定位时返回空结果。
    /// # 错误
    /// 损坏返回内部错误，异载荷或对象不一致返回冲突。
    pub fn verify(&self, fingerprint: &str, resource_id: Option<&str>, conflict_message: &str) -> Result<()> {
        self.validate()?;
        if resource_id.is_some_and(|id| self.resource_id != id) {
            return Err(Error::ConflictError("幂等收据与当前业务资源不一致".to_string()));
        }
        if self.fingerprint != fingerprint {
            return Err(Error::ConflictError(conflict_message.to_string()));
        }
        Ok(())
    }

    /// 校验本次请求身份，不允许损坏元数据恢复成功结果。
    ///
    /// # 参数
    /// * `command_id` - 当前稳定命令编号。
    /// * `actor_id` - 当前已认证操作人。
    /// * `action` - 当前命令动作。
    /// * `scope_id` - 原入口定位的对象或任务。
    /// * `key_hash` - 按原算法计算的规范化幂等键摘要。
    /// # 返回
    /// 完整身份一致时返回空结果。
    /// # 错误
    /// 任一身份不一致时返回内部错误，禁止退回未执行分支。
    pub fn verify_identity(
        &self,
        command_id: &str,
        actor_id: &str,
        action: &str,
        scope_id: &str,
        key_hash: &str,
    ) -> Result<()> {
        self.validate()?;
        if self.base.id != command_id
            || self.actor_id != actor_id
            || self.action != action
            || self.scope_id != scope_id
            || self.idempotency_key_hash != key_hash
        {
            return Err(corrupted());
        }
        Ok(())
    }

    /// 校验动作目录、结果引用及关键版本。
    fn validate_result(&self) -> Result<()> {
        if self.result.matches_action(&self.action, &self.resource_type)
            && self.result.valid_payload(&self.resource_id, &self.action)
            && (matches!(&self.result, SupplyCommandResult::Completion(_))
                || self.action == "supplier_fulfillment.task_investigate"
                || self.scope_id == self.resource_id)
        {
            Ok(())
        } else {
            Err(corrupted())
        }
    }
}

/// 指纹只接受固定长度的十六进制摘要。
///
/// # 参数
/// * `value` - 待检查的指纹文本。
///
/// # 返回
/// 长度为 64 且全部为 ASCII 十六进制字符时返回 `true`，否则返回 `false`。
///
/// # 错误
/// 不返回错误。
pub(super) fn digest_is_valid(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

/// 损坏回执禁止退回展示审计恢复。
fn corrupted() -> Error {
    Error::Internal("供应链命令收据格式无效".to_string())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::Amount;
    use serde_json::json;

    use super::*;
    use crate::dto::supplier_settlement::SettlementReviewDecisionStatus;

    /// 通过实际共享命令规范化入口生成供应停止请求身份。
    fn exception_command(actor: &str, comment: &str) -> CommandReceipt {
        CommandReceipt::from_payload(
            "supply-exception-",
            actor,
            "supplier_offering.supply_exception.complete",
            "work_item",
            " private-operation-key ",
            &json!({"task":"task-1","comment":comment}),
        )
        .unwrap()
    }

    /// 固定实体时间戳不参与回执身份与指纹规则。
    fn receipt(action: &str, resource_type: &str, result: SupplyCommandResult) -> SupplyCommandReceipt {
        let mut base = BaseModel::fake();
        base.id = "command-1".to_string();
        SupplyCommandReceipt {
            base,
            schema_version: 1,
            actor_id: "actor-1".to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: "resource-1".to_string(),
            scope_id: "resource-1".to_string(),
            idempotency_key_hash: "a".repeat(64),
            fingerprint: "b".repeat(64),
            fingerprint_algorithm: "sha256-length-prefixed-v1".to_string(),
            result,
            audit_event_id: "event-1".to_string(),
        }
    }

    #[test]
    fn immutable_exception_result_replays_and_changed_request_conflicts_without_display_log() {
        let original = exception_command("actor-1", "原决定");
        let value = SupplyCommandReceipt::from_command(
            &original,
            "task-1".to_string(),
            SupplyCommandResult::SupplyException(SupplyExceptionReceipt {
                work_item_id: "task-1".to_string(),
                offering_id: "offering-1".to_string(),
                subject_version: "source-1".to_string(),
                task_version: 4,
                evidence_reference: "evidence-1".to_string(),
                comment: "原决定".to_string(),
            }),
            "event-1".to_string(),
        )
        .unwrap();
        assert_eq!(value.committed_resource_id(&original).unwrap(), "task-1");
        assert!(matches!(
            value.committed_resource_id(&exception_command("actor-1", "新决定")),
            Err(Error::ConflictError(_))
        ));
        assert!(matches!(
            value.committed_resource_id(&exception_command("actor-2", "原决定")),
            Err(Error::Internal(_))
        ));
        let encoded = serde_json::to_string(&value).unwrap();
        assert!(!encoded.contains("private-operation-key"));
        let restored: SupplyCommandReceipt = serde_json::from_str(&encoded).unwrap();
        assert_eq!(restored, value);
        assert_eq!(restored.committed_resource_id(&original).unwrap(), "task-1");
    }

    #[test]
    fn corrupt_receipts_fail_closed_and_never_become_an_absent_command() {
        let valid = receipt(
            "supplier_api_capability.update",
            "supplier_api_connection",
            SupplyCommandResult::CapabilitiesUpdated,
        );
        let mutations: [fn(&mut SupplyCommandReceipt); 9] = [
            |value| value.schema_version = 2,
            |value| value.base.version = 0,
            |value| value.base.deleted_at = 1,
            |value| value.actor_id.clear(),
            |value| value.audit_event_id.clear(),
            |value| value.fingerprint = "invalid".to_string(),
            |value| value.idempotency_key_hash = "not-a-digest".to_string(),
            |value| value.fingerprint_algorithm = "unknown".to_string(),
            |value| value.action = "supplier_settlement.refresh".to_string(),
        ];
        for mutate in mutations {
            let mut damaged = valid.clone();
            mutate(&mut damaged);
            assert!(matches!(
                damaged.verify(&valid.fingerprint, Some("resource-1"), "原冲突"),
                Err(Error::Internal(_))
            ));
        }
        assert!(
            matches!(valid.verify(&"c".repeat(64), Some("resource-1"), "原冲突"), Err(Error::ConflictError(message)) if message == "原冲突")
        );
        assert!(matches!(
            valid.verify(&valid.fingerprint, Some("different-resource"), "原冲突"),
            Err(Error::ConflictError(_))
        ));
    }

    #[test]
    fn replay_identity_checks_actor_original_scope_action_command_and_key_digest() {
        let valid = receipt(
            "supplier_fulfillment.task_complete",
            "SUPPLIER_FULFILLMENT_ORDER",
            SupplyCommandResult::Completion(CompletionReceipt {
                terminal_action_id: "terminal-1".to_string(),
                order_version: 3,
                task_version: 4,
                resolution: crate::dto::supplier_fulfillment::SupplierOrderResolution::OrderCompleted,
            }),
        );
        valid
            .verify_identity(
                "command-1",
                "actor-1",
                "supplier_fulfillment.task_complete",
                "resource-1",
                &"a".repeat(64),
            )
            .unwrap();
        let mutations: [fn(&mut SupplyCommandReceipt); 5] = [
            |value| value.base.id = "another-command".to_string(),
            |value| value.actor_id = "another-actor".to_string(),
            |value| value.action = "supplier_fulfillment.investigate".to_string(),
            |value| value.scope_id = "another-work-item".to_string(),
            |value| value.idempotency_key_hash = "c".repeat(64),
        ];
        for mutate in mutations {
            let mut damaged = valid.clone();
            mutate(&mut damaged);
            assert!(matches!(
                damaged.verify_identity(
                    "command-1",
                    "actor-1",
                    "supplier_fulfillment.task_complete",
                    "resource-1",
                    &"a".repeat(64)
                ),
                Err(Error::Internal(_))
            ));
        }
    }

    #[test]
    fn unchanged_refresh_is_a_valid_original_result_but_missing_snapshot_or_version_is_rejected() {
        let result = RefreshReceipt::new("request-1".to_string(), "a".repeat(64)).with_counts(7, 0, 0);
        let mut value = receipt(
            "supplier_settlement.refresh",
            "supplier_settlement_statement",
            SupplyCommandResult::Refresh(result),
        );
        value.validate().unwrap();
        let SupplyCommandResult::Refresh(result) = &mut value.result else { unreachable!() };
        result.statement_version = 0;
        assert!(value.validate().is_err());
        let SupplyCommandResult::Refresh(result) = &mut value.result else { unreachable!() };
        result.statement_version = 7;
        result.source_snapshot_hash.clear();
        assert!(value.validate().is_err());
    }

    #[test]
    fn result_catalog_rejects_wrong_action_or_resource_and_lost_reference_versions() {
        use crate::dto::supplier_fulfillment::SupplierOrderResolution;

        let cases = [
            (
                "supplier_fulfillment.investigate",
                "SUPPLIER_FULFILLMENT_ORDER",
                SupplyCommandResult::Investigation(InvestigationReceipt {
                    evidence_id: "evidence-1".to_string(),
                    order_version: 3,
                    task_version: None,
                }),
            ),
            (
                "supplier_fulfillment.task_investigate",
                "SUPPLIER_FULFILLMENT_ORDER",
                SupplyCommandResult::Investigation(InvestigationReceipt {
                    evidence_id: "evidence-1".to_string(),
                    order_version: 3,
                    task_version: Some(4),
                }),
            ),
            (
                "supplier_fulfillment.task_complete",
                "SUPPLIER_FULFILLMENT_ORDER",
                SupplyCommandResult::Completion(CompletionReceipt {
                    terminal_action_id: "terminal-1".to_string(),
                    order_version: 3,
                    task_version: 4,
                    resolution: SupplierOrderResolution::OrderCompleted,
                }),
            ),
            (
                "supplier_settlement.difference_decision",
                "supplier_settlement_difference",
                SupplyCommandResult::DifferenceDecision(DifferenceDecisionReceipt {
                    operation_id: "operation-1".to_string(),
                    statement_id: "statement-1".to_string(),
                    statement_version: 4,
                    difference_version: 5,
                }),
            ),
            (
                "supplier_settlement.submit_review",
                "supplier_settlement_statement",
                SupplyCommandResult::ReviewSubmission(ReviewSubmissionReceipt {
                    operation_id: "operation-1".to_string(),
                    statement_version: 4,
                    work_item_id: "task-1".to_string(),
                    task_version: 5,
                }),
            ),
            (
                "supplier_fulfillment.handover",
                "supplier_fulfillment_order",
                SupplyCommandResult::FulfillmentHandover,
            ),
            (
                "supplier_settlement.handover",
                "supplier_settlement_statement",
                SupplyCommandResult::SettlementHandover,
            ),
            (
                "supplier_settlement.reassign_difference_handler",
                "supplier_settlement_statement",
                SupplyCommandResult::DifferenceHandlerReassigned,
            ),
            (
                "supplier_api_capability.update",
                "supplier_api_connection",
                SupplyCommandResult::CapabilitiesUpdated,
            ),
        ];
        for (action, resource_type, result) in cases {
            let valid = receipt(action, resource_type, result);
            valid.validate().unwrap();
            let decoded: SupplyCommandReceipt =
                serde_json::from_str(&serde_json::to_string(&valid).unwrap()).unwrap();
            assert_eq!(decoded, valid);
            let mut wrong = valid.clone();
            wrong.action = "unregistered.action".to_string();
            assert!(wrong.validate().is_err());
            let mut wrong = valid;
            wrong.resource_type = "different-resource-type".to_string();
            assert!(wrong.validate().is_err());
        }
        let mut missing = receipt(
            "supplier_fulfillment.task_investigate",
            "SUPPLIER_FULFILLMENT_ORDER",
            SupplyCommandResult::Investigation(InvestigationReceipt {
                evidence_id: String::new(),
                order_version: 3,
                task_version: Some(4),
            }),
        );
        assert!(missing.validate().is_err());
        missing.result = SupplyCommandResult::Investigation(InvestigationReceipt {
            evidence_id: "evidence-1".to_string(),
            order_version: 3,
            task_version: Some(0),
        });
        assert!(missing.validate().is_err());
    }

    #[test]
    fn review_status_payable_and_exact_cost_delta_must_match_original_decision() {
        let decision = ReviewDecisionReceipt {
            operation_id: "operation-1".to_string(),
            result_status: SettlementReviewDecisionStatus::Confirmed,
            statement_version: 5,
            task_version: 8,
            payable_account_id: Some("payable-1".to_string()),
            cost_delta: Some(Amount::from_str("-12.34").unwrap()),
        };
        let mut value = receipt(
            "supplier_settlement.review_confirm",
            "supplier_settlement_statement",
            SupplyCommandResult::ReviewDecision(decision),
        );
        value.validate().unwrap();
        let restored: SupplyCommandReceipt =
            serde_json::from_str(&serde_json::to_string(&value).unwrap()).unwrap();
        assert_eq!(restored, value);
        value.action = "supplier_settlement.review_reject".to_string();
        assert!(value.validate().is_err());
        let SupplyCommandResult::ReviewDecision(decision) = &mut value.result else { unreachable!() };
        decision.result_status = SettlementReviewDecisionStatus::Rejected;
        assert!(value.validate().is_err());
        let SupplyCommandResult::ReviewDecision(decision) = &mut value.result else { unreachable!() };
        decision.payable_account_id = None;
        decision.cost_delta = None;
        value.validate().unwrap();
    }
}
