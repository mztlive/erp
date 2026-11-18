//! W27 供应商结算草稿的服务端来源快照创建与刷新。

use application_core::AuditActor;
use erp_audit::execution::{persist_log, recover_command};
use erp_audit::{AuditActorLogs, AuditLog};
use erp_fulfillment_execution::supply_execution::receipt::persist_supply_receipt;
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{RefreshReceipt, SupplyCommandResult};
use erp_supply::entity::supplier_settlement::SupplierSettlementStatement;
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_settlement::draft::{PreparedStatement, StatementPreparation};
use erp_supply::service::supplier_settlement::{SettlementAction, SupplierSettlementService};
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::{
    CreateSettlementStatementRequest, RefreshSettlementStatementRequest, SettlementDraftAction,
    SettlementDraftCommandResult, SupplierSettlementProcess, command_audit_id, digest_parts,
};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> SupplierSettlementProcess<A> {
    /// 从服务端最新的不可变来源批次试算并创建供应商结算草稿，客户端不提供逐行金额。
    /// 草稿、明细、差异和审计同事务保存，并返回明细和差异计数。
    ///
    /// 没有来源批次或无权在当前范围创建时拒绝；同一命令返回原创建结果，未知提交按原创建键查证一次。
    pub async fn create_statement(
        &self,
        req: CreateSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
        let process = self.clone();
        let command = req.clone();
        let writer = actor.clone();
        let result = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.create_statement_apply(command, &writer, executor).await })
            })
            .await;

        let Err(error) = result else { return result };
        let process = self.clone();
        let actor = actor.clone();
        let recovered = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    match process.prepare_scoped_statement(&req, &actor, executor).await? {
                        StatementPreparation::Replay(result) => Ok(Some(result)),
                        StatementPreparation::Ready(_) => Ok(None),
                    }
                })
            })
            .await;
        recover_command(error, recovered)
    }

    // 在创建事务内核验创建范围并准备试算；已命中回放则返回原草稿，否则重验范围并原子写入草稿、明细、差异和审计。
    async fn create_statement_apply(
        &self,
        req: CreateSettlementStatementRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<SettlementDraftCommandResult> {
        let prepared = match self.prepare_scoped_statement(&req, actor, executor).await? {
            StatementPreparation::Replay(result) => return Ok(result),
            StatementPreparation::Ready(prepared) => prepared,
        };

        let PreparedStatement { statement, snapshot, statement_no, .. } = prepared;
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.create", &statement_no, &req.idempotency_key);
        let audit = create_statement_audit(actor, audit_id, &statement)?;
        let item_count = snapshot.items.len();
        let difference_count = snapshot.differences.len();

        self.data_scope.require_create(actor, actor.id(), executor).await?;
        SupplierSettlementService::new(self.db.clone())
            .persist_statement_with_items(&statement, &snapshot.items, &snapshot.differences, executor)
            .await?;
        persist_log(&self.db, &audit, executor).await?;

        Ok(SettlementDraftCommandResult {
            result_status: "CREATED".to_string(),
            message: "已按当前结算来源资料创建草稿".to_string(),
            request_id: req.request_id,
            statement: statement.into(),
            item_count,
            difference_count,
        })
    }

    async fn prepare_scoped_statement(
        &self,
        req: &CreateSettlementStatementRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<StatementPreparation> {
        let org = self.domain().access().require_create(actor, actor.id(), executor).await?;
        Ok(self.domain().prepare_statement(req, actor.id(), &org, executor).await?)
    }

    /// 为未提交复核的草稿按同一供应商、期间和冻结策略拉取最新来源，先核对路径、单据版本及旧来源摘要。
    ///
    /// 来源相同只登记幂等结果和审计，不改变版本；来源更新则在同一事务中物理替换原试算明细和差异。
    /// 同一命令只在草稿版本和来源仍匹配时重放，校验或写入失败不提交本次更新。
    pub async fn refresh_statement(
        &self,
        id: &str,
        req: RefreshSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
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
                    process.refresh_statement_apply(&write_id, command, &writer, executor).await
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
                Box::pin(async move { process.recover_refresh(&id, &req, &actor, executor).await })
            })
            .await;
        recover_command(error, recovered)
    }

    // 写入失败后使用恢复事务，按原操作人、原命令资格和幂等身份查询已提交刷新结果，不重新替换明细。
    async fn recover_refresh(
        &self,
        id: &str,
        req: &RefreshSettlementStatementRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDraftCommandResult>> {
        let fingerprint = refresh_fingerprint(req);
        let audit_id = command_audit_id(actor.id(), "supplier_settlement.refresh", id, &req.idempotency_key);
        self.replay_refresh(&audit_id, &fingerprint, id, (actor, &req.idempotency_key), executor).await
    }

    // 在原事务重验草稿刷新权限并优先查回执，再按新旧来源摘要区分保留原版与替换试算结果，防止重复刷新。
    async fn refresh_statement_apply(
        &self,
        id: &str,
        req: RefreshSettlementStatementRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<SettlementDraftCommandResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        validate_refresh_identity(id, &req)?;
        let fingerprint = refresh_fingerprint(&req);
        let audit_id = command_audit_id(actor.id(), "supplier_settlement.refresh", id, &req.idempotency_key);

        if let Some(result) =
            self.replay_refresh(&audit_id, &fingerprint, id, (actor, &replay_key), executor).await?
        {
            return Ok(result);
        }

        let prepared = self.domain().prepare_refresh(id, &req, actor, executor).await?;
        let mut statement = prepared.statement;
        let Some(snapshot) = prepared.snapshot else {
            return self
                .finish_unchanged_refresh(
                    (audit_id, fingerprint),
                    statement,
                    (prepared.item_count, prepared.difference_count),
                    &req,
                    actor,
                    executor,
                )
                .await;
        };

        let receipt = RefreshReceipt::new(req.request_id.clone(), statement.source_snapshot_hash.clone())
            .with_counts(statement.base.version + 1, snapshot.items.len(), snapshot.differences.len());
        let audit = refresh_audit(audit_id, &statement, actor)?;
        self.data_scope.require_action(actor, SettlementAction::Refresh, id, executor).await?;
        SupplierSettlementService::new(self.db.clone())
            .persist_refreshed_statement(
                &mut statement,
                &snapshot,
                (&prepared.old_item_ids, &prepared.old_difference_ids),
                &req,
                executor,
            )
            .await?;

        persist_supply_receipt(
            &self.db,
            &audit,
            &fingerprint,
            &req.idempotency_key,
            id,
            SupplyCommandResult::Refresh(receipt.clone()),
            executor,
        )
        .await
        .map_err(Error::from)?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(refresh_result(statement, receipt, "REFRESHED", "已按最新结算来源资料更新试算"))
    }

    // 来源相同时保留草稿原版本，记录明细和差异计数及审计，使不改业务数据的刷新也能恢复原结果。
    async fn finish_unchanged_refresh(
        &self,
        identity: (String, String),
        statement: SupplierSettlementStatement,
        counts: (usize, usize),
        request: &RefreshSettlementStatementRequest,
        actor: &AuditActor,

        executor: &mut dyn Executor,
    ) -> Result<SettlementDraftCommandResult> {
        let receipt = RefreshReceipt::new(request.request_id.clone(), statement.source_snapshot_hash.clone())
            .with_counts(statement.base.version, counts.0, counts.1);
        self.persist_refresh_audit(identity, &statement, &receipt, actor, &request.idempotency_key, executor)
            .await?;
        Ok(refresh_result(statement, receipt, "UNCHANGED", "结算试算已使用最新来源资料，无需更新"))
    }

    // 用当前执行器先保存不可变刷新回执再写审计；来源无变化时也记录本次刷新，供幂等重试查证。
    async fn persist_refresh_audit(
        &self,
        identity: (String, String),
        statement: &SupplierSettlementStatement,
        receipt: &RefreshReceipt,
        actor: &AuditActor,
        key: &str,

        executor: &mut dyn Executor,
    ) -> Result<()> {
        let (audit_id, fingerprint) = identity;
        let audit = refresh_audit(audit_id, statement, actor)?;
        self.data_scope
            .require_action(actor, SettlementAction::Refresh, &statement.base.id, executor)
            .await?;
        let current = self.domain().load_statement(&statement.base.id, executor).await?;
        ensure_refresh_replay(&current, receipt)?;

        persist_supply_receipt(
            &self.db,
            &audit,
            &fingerprint,
            key,
            &statement.base.id,
            SupplyCommandResult::Refresh(receipt.clone()),
            executor,
        )
        .await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(())
    }

    // 检查真实账号当前是否可刷新该单，然后核对回执身份、请求指纹、结算版本和来源摘要；已推进到新快照时不返回旧结果。
    async fn replay_refresh(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
        identity: (&AuditActor, &str),
        executor: &mut dyn Executor,
    ) -> Result<Option<SettlementDraftCommandResult>> {
        let (actor, key) = identity;
        self.data_scope.require_action(actor, SettlementAction::Refresh, statement_id, executor).await?;

        let Some(stored) = self.db.supply_command_receipts().find_command(audit_id, executor).await? else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            actor.id(),
            "supplier_settlement.refresh",
            statement_id,
            &stable_digest(key.trim()),
        )?;
        stored.verify(
            expected_fingerprint,
            Some(statement_id),
            "本次操作编号已用于其他结算试算刷新，请核对原办理记录",
        )?;
        let SupplyCommandResult::Refresh(receipt) = stored.result else {
            return Err(Error::Internal("结算刷新回执类型非法".to_string()));
        };

        let statement = self.domain().load_statement(statement_id, executor).await?;
        ensure_refresh_replay(&statement, &receipt)?;
        Ok(Some(refresh_result(statement, receipt, "REPLAYED", "已查询到上次刷新试算的结果")))
    }
}

fn refresh_fingerprint(req: &RefreshSettlementStatementRequest) -> String {
    digest_parts(&[
        "REFRESH".to_string(),
        req.request_id.clone(),
        req.statement_id.clone(),
        req.expected_lock_version.to_string(),
        req.expected_source_snapshot_hash.clone(),
    ])
}

fn refresh_audit(
    audit_id: String,
    statement: &SupplierSettlementStatement,
    actor: &AuditActor,
) -> Result<AuditLog> {
    Ok(actor
        .clone()
        .resource_log_with_id(
            audit_id.clone(),
            "supplier_settlement.refresh",
            "supplier_settlement_statement",
            statement.base.id.clone(),
            Some("结算来源快照刷新结果已登记".to_string()),
        )?
        .with_command_id(Some(audit_id))?
        .with_resource_number(Some(statement.statement_no.clone()))?)
}

fn refresh_result(
    statement: SupplierSettlementStatement,
    receipt: RefreshReceipt,
    result_status: &str,
    message: &str,
) -> SettlementDraftCommandResult {
    SettlementDraftCommandResult {
        result_status: result_status.to_string(),
        message: message.to_string(),
        request_id: receipt.request_id,
        statement: statement.into(),
        item_count: receipt.item_count,
        difference_count: receipt.difference_count,
    }
}

// 只接受仍与回执版本和来源摘要一致的草稿，避免把再次刷新后的草稿误当作原刷新结果。
fn ensure_refresh_replay(statement: &SupplierSettlementStatement, receipt: &RefreshReceipt) -> Result<()> {
    if statement.base.version != receipt.statement_version
        || statement.source_snapshot_hash != receipt.source_snapshot_hash
    {
        return Err(Error::ConflictError("结算来源资料已再次更新，请查看当前结算单详情".to_string()));
    }
    Ok(())
}

// 用新草稿的编号和稳定命令身份构造创建审计，便于按单据和原操作追溯。
fn create_statement_audit(
    actor: &AuditActor,
    command_id: String,
    statement: &SupplierSettlementStatement,
) -> Result<AuditLog> {
    let audit = actor
        .clone()
        .resource_log_with_id(
            command_id.clone(),
            "supplier_settlement.create",
            "supplier_settlement_statement",
            statement.base.id.clone(),
            Some("结算草稿已从服务端来源快照创建".to_string()),
        )?
        .with_command_id(Some(command_id))?
        .with_resource_number(Some(statement.statement_no.clone()))?;
    Ok(audit)
}

// 先确认请求动作为刷新，再检查路径与载荷是否指向同一结算单，保持入口错误顺序并防止更新错单。
fn validate_refresh_identity(id: &str, req: &RefreshSettlementStatementRequest) -> Result<()> {
    if req.action != SettlementDraftAction::Refresh {
        return Err(Error::ValidationError("请在结算草稿详情中选择刷新试算".to_string()));
    }
    if id != req.statement_id {
        return Err(Error::ValidationError("本次操作与当前打开的结算单不符，请重新选择结算单".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod replay_tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;

    #[tokio::test]
    async fn refresh_first_attempt_and_recovery_require_command_access_before_receipt_reads() {
        let process = super::super::tests::unwired_process().await;
        let actor = AuditActor::new("preparer".into(), "preparer-login".into(), AccountKind::Supplier);
        let request = RefreshSettlementStatementRequest {
            action: SettlementDraftAction::Refresh,
            statement_id: "statement-1".into(),
            expected_lock_version: 1,
            expected_source_snapshot_hash: "a".repeat(64),
            request_id: "request-1".into(),
            idempotency_key: "refresh-key".into(),
        };

        let attempt =
            process.refresh_statement_apply("statement-1", request.clone(), &actor, &mut NoTransaction).await;
        assert!(matches!(attempt, Err(Error::Internal(message)) if message == "供应链授权未接线"));

        let recovery = process.recover_refresh("statement-1", &request, &actor, &mut NoTransaction).await;
        assert!(matches!(recovery, Err(Error::Internal(message)) if message == "供应链授权未接线"));

        let replay = process
            .replay_refresh(
                "audit",
                "fingerprint",
                "statement-1",
                (&actor, "refresh-key"),
                &mut NoTransaction,
            )
            .await;
        assert!(matches!(replay, Err(Error::Internal(message)) if message == "供应链授权未接线"));
    }

    #[test]
    fn refresh_receipt_rejects_both_older_and_newer_versions_and_replaced_hash() {
        let mut statement = super::super::tests::sample_statement();
        let receipt =
            RefreshReceipt::new("req-1".into(), statement.source_snapshot_hash.clone()).with_counts(2, 1, 0);
        for version in [1, 2, 3] {
            statement.base.version = version;
            assert_eq!(ensure_refresh_replay(&statement, &receipt).is_ok(), version == 2);
        }

        statement.base.version = 2;
        statement.source_snapshot_hash = "c".repeat(64);
        assert!(ensure_refresh_replay(&statement, &receipt).is_err());
    }
}
