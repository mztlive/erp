use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::AccountKind;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt};
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::ports::connector::common::ActionKey;
use erp_supply::ports::connector::order::OrderReference;
use erp_supply::ports::connector::receipt::{ConfirmReceipt, ReceiptLine};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::evidence::{ReceiptEvidence, fingerprint};
use super::facts;
use crate::audit::persist_log;
use crate::supplier_api_orders::SupplierApiOrderProcess;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn receipt_source_evidence(&self, execution: &ApiOrderExecution) -> Result<()> {
        let (process, execution) = (self.clone(), execution.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { process.load_receipt_evidence(&execution, executor).await.map(|_| ()) })
            })
            .await
    }

    async fn load_receipt_evidence(
        &self,
        execution: &ApiOrderExecution,
        executor: &mut dyn Executor,
    ) -> Result<(facts::LoadedReceiptFacts, ReceiptEvidence)> {
        if execution.pending_order_tree.is_some() {
            return Err(Error::BusinessLogicError("原供应商子单关系仍待核实，不能通知全部收到".into()));
        }
        let tracking =
            self.db.supplier_order_tracking().find_by_id(&execution.base.id, executor).await?.ok_or_else(
                || Error::BusinessLogicError("原执行缺少交付数量关联，请核对原履约记录".into()),
            )?;
        let snapshot = self.decode(execution)?;
        let tree = execution
            .order_tree
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("原供应商真实物流子单尚未核实，不能通知收到".into()))?;
        tree.validate(&snapshot.lines)?;
        let loaded = facts::load(&self.db, execution, &tracking, executor).await?;
        let evidence = ReceiptEvidence::from_facts(execution, &tracking, &snapshot.lines, &loaded.actual)?;
        Ok((loaded, evidence))
    }

    pub(super) async fn claim_receipt(
        &self,
        expected: &ApiOrderExecution,
        target: &OrderReference,
    ) -> Result<(ApiOrderExecution, ConfirmReceipt)> {
        let (process, expected, target) = (self.clone(), expected.clone(), target.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut current = process
                        .db
                        .supplier_api_order_executions()
                        .find_by_id(&expected.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("原收货通知执行不存在".into()))?;
                    if current.base.version != expected.base.version {
                        return Err(Error::ConflictError(
                            "原执行或实收依据已变化，请查询原结果后核验".into(),
                        ));
                    }
                    let (mut loaded, evidence) = process.load_receipt_evidence(&current, executor).await?;
                    let request = current_receipt_request(&current, &target, &evidence)?;
                    if !current.begin_receipt_confirmation(&request)? {
                        return Err(Error::ConflictError("原收货通知已登记，不能重复发送".into()));
                    }
                    loaded.fence(&process.db, executor).await?;
                    if let Some(mut tracking) =
                        process.db.supplier_order_tracking().find_by_id(&current.base.id, executor).await?
                    {
                        process.db.supplier_order_tracking().update(&mut tracking, executor).await?;
                    }
                    current.receipt_next_run_at = Some(super::later());
                    current.receipt_blocker = Some("原收货通知已经登记，等待查询实际结果".into());
                    process.db.supplier_api_order_executions().update(&mut current, executor).await?;
                    receipt_audit(&process.db, &current.base.id, executor).await?;
                    Ok((current, request))
                })
            })
            .await
    }

    pub(super) async fn save_receipt_progress(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        let (db, mut saved) = (self.db.clone(), execution.clone());
        *execution = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let previous = db
                        .supplier_api_order_executions()
                        .find_by_id(&saved.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("原收货通知执行不存在".into()))?;
                    let changed = previous.receipt_actions != saved.receipt_actions
                        || previous.receipt_blocker != saved.receipt_blocker;
                    db.supplier_api_order_executions().update(&mut saved, executor).await?;
                    if changed {
                        receipt_audit(&db, &saved.base.id, executor).await?;
                    }
                    Ok::<_, Error>(saved)
                })
            })
            .await?;
        Ok(())
    }
}

fn current_receipt_request(
    execution: &ApiOrderExecution,
    order: &OrderReference,
    evidence: &ReceiptEvidence,
) -> Result<ConfirmReceipt> {
    execution.ensure_receipt_coverage(&evidence.lines)?;
    let tree = execution
        .order_tree
        .as_ref()
        .ok_or_else(|| Error::BusinessLogicError("原供应商真实叶子尚未核实".into()))?;
    let leaf =
        tree.leaves().into_iter().find(|node| node.external_order_no == order.external_order_no).ok_or_else(
            || Error::BusinessLogicError("原目标已经拆分或不是当前物流子单，请先核实原关系".into()),
        )?;
    let lines = leaf
        .lines
        .iter()
        .map(|line| ReceiptLine {
            line_id: line.line_id.clone(),
            sku: line.sku.clone(),
            quantity: line.quantity,
        })
        .collect::<Vec<_>>();
    let payload_hash = fingerprint(&(order, tree.version, &lines, &evidence.fingerprint))?;
    Ok(ConfirmReceipt {
        action: ActionKey {
            id: format!("{}-receipt-{}", execution.base.id, fingerprint(&order.external_order_no)?),
            payload_hash,
        },
        order: order.clone(),
        topology_version: tree.version,
        lines,
        evidence_fingerprint: evidence.fingerprint.clone(),
    })
}

async fn receipt_audit(db: &Database, execution: &str, executor: &mut dyn Executor) -> Result<()> {
    let actor = AuditActor::new(
        "system:supplier-receipt".into(),
        "system:supplier-receipt".into(),
        AccountKind::Admin,
    )
    .with_actor_name_snapshot(Some("供应商实收通知处理".into()))?;
    let log = actor.resource_log(
        "supplier_fulfillment.receipt_confirmation",
        "supplier_fulfillment_order",
        execution.to_owned(),
    )?;
    persist_log(db, &log, executor).await?;
    Ok(())
}
