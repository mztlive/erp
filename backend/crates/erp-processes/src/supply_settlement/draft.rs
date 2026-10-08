//! W27 供应商结算草稿的服务端来源快照创建与刷新。
use std::sync::Arc;

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_supply::command_receipt::repository::{SupplyCommandReceiptExt, SupplyCommandReceiptReadExt};
use erp_supply::command_receipt::{RefreshReceipt, SupplyCommandResult};
use erp_supply::entity::supplier_settlement::{SupplierSettlementDraftSnapshot, SupplierSettlementStatement};
use erp_supply::service::supplier_fulfillment::receipt::stable_digest;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::draft::{PreparedStatement, StatementPreparation};
use erp_supply::{SettlementAccess, SettlementDataScopePort};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};
use validator::Validate;

use super::{
    CreateSettlementStatementRequest, RefreshSettlementStatementRequest, SettlementDraftAction,
    SettlementDraftCommandResult, SupplierSettlementProcess, command_audit_id, digest_parts,
};
use crate::audit::{persist_log, recover_command};
use crate::supply_execution::receipt::persist_supply_receipt;
use crate::{Error, Result};

impl SupplierSettlementProcess {
    /// 从服务端最新的不可变来源批次创建结算草稿。
    ///
    /// 请求不接收金额明细。服务端逐行构造结算项与差异，并把单头、明细、差异和
    /// 幂等审计置于同一事务中；缺少来源批次时整单失败。
    ///
    /// # 参数
    /// * `req` - 创建草稿命令，不含金额明细。
    /// * `actor` - 当前经办人。
    ///
    /// # 返回
    /// 返回已创建的结算草稿及明细、差异计数；幂等重放返回既有创建结果。
    ///
    /// # 错误
    /// 范围不足、来源批次缺失、准备失败或事务写入失败时返回对应错误。未知提交会按原创建键查证一次。
    pub async fn create_statement(
        &self,
        req: CreateSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
        let prepared = match self.prepare_scoped_statement(&req, actor).await? {
            StatementPreparation::Replay(result) => return Ok(result),
            StatementPreparation::Ready(prepared) => prepared,
        };
        let PreparedStatement { statement, snapshot, statement_no, period_start, period_end } = prepared;
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.create", &statement_no, &req.idempotency_key);
        let audit = create_statement_audit(actor, audit_id, &statement)?;
        let db = self.db.clone();
        let client = db.client().clone();
        let data_scope = self.data_scope.clone();
        let actor_for_tx = actor.clone();
        let owner_id = actor.id().to_string();
        let statement_for_tx = statement.clone();
        let items_for_tx = snapshot.items.clone();
        let differences_for_tx = snapshot.differences.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    erp_supply::SettlementAccess::new(db.clone(), data_scope)
                        .require_create(&actor_for_tx, &owner_id, executor)
                        .await?;
                    SupplierSettlementService::new(db.clone())
                        .persist_statement_with_items(
                            &statement_for_tx,
                            &items_for_tx,
                            &differences_for_tx,
                            executor,
                        )
                        .await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            return recover_command(
                error,
                self.domain()
                    .replay_statement_create(
                        &req,
                        &statement_no,
                        period_start,
                        period_end,
                        &mut NoTransaction,
                    )
                    .await
                    .map_err(Error::from),
            );
        }
        Ok(SettlementDraftCommandResult {
            result_status: "CREATED".to_string(),
            message: "结算草稿已从服务端权威来源快照创建".to_string(),
            request_id: req.request_id,
            statement: statement.into(),
            item_count: snapshot.items.len(),
            difference_count: snapshot.differences.len(),
        })
    }

    async fn prepare_scoped_statement(
        &self,
        req: &CreateSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<StatementPreparation> {
        let (_, org) = self.domain().access().require_create(actor, actor.id(), &mut NoTransaction).await?;
        Ok(self.domain().prepare_statement(req, actor.id(), &org, &mut NoTransaction).await?)
    }

    /// 使用同一供应商、期间和冻结策略下的最新来源批次刷新可编辑草稿。
    ///
    /// 命令要求结算单 CAS 与来源摘要同时匹配。相同来源为受审计的 no-op；新来源
    /// 则在事务内物理替换尚未提交复核的试算明细和差异。
    ///
    /// # 参数
    /// * `id` - 路径中的结算单 ID。
    /// * `req` - 刷新命令。
    /// * `actor` - 当前经办人。
    ///
    /// # 返回
    /// 来源未变时返回原版本的刷新结果；来源更新时返回替换后的草稿。同一命令重放返回原结果。
    ///
    /// # 错误
    /// 请求校验失败、路径与载荷身份不一致、结算单不可更新、准备失败或事务写入失败时返回对应错误。
    pub async fn refresh_statement(
        &self,
        id: &str,
        req: RefreshSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
        req.validate()?;
        let replay_key = req.idempotency_key.clone();
        validate_refresh_identity(id, &req)?;
        let fingerprint = refresh_fingerprint(&req);
        let audit_id = command_audit_id(actor.id(), "supplier_settlement.refresh", id, &req.idempotency_key);
        if let Some(result) =
            self.replay_refresh(&audit_id, &fingerprint, id, (actor.id(), &replay_key)).await?
        {
            return Ok(result);
        }
        self.domain().access().require_statement(actor, "update", id, &mut NoTransaction).await?;
        let prepared = self.domain().prepare_refresh(id, &req, actor.id(), &mut NoTransaction).await?;
        let statement = prepared.statement;
        let Some(snapshot) = prepared.snapshot else {
            return self
                .finish_unchanged_refresh(
                    audit_id,
                    fingerprint,
                    statement,
                    (prepared.item_count, prepared.difference_count),
                    &req,
                    actor,
                )
                .await;
        };
        let receipt = RefreshReceipt::new(req.request_id.clone(), statement.source_snapshot_hash.clone())
            .with_counts(statement.base.version + 1, snapshot.items.len(), snapshot.differences.len());
        let audit = refresh_audit(audit_id.clone(), &statement, actor)?;
        let db = self.db.clone();
        let client = db.client().clone();
        let data_scope = self.data_scope.clone();
        let actor_for_tx = actor.clone();
        let id_for_tx = id.to_string();
        let mut statement_for_tx = statement.clone();
        let snapshot_for_tx = snapshot;
        let receipt_for_tx = receipt.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    persist_refreshed_result(
                        RefreshWrite {
                            db: &db,
                            data_scope,
                            actor: &actor_for_tx,
                            id: &id_for_tx,
                            statement: &mut statement_for_tx,
                            snapshot: &snapshot_for_tx,
                            old_ids: (&prepared.old_item_ids, &prepared.old_difference_ids),
                            request: &req,
                            audit: &audit,
                            fingerprint: &fingerprint_for_tx,
                            receipt: receipt_for_tx,
                        },
                        executor,
                    )
                    .await?;
                    Ok::<SupplierSettlementStatement, crate::Error>(statement_for_tx)
                })
            })
            .await;
        let statement = match transaction_result {
            Ok(statement) => statement,
            Err(error) => {
                return recover_command(
                    error,
                    self.replay_refresh(&audit_id, &fingerprint, id, (actor.id(), &replay_key)).await,
                );
            },
        };
        Ok(refresh_result(statement, receipt, "REFRESHED", "结算试算已刷新为最新权威来源快照"))
    }

    /// 相同来源仍保存原版本、明细计数及同事务审计结果。
    async fn finish_unchanged_refresh(
        &self,
        command_id: String,
        fingerprint: String,
        statement: SupplierSettlementStatement,
        counts: (usize, usize),
        request: &RefreshSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
        let receipt = RefreshReceipt::new(request.request_id.clone(), statement.source_snapshot_hash.clone())
            .with_counts(statement.base.version, counts.0, counts.1);
        self.persist_refresh_audit(
            command_id,
            fingerprint,
            &statement,
            &receipt,
            actor,
            &request.idempotency_key,
        )
        .await?;
        Ok(refresh_result(statement, receipt, "UNCHANGED", "当前已是最新权威来源快照"))
    }

    /// 来源未变化时仍以同一事务登记不可变回执和单次业务事件。
    async fn persist_refresh_audit(
        &self,
        audit_id: String,
        fingerprint: String,
        statement: &SupplierSettlementStatement,
        receipt: &RefreshReceipt,
        actor: &AuditActor,
        key: &str,
    ) -> Result<()> {
        let audit = refresh_audit(audit_id, statement, actor)?;
        let db = self.db.clone();
        let scope = self.data_scope.clone();
        let actor = actor.clone();
        let id = statement.base.id.clone();
        let receipt = receipt.clone();
        let key_for_tx = key.to_string();
        let audit_for_tx = audit.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let current = erp_supply::SettlementAccess::new(db.clone(), scope)
                        .require_statement(&actor, "update", &id, executor)
                        .await?;
                    ensure_refresh_replay(&current, &receipt)?;
                    persist_supply_receipt(
                        &db,
                        &audit_for_tx,
                        &fingerprint_for_tx,
                        &key_for_tx,
                        &id,
                        SupplyCommandResult::Refresh(receipt),
                        executor,
                    )
                    .await?;
                    persist_log(&db, &audit_for_tx, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await;
        match result {
            Ok(()) => Ok(()),
            Err(error) => recover_command(
                error,
                self.replay_refresh(&audit.base.id, &fingerprint, &statement.base.id, (&audit.actor_id, key))
                    .await
                    .map(|result| result.map(|_| ())),
            ),
        }
    }

    async fn replay_refresh(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
        identity: (&str, &str),
    ) -> Result<Option<SettlementDraftCommandResult>> {
        let Some(stored) =
            self.db.supply_command_receipts().find_command(audit_id, &mut NoTransaction).await?
        else {
            return Ok(None);
        };
        stored.verify_identity(
            audit_id,
            identity.0,
            "supplier_settlement.refresh",
            statement_id,
            &stable_digest(identity.1.trim()),
        )?;
        stored.verify(expected_fingerprint, Some(statement_id), "幂等键已用于不同的刷新结算试算命令")?;
        let SupplyCommandResult::Refresh(receipt) = stored.result else {
            return Err(Error::Internal("结算刷新回执类型非法".to_string()));
        };
        let statement = self.domain().load_statement(statement_id, &mut NoTransaction).await?;
        ensure_refresh_replay(&statement, &receipt)?;
        Ok(Some(refresh_result(statement, receipt, "REPLAYED", "结算试算刷新结果已恢复")))
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

/// 刷新回执只恢复同一版本与来源快照。
fn ensure_refresh_replay(statement: &SupplierSettlementStatement, receipt: &RefreshReceipt) -> Result<()> {
    if statement.base.version != receipt.statement_version
        || statement.source_snapshot_hash != receipt.source_snapshot_hash
    {
        return Err(Error::ConflictError("刷新幂等结果已被后续来源快照替代，请读取当前详情".to_string()));
    }
    Ok(())
}

/// 以已准备的结算单保存本次编号和独立命令关联。
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

/// 已准备的来源快照与原请求沿调用方执行器提交。
struct RefreshWrite<'a> {
    db: &'a Database,
    data_scope: Arc<dyn SettlementDataScopePort>,
    actor: &'a AuditActor,
    id: &'a str,
    statement: &'a mut SupplierSettlementStatement,
    snapshot: &'a SupplierSettlementDraftSnapshot,
    old_ids: (&'a [String], &'a [String]),
    request: &'a RefreshSettlementStatementRequest,
    audit: &'a AuditLog,
    fingerprint: &'a str,
    receipt: RefreshReceipt,
}

/// 保持权限重验、正式快照替换、回执与事件的原执行顺序。
async fn persist_refreshed_result(input: RefreshWrite<'_>, executor: &mut dyn Executor) -> Result<()> {
    SettlementAccess::new(input.db.clone(), input.data_scope)
        .require_statement(input.actor, "update", input.id, executor)
        .await?;
    SupplierSettlementService::new(input.db.clone())
        .persist_refreshed_statement(input.statement, input.snapshot, input.old_ids, input.request, executor)
        .await?;
    persist_supply_receipt(
        input.db,
        input.audit,
        input.fingerprint,
        &input.request.idempotency_key,
        input.id,
        SupplyCommandResult::Refresh(input.receipt),
        executor,
    )
    .await?;
    persist_log(input.db, input.audit, executor).await?;
    Ok(())
}

/// 验证刷新动作与路径身份，错误顺序与命令入口一致。
fn validate_refresh_identity(id: &str, req: &RefreshSettlementStatementRequest) -> Result<()> {
    if req.action != SettlementDraftAction::Refresh {
        return Err(Error::ValidationError("刷新结算草稿必须使用 REFRESH 动作".to_string()));
    }
    if id != req.statement_id {
        return Err(Error::ValidationError("结算单路径ID与命令载荷不一致".to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod replay_tests {
    use super::*;
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
