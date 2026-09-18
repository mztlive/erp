//! W27 供应商结算草稿的服务端来源快照创建与刷新。
use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_supply::entity::supplier_settlement::SupplierSettlementStatement;
use erp_supply::service::supplier_settlement::SupplierSettlementService;
use erp_supply::service::supplier_settlement::draft::{PreparedStatement, StatementPreparation};
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::{
    CreateSettlementStatementRequest, RefreshSettlementStatementRequest, SettlementDraftAction,
    SettlementDraftCommandResult, SupplierSettlementProcess, command_audit_id, digest_parts,
    ensure_audit_resource, receipt_result,
};
use crate::{Error, Result};

/// 刷新命令的持久化幂等收据。
struct RefreshReceipt {
    request_id: String,
    statement_version: u64,
    source_snapshot_hash: String,
    item_count: usize,
    difference_count: usize,
}

impl RefreshReceipt {
    /// 构造刷新命令收据。
    ///
    /// # 参数
    /// * `request_id` - 幂等请求身份
    /// * `source_snapshot_hash` - 来源快照哈希
    ///
    /// # 返回
    /// 返回版本为一、明细为空的收据。
    fn new(request_id: String, source_snapshot_hash: String) -> Self {
        Self { request_id, statement_version: 1, source_snapshot_hash, item_count: 0, difference_count: 0 }
    }

    /// 设置结算单版本与明细计数。
    ///
    /// # 参数
    /// * `statement_version` - 结算单版本
    /// * `item_count` - 结算明细数
    /// * `difference_count` - 结算差异数
    ///
    /// # 返回
    /// 返回更新后的收据。
    fn with_counts(mut self, statement_version: u64, item_count: usize, difference_count: usize) -> Self {
        self.statement_version = statement_version;
        self.item_count = item_count;
        self.difference_count = difference_count;
        self
    }
}

impl SupplierSettlementProcess {
    /// 从服务端最新的不可变来源批次创建结算草稿。
    ///
    /// 请求不接收金额明细。服务端逐行构造结算项与差异，并把单头、明细、差异和
    /// 幂等审计置于同一事务中；缺少来源批次时整单失败。
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
        let fingerprint = create_fingerprint(&req);
        let audit_id =
            command_audit_id(actor.id(), "supplier_settlement.create", &statement_no, &req.idempotency_key);
        let audit = actor.clone().resource_log_with_id(
            audit_id,
            "supplier_settlement.create",
            "supplier_settlement_statement",
            statement.base.id.clone(),
            Some(format!(
                "command_sha256={fingerprint};result={}|{}|{}|{}",
                req.request_id,
                statement.base.version,
                snapshot.items.len(),
                snapshot.differences.len(),
            )),
        )?;
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
                    db.audit_logs().create(&audit, executor).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await;
        if let Err(error) = transaction_result {
            if let Some(result) = self
                .domain()
                .replay_statement_create(&req, &statement_no, period_start, period_end, &mut NoTransaction)
                .await?
            {
                return Ok(result);
            }
            return Err(error);
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
    pub async fn refresh_statement(
        &self,
        id: &str,
        req: RefreshSettlementStatementRequest,
        actor: &AuditActor,
    ) -> Result<SettlementDraftCommandResult> {
        req.validate()?;
        if req.action != SettlementDraftAction::Refresh {
            return Err(Error::ValidationError("刷新结算草稿必须使用 REFRESH 动作".to_string()));
        }
        if id != req.statement_id {
            return Err(Error::ValidationError("结算单路径ID与命令载荷不一致".to_string()));
        }
        let fingerprint = refresh_fingerprint(&req);
        let audit_id = command_audit_id(actor.id(), "supplier_settlement.refresh", id, &req.idempotency_key);
        if let Some(result) = self.replay_refresh(&audit_id, &fingerprint, id).await? {
            return Ok(result);
        }
        self.domain().access().require_statement(actor, "update", id, &mut NoTransaction).await?;
        let prepared = self.domain().prepare_refresh(id, &req, actor.id(), &mut NoTransaction).await?;
        let erp_supply::service::supplier_settlement::draft::PreparedRefresh {
            statement,
            snapshot,
            old_item_ids,
            old_difference_ids,
            item_count,
            difference_count,
        } = prepared;
        let snapshot = match snapshot {
            Some(snapshot) => snapshot,
            None => {
                let receipt =
                    RefreshReceipt::new(req.request_id.clone(), statement.source_snapshot_hash.clone())
                        .with_counts(statement.base.version, item_count, difference_count);
                self.persist_refresh_audit(audit_id, fingerprint, &statement, &receipt, actor).await?;
                return Ok(refresh_result(statement, receipt, "UNCHANGED", "当前已是最新权威来源快照"));
            },
        };
        let receipt = RefreshReceipt::new(req.request_id.clone(), statement.source_snapshot_hash.clone())
            .with_counts(statement.base.version + 1, snapshot.items.len(), snapshot.differences.len());
        let audit = refresh_audit(audit_id.clone(), &fingerprint, &statement, &receipt, actor)?;
        let db = self.db.clone();
        let client = db.client().clone();
        let data_scope = self.data_scope.clone();
        let actor_for_tx = actor.clone();
        let id_for_tx = id.to_string();
        let mut statement_for_tx = statement.clone();
        let snapshot_for_tx = snapshot;
        let transaction_result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    erp_supply::SettlementAccess::new(db.clone(), data_scope)
                        .require_statement(&actor_for_tx, "update", &id_for_tx, executor)
                        .await?;
                    SupplierSettlementService::new(db.clone())
                        .persist_refreshed_statement(
                            &mut statement_for_tx,
                            &snapshot_for_tx,
                            (&old_item_ids, &old_difference_ids),
                            &req,
                            executor,
                        )
                        .await?;
                    db.audit_logs().create(&audit, executor).await?;
                    Ok::<SupplierSettlementStatement, crate::Error>(statement_for_tx)
                })
            })
            .await;
        let statement = match transaction_result {
            Ok(statement) => statement,
            Err(error) => {
                if let Some(result) = self.replay_refresh(&audit_id, &fingerprint, id).await? {
                    return Ok(result);
                }
                return Err(error);
            },
        };
        Ok(refresh_result(statement, receipt, "REFRESHED", "结算试算已刷新为最新权威来源快照"))
    }

    async fn persist_refresh_audit(
        &self,
        audit_id: String,
        fingerprint: String,
        statement: &SupplierSettlementStatement,
        receipt: &RefreshReceipt,
        actor: &AuditActor,
    ) -> Result<()> {
        let audit = refresh_audit(audit_id, &fingerprint, statement, receipt, actor)?;
        match self.db.audit_logs().create(&audit, &mut NoTransaction).await {
            Ok(()) => Ok(()),
            Err(error) => {
                if self.replay_refresh(&audit.base.id, &fingerprint, &statement.base.id).await?.is_some() {
                    Ok(())
                } else {
                    Err(error.into())
                }
            },
        }
    }

    async fn replay_refresh(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        statement_id: &str,
    ) -> Result<Option<SettlementDraftCommandResult>> {
        let Some(audit) = self.db.audit_logs().find_by_id(audit_id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        ensure_audit_resource(&audit, statement_id)?;
        let message =
            audit.message.as_deref().ok_or_else(|| Error::Internal("刷新幂等收据缺少结果".to_string()))?;
        let receipt = parse_refresh_receipt(message, expected_fingerprint)?;
        let statement = self.domain().load_statement(statement_id, &mut NoTransaction).await?;
        ensure_refresh_replay(&statement, &receipt)?;
        Ok(Some(refresh_result(statement, receipt, "REPLAYED", "结算试算刷新结果已恢复")))
    }
}

fn create_fingerprint(req: &CreateSettlementStatementRequest) -> String {
    digest_parts(&[
        "CREATE".to_string(),
        req.request_id.clone(),
        req.supplier_id.to_string(),
        req.period_start.trim().to_string(),
        req.period_end.trim().to_string(),
    ])
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
    fingerprint: &str,
    statement: &SupplierSettlementStatement,
    receipt: &RefreshReceipt,
    actor: &AuditActor,
) -> Result<erp_audit::AuditLog> {
    actor
        .clone()
        .resource_log_with_id(
            audit_id,
            "supplier_settlement.refresh",
            "supplier_settlement_statement",
            statement.base.id.clone(),
            Some(format!(
                "command_sha256={fingerprint};result={}|{}|{}|{}|{}",
                receipt.request_id,
                receipt.statement_version,
                receipt.source_snapshot_hash,
                receipt.item_count,
                receipt.difference_count,
            )),
        )
        .map_err(Into::into)
}

fn parse_refresh_receipt(message: &str, fingerprint: &str) -> Result<RefreshReceipt> {
    let fields = receipt_result(message, fingerprint, "刷新结算试算")?.split('|').collect::<Vec<_>>();
    let [request_id, version, source_hash, item_count, difference_count] = fields.as_slice() else {
        return Err(Error::Internal("刷新结算试算幂等收据非法".to_string()));
    };
    Ok(RefreshReceipt {
        request_id: (*request_id).to_string(),
        statement_version: parse_positive_u64(version, "结算单版本")?,
        source_snapshot_hash: (*source_hash).to_string(),
        item_count: parse_usize(item_count, "结算明细数")?,
        difference_count: parse_usize(difference_count, "结算差异数")?,
    })
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

fn parse_positive_u64(value: &str, field: &str) -> Result<u64> {
    let value = value.parse::<u64>().map_err(|_| Error::Internal(format!("刷新收据{field}非法")))?;
    if value == 0 {
        return Err(Error::Internal(format!("刷新收据{field}非法")));
    }
    Ok(value)
}

fn parse_usize(value: &str, field: &str) -> Result<usize> {
    value.parse::<usize>().map_err(|_| Error::Internal(format!("刷新收据{field}非法")))
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
