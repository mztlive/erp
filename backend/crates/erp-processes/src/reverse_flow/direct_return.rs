//! 已生效销售减量的客户直退登记：原发货保留，数量凭据与双向退货单原子追加。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseReturnLineId, PurchaseReturnOrderId, SalesReturnCaseId, SalesReturnLineId};
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_procurement::entity::purchase_order::{PurchaseOrder, ReturnSettlementDecision};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_source_update::{find_open_for_purchase, purchase_source_updates};
use erp_read_models::purchase_center::direct_return::{DirectReturnPlan, direct_return_plan};
use erp_returns::entity::returns::{
    CaseType, DirectReturnEvidence, DirectReturnRequest, PurchaseReturnLine, PurchaseReturnLineData,
    PurchaseReturnOrder, PurchaseReturnOrderData, PurchaseReturnStatus, ReturnMode, ReturnRoute,
    SalesReturnCase, SalesReturnCaseData, SalesReturnCaseStatus, SalesReturnLine, SalesReturnLineData,
    SupplierRefundStatus,
};
use erp_returns::repository::ReturnsExt;
use erp_returns::repository::direct_return::{direct_return_command, purchase_settlements};
use erp_sales::repository::SalesOrderExt;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::ReturnsProcess;
use super::purchase_return::register_created_purchase_return_document;
use super::return_finance::sync_return_finance;
use super::sales_return::register_created_sales_return_document;
use crate::adapters::purchase_access;
use crate::audit::persist_log;
use crate::{Error, Result};

impl ReturnsProcess {
    /// 保存采购确认的剩余款项方案并原子交接财务任务。
    /// # 参数
    /// * `id` - 采购身份。
    /// * `actor` - 采购经办人。
    /// # 返回
    /// 已有开放任务复用，不登记资金或票据。
    /// # 错误
    /// 无权修改采购、责任配置缺失或并发冲突时拒绝。
    pub async fn reconcile_return_finance(
        &self,
        id: &str,
        decision: ReturnSettlementDecision,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut order = purchase_access(db.clone(), rbac)
                        .require_object(
                            &actor,
                            "update",
                            &id,
                            &["purchase_return_order:create".into()],
                            executor,
                        )
                        .await?;
                    if !save_settlement_decision(&db, &order, &decision, executor).await? {
                        return Ok(());
                    }
                    db.purchase_orders().update(&mut order, executor).await?;
                    sync_return_finance(&db, &id, executor).await?;
                    let log = actor.resource_log("purchase_return_finance.sync", "purchase_order", id)?;
                    persist_log(&db, &log, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    /// 核实客户直退供应商，追加实际退回凭据，保留采购依据待办到改单生效。
    /// # 参数
    /// * `id` - 原采购身份。
    /// * `req` - 原版本、同键请求、实际退回数量与收货凭据。
    /// * `actor` - 当前经办采购。
    /// # 返回
    /// 返回稳定采购退货身份；同键同载荷重放不追加事实。
    /// # 错误
    /// 无采购写资格、异载荷重试、版本过期、超量或缺收货依据时拒绝。
    pub async fn confirm_direct_return(
        &self,
        id: &str,
        req: DirectReturnRequest,
        actor: &AuditActor,
    ) -> Result<String> {
        req.validate()?;
        let id = id.to_owned();
        let actor = actor.clone();
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let object_read = self.object_read.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = purchase_access(db.clone(), rbac.clone())
                        .require_object(
                            &actor,
                            "update",
                            &id,
                            &["purchase_return_order:create".into()],
                            executor,
                        )
                        .await?;
                    let number = format!("PR-{}", req.idempotency_key);
                    if let Some(existing) = direct_return_command(&db, &number, executor).await? {
                        ensure_replay(&existing, &id, &req, &actor)?;
                        return Ok(existing.base.id);
                    }
                    let plan = prepare(&db, &order, &req, &actor, executor).await?;
                    let (return_order, sales_case) = build_heads(&order, &req, &plan, &actor, number)?;
                    register_created_sales_return_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
                        &sales_case,
                        &actor,
                        executor,
                    )
                    .await?;
                    register_created_purchase_return_document(
                        &db,
                        &rbac,
                        object_read.as_ref(),
                        &return_order,
                        &actor,
                        executor,
                    )
                    .await?;
                    persist(&db, &return_order, &sales_case, &plan, &req, &actor, executor).await?;
                    sync_return_finance(&db, &id, executor).await?;
                    Ok(return_order.base.id)
                })
            })
            .await
    }
}

fn ensure_replay(
    existing: &PurchaseReturnOrder,
    order_id: &str,
    req: &DirectReturnRequest,
    actor: &AuditActor,
) -> Result<()> {
    if existing.purchase_order_id.as_ref() != order_id
        || existing.stable.created_by != actor.id()
        || existing.direct_return.as_ref().map(|e| &e.request) != Some(req)
    {
        return Err(Error::ConflictError("该退货提交标识已用于其他内容，请核对原登记".into()));
    }
    Ok(())
}

async fn prepare(
    db: &Database,
    order: &PurchaseOrder,
    req: &DirectReturnRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<DirectReturnPlan> {
    order.revision_id_for_change()?;
    if order.base.version != req.expected_version {
        return Err(Error::ConflictError("采购版本已变化，请刷新后重新核对退货".into()));
    }
    let update = find_open_for_purchase(db, &order.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("本采购没有待处理的销售依据更新".into()))?;
    let plan = direct_return_plan(db, order, executor).await?;
    if update.sales_order_revision_id != plan.sales_revision_id {
        return Err(Error::ConflictError("销售已再次变更，请刷新采购依据".into()));
    }
    let limits =
        plan.lines.iter().map(|l| (l.purchase_line_id.as_str(), l.max_direct_return)).collect::<Vec<_>>();
    req.check_limits(&limits)?;
    // 与销售变更和采购改单共同竞争来源销售 guard，防止并发超退。
    let mut sales = db
        .sales_orders()
        .find_by_id(order.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".into()))?;
    sales.advance_procurement_guard(actor.id())?;
    db.sales_orders().update(&mut sales, executor).await?;
    let mut guarded_order = order.clone();
    db.purchase_orders().update(&mut guarded_order, executor).await?;
    Ok(plan)
}

fn build_heads(
    order: &PurchaseOrder,
    req: &DirectReturnRequest,
    plan: &DirectReturnPlan,
    actor: &AuditActor,
    number: String,
) -> Result<(PurchaseReturnOrder, SalesReturnCase)> {
    let case_id = SalesReturnCaseId::new(next_id());
    let mut sales_case = SalesReturnCase::new(
        case_id.clone(),
        SalesReturnCaseData {
            return_no: format!("SR-{}", req.idempotency_key),
            sales_order_id: order.sales_order_id.clone(),
            acceptance_id: None,
            case_type: CaseType::Return,
            reason: req.reason.clone(),
            discovered_at: Instant::now(),
            return_route: ReturnRoute::DirectToSupplier,
        },
        actor.id(),
    )?;
    sales_case.stable.status = SalesReturnCaseStatus::PendingFinance;
    let mut purchase = PurchaseReturnOrder::new(
        PurchaseReturnOrderId::new(next_id()),
        PurchaseReturnOrderData {
            purchase_return_no: number,
            purchase_order_id: order.base.id.clone().into(),
            sales_return_case_id: Some(case_id),
            return_mode: ReturnMode::DirectToSupplier,
        },
        actor.id(),
    )?;
    purchase.stable.status = PurchaseReturnStatus::Returned;
    let returned_lines = req
        .lines
        .iter()
        .map(|r| {
            let line = plan
                .lines
                .iter()
                .find(|l| l.purchase_line_id == r.purchase_line_id)
                .ok_or_else(|| Error::ValidationError("退货行不属于当前采购".into()))?;
            Ok((line.sales_line_id.clone(), r.quantity))
        })
        .collect::<Result<Vec<_>>>()?;
    purchase.direct_return = Some(DirectReturnEvidence {
        request: req.clone(),
        sales_revision_id: plan.sales_revision_id.clone(),
        returned_lines,
    });
    Ok((purchase, sales_case))
}

async fn persist(
    db: &Database,
    order: &PurchaseReturnOrder,
    case: &SalesReturnCase,
    plan: &DirectReturnPlan,
    req: &DirectReturnRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.sales_return_cases().create(case, executor).await?;
    db.purchase_return_orders().create(order, executor).await?;
    for requested in &req.lines {
        let option = plan
            .lines
            .iter()
            .find(|l| l.purchase_line_id == requested.purchase_line_id)
            .ok_or_else(|| Error::ValidationError("退货行不属于当前采购".into()))?;
        let sales_line = SalesReturnLine::new(
            SalesReturnLineId::new(next_id()),
            SalesReturnLineData {
                sales_return_case_id: case.base.id.clone().into(),
                sales_order_line_id: option.sales_line_id.clone().into(),
                requested_quantity: requested.quantity,
                received_quantity: Some(requested.quantity),
                quality_result: None,
                restockable_quantity: None,
            },
        )?;
        let purchase_line = PurchaseReturnLine::new(
            PurchaseReturnLineId::new(next_id()),
            PurchaseReturnLineData {
                purchase_return_order_id: order.base.id.clone().into(),
                purchase_order_revision_line_id: requested.purchase_line_id.clone().into(),
                return_quantity: requested.quantity,
                warehouse_id: None,
            },
        )?;
        db.sales_return_lines().create(&sales_line, executor).await?;
        db.purchase_return_lines().create(&purchase_line, executor).await?;
    }
    let audit = actor.clone().resource_log(
        "purchase_return_order.confirm_direct",
        "purchase_return_order",
        order.base.id.clone(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

async fn save_settlement_decision(
    db: &Database,
    order: &PurchaseOrder,
    decision: &ReturnSettlementDecision,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let mut update = find_open_for_purchase(db, &order.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购依据更新已经结束".into()))?;
    if let Some(existing) = &update.return_settlement {
        if existing == decision {
            return Ok(false);
        }
        if existing.idempotency_key == decision.idempotency_key
            || update
                .return_settlement_history
                .iter()
                .any(|row| row.idempotency_key == decision.idempotency_key)
        {
            return Err(Error::ConflictError("提交标识已用于其他方案，请刷新后重新提交".into()));
        }
        if purchase_settlements(db, &order.base.id, executor)
            .await?
            .iter()
            .any(|row| row.status == SupplierRefundStatus::InApproval)
        {
            return Err(Error::ConflictError(
                "财务仍有款项在审批中，请由经办人撤回后再修改剩余款项方案".into(),
            ));
        }
    }
    let plan = direct_return_plan(db, order, executor).await?;
    if !plan.pending_settlement || plan.lines.iter().any(|line| !line.required_return.to_decimal().is_zero())
    {
        return Err(Error::ConflictError("请先确认供应商已收到本次应退货物".into()));
    }
    let account = db
        .payable_accounts()
        .find_by_purchase_order(&order.base.id.clone().into(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购应付不存在".into()))?;
    let excess = ReturnSettlementDecision::excess(account.settled_total, plan.target_gross);
    decision.validate(&plan.sales_revision_id, plan.target_gross, excess)?;
    if let Some(previous) = update.return_settlement.replace(decision.clone()) {
        update.return_settlement_history.push(previous);
    }
    purchase_source_updates(db).update(&mut update, executor).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::AccountKind;
    use erp_core::ids::PurchaseOrderId;
    use erp_core::money::Quantity;
    use erp_returns::entity::returns::DirectReturnLine;

    use super::*;
    #[test]
    fn replay_checks_original_actor_order_and_entire_receipt_payload() {
        let actor = AuditActor::new("buyer".into(), "采购".into(), AccountKind::Admin);
        let req = DirectReturnRequest {
            expected_version: 1,
            idempotency_key: "12345678-1234-1234-1234-123456789abc".into(),
            reason: "销售减量".into(),
            receipt_reference: "收货回单".into(),
            lines: vec![DirectReturnLine {
                purchase_line_id: "line".into(),
                quantity: Quantity::from_str("27").unwrap(),
            }],
        };
        let mut order = PurchaseReturnOrder::new(
            PurchaseReturnOrderId::new("return"),
            PurchaseReturnOrderData {
                purchase_return_no: "PR-1".into(),
                purchase_order_id: PurchaseOrderId::new("po"),
                sales_return_case_id: None,
                return_mode: ReturnMode::DirectToSupplier,
            },
            actor.id(),
        )
        .unwrap();
        order.direct_return = Some(DirectReturnEvidence {
            request: req.clone(),
            sales_revision_id: "sr2".into(),
            returned_lines: vec![],
        });
        ensure_replay(&order, "po", &req, &actor).unwrap();
        order.stable.status = PurchaseReturnStatus::Completed;
        ensure_replay(&order, "po", &req, &actor).unwrap();
        assert!(ensure_replay(&order, "other", &req, &actor).is_err());
        let other = AuditActor::new("other".into(), "他人".into(), AccountKind::Admin);
        assert!(ensure_replay(&order, "po", &req, &other).is_err());
        let mut changed = req.clone();
        changed.receipt_reference = "另一回单".into();
        assert!(ensure_replay(&order, "po", &changed, &actor).is_err());
        changed = req.clone();
        changed.lines[0].quantity = Quantity::from_str("20").unwrap();
        assert!(ensure_replay(&order, "po", &changed, &actor).is_err());
        changed = req;
        changed.expected_version = 2;
        assert!(ensure_replay(&order, "po", &changed, &actor).is_err());
    }
}
