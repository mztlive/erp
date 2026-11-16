use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::{persist_log, recover_command};
use erp_fulfillment_execution::supply_execution::receipt::persist_supply_receipt;
use erp_identity::AccessTarget;
use erp_permissions::supplier_settlement;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{DifferenceDecisionReceipt, SupplyCommandResult};
use erp_supply::entity::supplier_settlement::{
    SettlementDifferenceConclusion, SupplierSettlementDifference, SupplierSettlementStatement,
};
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_settlement::difference::{
    difference_conclusion_kind, difference_decision_fingerprint, settlement_difference_view,
};
use erp_supply::service::supplier_settlement::{SettlementAction, SupplierSettlementService};
use erp_workflow::ports::WorkflowAuthorizationPort;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::dto::{SettlementDifferenceDecisionRequest, SettlementDifferenceDecisionResult};
use super::{SettlementWriteContext, SupplierSettlementProcess, command_audit_id, dto, ensure_same_id};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 登记财务经办的强类型差异结论。
    ///
    /// 命令同时 CAS 结算单与差异版本，规范化受控原因/证据，推进主题摘要并写
    /// 幂等审计。客户端不能提交处理人、处理时间或任意持久化状态。
    ///
    /// # 参数
    /// * `id` - 路径中的差异 ID。
    /// * `req` - 差异结论命令。
    /// * `actor` - 当前财务经办人。
    ///
    /// # 返回
    /// 返回已登记的差异结论；同一命令重放返回原结果。
    ///
    /// # 错误
    /// 路径身份、归属、版本、经办责任或证据规则不一致时失败关闭。请求校验或事务写入失败时返回对应错误。
    pub async fn decide_difference(
        &self,
        id: &str,
        req: SettlementDifferenceDecisionRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDifferenceDecisionResult> {
        let process = self.clone();
        let command = req.clone();
        let writer = actor.clone();
        let id = id.to_owned();
        let write_id = id.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.decide_difference_apply(&write_id, command, &writer, executor).await
                })
            })
            .await;
        let Err(error) = result else { return result };
        let process = self.clone();
        let actor = actor.clone();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { process.recover_difference_decision(&id, &req, &actor, executor).await },
                )
            })
            .await;
        recover_command(error, recovered)
    }

    /// 在独立读取事务内重新授权并恢复已提交的差异决定回执。
    async fn recover_difference_decision(
        &self,
        id: &str,
        req: &SettlementDifferenceDecisionRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDifferenceDecisionResult>> {
        let fingerprint = difference_decision_fingerprint(req, &parse_difference_conclusion(req)?);
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.difference_decision", id, &req.idempotency_key);
        self.replay_difference_decision(
            &audit_id,
            &fingerprint,
            id,
            &req.statement_id,
            (actor, &req.idempotency_key),
            executor,
        )
        .await
    }

    /// 在入口事务内完成授权、准备、重放与持久化。
    async fn decide_difference_apply(
        &self,
        id: &str,
        req: SettlementDifferenceDecisionRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<SettlementDifferenceDecisionResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        ensure_same_id(id, &req.difference_id, "结算差异")?;
        let conclusion = parse_difference_conclusion(&req)?;
        let fingerprint = difference_decision_fingerprint(&req, &conclusion);
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.difference_decision", id, &req.idempotency_key);
        if let Some(result) = self
            .replay_difference_decision(
                &audit_id,
                &fingerprint,
                id,
                &req.statement_id,
                (actor, &replay_key),
                executor,
            )
            .await?
        {
            return Ok(result);
        }
        let (statement, difference) =
            self.domain().prepare_difference_decision(id, &req, &conclusion, actor, executor).await?;
        let operation_id = req.operation_id.clone();
        let input =
            DifferenceCommit::new(self, req, actor, (statement, difference), (&audit_id, &fingerprint));
        let (statement, difference) = input.persist(executor).await?;
        Ok(SettlementDifferenceDecisionResult {
            result_status: dto::SettlementDifferenceDecisionStatus::Resolved,
            message: "结算差异正式结论已登记".to_string(),
            operation_id,
            statement_id: statement.base.id,
            statement_lock_version: statement.base.version,
            difference: settlement_difference_view(difference),
        })
    }

    /// 首次执行与重放均先证明真实处理人及功能资格，再恢复同一业务结果。
    async fn replay_difference_decision(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        difference_id: &str,
        statement_id: &str,
        identity: (&AuditActor, &str),
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDifferenceDecisionResult>> {
        let (actor, key) = identity;
        self.data_scope
            .require_action(actor, SettlementAction::ResolveDifference, statement_id, executor)
            .await?;
        self.authorization
            .require(actor, &supplier_settlement::DIFFERENCE_DECIDE, AccessTarget::Operation, executor)
            .await?;
        let Some(stored) = self.db.supply_command_receipts().find_command(audit_id, executor).await? else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            actor.id(),
            "supplier_settlement.difference_decision",
            difference_id,
            &stable_digest(key.trim()),
        )?;
        stored.verify(expected_fingerprint, Some(difference_id), "幂等键已用于不同的结算差异决定命令")?;
        let SupplyCommandResult::DifferenceDecision(receipt) = stored.result else {
            return Err(Error::Internal("结算差异决定回执类型非法".to_string()));
        };
        ensure_difference_statement(&receipt, statement_id)?;
        let difference = self.load_replayed_difference(difference_id, &receipt, executor).await?;
        let statement = self.domain().load_statement(&receipt.statement_id, executor).await?;
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

    /// 回执只能恢复同一结算归属且仍为原版本的正式差异。
    async fn load_replayed_difference(
        &self,
        difference_id: &str,
        receipt: &DifferenceDecisionReceipt,
        executor: &mut dyn Executor,
    ) -> Result<SupplierSettlementDifference> {
        let difference = self
            .db
            .supplier_settlement_differences()
            .find_by_id(difference_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的差异不存在".to_string()))?;
        let item = self
            .db
            .supplier_settlement_items()
            .find_by_id(&difference.statement_item_id, executor)
            .await?
            .ok_or_else(|| Error::Internal("差异决定收据引用的结算明细不存在".to_string()))?;
        ensure_difference_replay(
            receipt,
            item.statement_id.as_ref(),
            difference.base.version,
            difference.is_pending(),
        )?;
        Ok(difference)
    }
}

/// 原回执必须属于本次真实操作人已获授权的结算单。
fn ensure_difference_statement(receipt: &DifferenceDecisionReceipt, statement_id: &str) -> Result<()> {
    if receipt.statement_id != statement_id {
        return Err(Error::ConflictError("差异决定幂等收据与当前结算单不一致".into()));
    }
    Ok(())
}

struct DifferenceCommit {
    context: SettlementWriteContext,
    request: SettlementDifferenceDecisionRequest,
    statement: SupplierSettlementStatement,
    difference: SupplierSettlementDifference,
    command_id: String,
    fingerprint: String,
}

impl DifferenceCommit {
    fn new<A: WorkflowAuthorizationPort>(
        process: &SupplierSettlementProcess<A>,
        request: SettlementDifferenceDecisionRequest,
        actor: &AuditActor,
        prepared: (SupplierSettlementStatement, SupplierSettlementDifference),
        identity: (&str, &str),
    ) -> Self {
        Self {
            context: process.write_context(actor),
            request,
            statement: prepared.0,
            difference: prepared.1,
            command_id: identity.0.to_string(),
            fingerprint: identity.1.to_string(),
        }
    }

    /// 在入口执行器上重验范围后保存结算主题、差异、回执和审计。
    async fn persist(
        mut self,
        executor: &mut dyn Executor,
    ) -> Result<(SupplierSettlementStatement, SupplierSettlementDifference)> {
        let context = self.context;
        context
            .data_scope
            .require_action(
                &context.actor,
                SettlementAction::ResolveDifference,
                &self.request.statement_id,
                executor,
            )
            .await?;
        SupplierSettlementService::new(context.db.clone())
            .persist_difference_decision(&mut self.statement, &mut self.difference, executor)
            .await?;
        persist_difference_result(
            DifferenceWrite {
                db: &context.db,
                actor: &context.actor,
                statement: &self.statement,
                difference: &self.difference,
                command_id: &self.command_id,
                fingerprint: &self.fingerprint,
                request: &self.request,
            },
            executor,
        )
        .await?;
        Ok((self.statement, self.difference))
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
    .await
    .map_err(Error::from)?;
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
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn difference_first_attempt_and_recovery_require_command_access_before_receipt_reads() {
        let process = super::super::tests::unwired_process().await;
        let actor = AuditActor::new("handler".into(), "handler-login".into(), AccountKind::Supplier);
        let request = SettlementDifferenceDecisionRequest {
            statement_id: "statement-1".into(),
            difference_id: "difference-1".into(),
            expected_lock_version: 1,
            expected_difference_version: 1,
            resolution: dto::SettlementDifferenceResolution::SupplierAccepted,
            reason_code: "BILL_ALIGNED".into(),
            evidence_reference_ids: Vec::new(),
            operation_id: "operation-1".into(),
            idempotency_key: "decision-key".into(),
        };
        let attempt = process
            .decide_difference_apply("difference-1", request.clone(), &actor, &mut NoTransaction)
            .await;
        assert!(matches!(attempt, Err(Error::Internal(message)) if message == "供应链授权未接线"));
        let recovery =
            process.recover_difference_decision("difference-1", &request, &actor, &mut NoTransaction).await;
        assert!(matches!(recovery, Err(Error::Internal(message)) if message == "供应链授权未接线"));
        let replay = process
            .replay_difference_decision(
                "audit",
                "fingerprint",
                "difference-1",
                "statement-1",
                (&actor, "decision-key"),
                &mut NoTransaction,
            )
            .await;
        assert!(matches!(replay, Err(Error::Internal(message)) if message == "供应链授权未接线"));
    }

    #[test]
    fn difference_receipt_requires_exact_resolved_difference_and_allows_later_statement() {
        let receipt = DifferenceDecisionReceipt {
            operation_id: "op-1".into(),
            statement_id: "statement-1".into(),
            statement_version: 2,
            difference_version: 2,
        };
        ensure_difference_statement(&receipt, "statement-1").unwrap();
        assert!(matches!(
            ensure_difference_statement(&receipt, "statement-2"),
            Err(Error::ConflictError(message)) if message == "差异决定幂等收据与当前结算单不一致"
        ));
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
