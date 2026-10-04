use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{DifferenceDecisionReceipt, SupplyCommandResult};
use erp_supply::entity::supplier_settlement::{
    SettlementDifferenceConclusion, SupplierSettlementDifference, SupplierSettlementStatement,
};
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::difference::{
    difference_conclusion_kind, difference_decision_fingerprint, settlement_difference_view,
};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::dto::{SettlementDifferenceDecisionRequest, SettlementDifferenceDecisionResult};
use super::{SupplierSettlementProcess, command_audit_id, dto, ensure_same_id};
use crate::audit::{persist_log, recover_command};
use crate::supply_execution::receipt::persist_supply_receipt;
use crate::{Error, Result};

impl SupplierSettlementProcess {
    /// 登记财务经办的强类型差异结论。
    ///
    /// 命令同时 CAS 结算单与差异版本，规范化受控原因/证据，推进主题摘要并写
    /// 幂等审计。客户端不能提交处理人、处理时间或任意持久化状态。
    ///
    /// # 错误
    /// 路径身份、归属、版本、经办责任或证据规则不一致时 fail-closed。
    pub async fn decide_difference(
        &self,
        id: &str,
        req: SettlementDifferenceDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceDecisionResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        ensure_same_id(id, &req.difference_id, "结算差异")?;
        let conclusion = parse_difference_conclusion(&req)?;
        let fingerprint = difference_decision_fingerprint(&req, &conclusion);
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.difference_decision", id, &req.idempotency_key);
        if let Some(result) =
            self.replay_difference_decision(&audit_id, &fingerprint, id, (actor.id(), &replay_key)).await?
        {
            return Ok(result);
        }
        self.domain()
            .access()
            .require_statement(actor, "update", &req.statement_id, &mut NoTransaction)
            .await?;
        let (mut statement, mut difference) = self
            .domain()
            .prepare_difference_decision(id, &req, &conclusion, actor.id(), &mut NoTransaction)
            .await?;
        let db = self.db.clone();
        let client = db.client().clone();
        let data_scope = self.data_scope.clone();
        let audit_actor = actor.clone();
        let statement_id = req.statement_id.clone();
        let operation_id = req.operation_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let audit_id_for_tx = audit_id.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    erp_supply::SettlementAccess::new(db.clone(), data_scope)
                        .require_statement(&audit_actor, "update", &statement_id, executor)
                        .await?;
                    SupplierSettlementService::new(db.clone())
                        .persist_difference_decision(&mut statement, &mut difference, executor)
                        .await?;
                    persist_difference_result(
                        DifferenceWrite {
                            db: &db,
                            actor: &audit_actor,
                            statement: &statement,
                            difference: &difference,
                            command_id: &audit_id_for_tx,
                            fingerprint: &fingerprint_for_tx,
                            request: &req,
                        },
                        executor,
                    )
                    .await?;
                    Ok::<(SupplierSettlementStatement, SupplierSettlementDifference), crate::Error>((
                        statement, difference,
                    ))
                })
            })
            .await;
        let (statement, difference) = match transaction_result {
            Ok(result) => result,
            Err(error) => {
                return recover_command(
                    error,
                    self.replay_difference_decision(&audit_id, &fingerprint, id, (actor.id(), &replay_key))
                        .await,
                );
            },
        };
        Ok(SettlementDifferenceDecisionResult {
            result_status: dto::SettlementDifferenceDecisionStatus::Resolved,
            message: "结算差异正式结论已登记".to_string(),
            operation_id,
            statement_id: statement.base.id,
            statement_lock_version: statement.base.version,
            difference: settlement_difference_view(difference),
        })
    }

    /// 重放差异决定并恢复同一业务结果。
    async fn replay_difference_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        difference_id: &str,
        identity: (&str, &str),
    ) -> Result<Option<SettlementDifferenceDecisionResult>> {
        let Some(stored) =
            self.db.supply_command_receipts().find_command(audit_id, &mut NoTransaction).await?
        else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            identity.0,
            "supplier_settlement.difference_decision",
            difference_id,
            &stable_digest(identity.1.trim()),
        )?;
        stored.verify(expected_fingerprint, Some(difference_id), "幂等键已用于不同的结算差异决定命令")?;
        let SupplyCommandResult::DifferenceDecision(receipt) = stored.result else {
            return Err(Error::Internal("结算差异决定回执类型非法".to_string()));
        };
        let difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(difference_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的差异不存在".to_string()))?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的结算明细不存在".to_string()))?;
        ensure_difference_replay(
            &receipt,
            item.statement_id.as_ref(),
            difference.base.version,
            difference.is_pending(),
        )?;
        let statement = self.domain().load_statement(&receipt.statement_id, &mut NoTransaction).await?;
        ensure_difference_statement_version(&receipt, statement.base.version)?;
        Ok(Some(SettlementDifferenceDecisionResult {
            result_status: dto::SettlementDifferenceDecisionStatus::Resolved,
            message: "结算差异正式结论已登记".to_string(),
            operation_id: receipt.operation_id,
            statement_id: receipt.statement_id,
            statement_lock_version: receipt.statement_version,
            difference: settlement_difference_view(difference),
        }))
    }
}

/// 差异收据只恢复同一已解决差异和同一结算归属。
fn ensure_difference_replay(
    receipt: &DifferenceDecisionReceipt,
    statement_id: &str,
    difference_version: u64,
    pending: bool,
) -> Result<()> {
    if receipt.statement_id != statement_id || difference_version != receipt.difference_version || pending {
        return Err(Error::ConflictError("差异决定幂等收据与当前正式事实不一致".to_string()));
    }
    Ok(())
}
/// 结算单可继续推进；响应仍由原收据的版本构造。
fn ensure_difference_statement_version(
    receipt: &DifferenceDecisionReceipt,
    current_version: u64,
) -> Result<()> {
    if current_version < receipt.statement_version {
        return Err(Error::ConflictError("差异决定幂等收据的结算单版本非法".to_string()));
    }
    Ok(())
}

/// 差异与结算主题已写入后的不可变结果输入。
struct DifferenceWrite<'a> {
    db: &'a Database,
    actor: &'a AuditActor,
    statement: &'a SupplierSettlementStatement,
    difference: &'a SupplierSettlementDifference,
    command_id: &'a str,
    fingerprint: &'a str,
    request: &'a SettlementDifferenceDecisionRequest,
}

/// 按原先的结果、回执、事件顺序保存差异决定。
async fn persist_difference_result(input: DifferenceWrite<'_>, executor: &mut dyn Executor) -> Result<()> {
    let receipt = DifferenceDecisionReceipt {
        operation_id: input.request.operation_id.clone(),
        statement_id: input.statement.base.id.clone(),
        statement_version: input.statement.base.version,
        difference_version: input.difference.base.version,
    };
    let audit = input
        .actor
        .clone()
        .resource_log_with_id(
            input.command_id.to_string(),
            "supplier_settlement.difference_decision",
            "supplier_settlement_difference",
            input.difference.base.id.clone(),
            Some("结算差异正式结论已登记".to_string()),
        )?
        .with_command_id(Some(input.command_id.to_string()))?;
    persist_supply_receipt(
        input.db,
        &audit,
        input.fingerprint,
        &input.request.idempotency_key,
        &input.difference.base.id,
        SupplyCommandResult::DifferenceDecision(receipt),
        executor,
    )
    .await?;
    persist_log(input.db, &audit, executor).await?;
    Ok(())
}

/// 按命令中受控原因与证据构造领域结论，保持原验证错误。
fn parse_difference_conclusion(
    req: &SettlementDifferenceDecisionRequest,
) -> Result<SettlementDifferenceConclusion> {
    SettlementDifferenceConclusion::new(
        difference_conclusion_kind(req.resolution),
        req.reason_code.clone(),
        req.evidence_reference_ids.clone(),
    )
    .map_err(|error| Error::ValidationError(error.to_string()))
}

#[cfg(test)]
mod replay_tests {
    use super::*;
    #[test]
    fn difference_receipt_requires_exact_resolved_difference_and_allows_later_statement() {
        let receipt = DifferenceDecisionReceipt {
            operation_id: "op-1".into(),
            statement_id: "statement-1".into(),
            statement_version: 2,
            difference_version: 2,
        };
        for version in [1, 2, 3] {
            assert_eq!(
                ensure_difference_replay(&receipt, "statement-1", version, false).is_ok(),
                version == 2
            );
            assert_eq!(ensure_difference_statement_version(&receipt, version).is_ok(), version >= 2);
        }
        assert!(ensure_difference_replay(&receipt, "statement-2", 2, false).is_err());
        assert!(ensure_difference_replay(&receipt, "statement-1", 2, true).is_err());
        assert_eq!(receipt.statement_version, 2);
    }
}
