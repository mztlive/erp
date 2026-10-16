//! 登记维持原采购（第一期 §6.3“销售变更对采购的影响”第 5、11 条）。
//!
//! 采购核实无法调整（如供应商不接受减量或撤单）时，由采购负责人登记原因：记录以当前
//! 销售版本作为后续判定基准并标记处理完成，关闭依据更新待办；不改写采购或应付。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_core::ids::SalesOrderId;
use erp_procurement::dto::purchase_order::MaintainPurchaseSourceRequest;
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseSourceUpdate};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_source_update::{find_open_for_purchase, purchase_source_updates};
use erp_returns::entity::returns::PurchaseReturnStatus;
use erp_returns::repository::direct_return::confirmed_direct_returns;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItemStatus;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::PurchaseOrderProcess;
use crate::audit::persist_log;
use crate::{Error, Result};

/// 维持原采购的授权动作。
const MAINTAIN_ACTION: &str = "maintain_source";

impl PurchaseOrderProcess {
    /// 登记维持原采购并关闭依据更新待办。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    /// * `req` - 记录期望版本与无法调整的原因
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 无权限或越权、没有待处理的依据更新、记录或销售版本已变化、原因非法时返回错误。
    ///
    /// # 关键业务约束
    /// 写入前在同一事务内重验采购对象范围与记录版本。
    pub async fn maintain_source(
        &self,
        id: &str,
        req: MaintainPurchaseSourceRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        req.validate()?;
        let access = self.command_access(actor, MAINTAIN_ACTION)?;
        let db = self.db.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = access.current(&id, executor).await?;
                    maintain_in_transaction(&db, &order, &req, &actor, executor).await
                })
            })
            .await
    }
}

/// 在事务内校验并写入维持登记、关闭待办和审计。
async fn maintain_in_transaction(
    db: &Database,
    order: &PurchaseOrder,
    req: &MaintainPurchaseSourceRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut update = find_open_for_purchase(db, &order.base.id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购依据已处理，请刷新后查看".to_string()))?;
    ensure_current(db, order, &update, req.expected_version, executor).await?;
    if confirmed_direct_returns(db, &order.base.id, executor)
        .await?
        .iter()
        .any(|r| r.stable.status == PurchaseReturnStatus::Returned)
    {
        return Err(Error::ConflictError(
            "已登记实际退货，请完成款票处理和采购改单，不能维持原采购金额".into(),
        ));
    }
    // 与实际直退登记竞争同一采购版本，禁止并发维持越过刚写入的退货事实。
    let mut guarded_order = order.clone();
    db.purchase_orders().update(&mut guarded_order, executor).await?;
    let now = Instant::now();
    update.maintain(&req.reason, actor.id(), now).map_err(Error::Logic)?;
    purchase_source_updates(db).update(&mut update, executor).await?;
    close_work_item(db, &update, now, executor).await?;
    let audit = actor.clone().resource_log(
        "purchase_order.maintain_source",
        "purchase_order",
        order.base.id.clone(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 确认记录版本未变且销售单未再次变更。
async fn ensure_current(
    db: &Database,
    order: &PurchaseOrder,
    update: &PurchaseSourceUpdate,
    expected_version: u64,
    executor: &mut dyn Executor,
) -> Result<()> {
    let sales = db
        .sales_orders()
        .find_by_id(&SalesOrderId::new(order.sales_order_id.as_ref()), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    ensure_record_current(update, expected_version, sales.current_revision_id())
}

/// 记录版本与登记时一致，且销售单此后未再次变更。
fn ensure_record_current(
    update: &PurchaseSourceUpdate,
    expected_version: u64,
    current_revision_id: Option<&str>,
) -> Result<()> {
    if update.base.version != expected_version {
        return Err(Error::ConflictError("采购依据已更新，请刷新后重新登记".to_string()));
    }
    if current_revision_id != Some(update.sales_order_revision_id.as_str()) {
        return Err(Error::ConflictError("销售单已再次变更，请刷新后重新登记".to_string()));
    }
    Ok(())
}

/// 关闭记录关联且仍开放的待办。
async fn close_work_item(
    db: &Database,
    update: &PurchaseSourceUpdate,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(id) = update.work_item_id.as_deref() else {
        return Ok(());
    };
    let Some(mut item) = db.work_items().find_by_id(id, executor).await? else {
        return Ok(());
    };
    if item.status != WorkItemStatus::Open {
        return Ok(());
    }
    item.complete_when_source_updated(now).map_err(Error::Logic)?;
    db.work_items().update(&mut item, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseSourceImpact,
        PurchaseSourceUpdate, PurchaseType, SourceImpactKind, SourceImpactReason, SourceUpdateOpening,
    };

    use super::ensure_record_current;
    use crate::Error;

    fn record() -> PurchaseSourceUpdate {
        let order = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("wh-1")),
            },
            "buyer",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        let impact = PurchaseSourceImpact {
            purchase_order_id: "po-1".into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 1,
                item_name: "礼盒".into(),
                kind: SourceImpactKind::Zeroed { baseline_quantity: Quantity::from_str("2").unwrap() },
            }],
        };
        PurchaseSourceUpdate::open(
            "psu-1".into(),
            SourceUpdateOpening {
                order: &order,
                impact: &impact,
                sales_order_revision_id: "rev-2",
                trigger_sales_change_order_id: Some("sco-1"),
            },
        )
    }

    /// 记录版本与销售版本均未变化时允许登记。
    #[test]
    fn current_record_and_revision_allow_maintain() {
        let update = record();

        assert!(ensure_record_current(&update, update.base.version, Some("rev-2")).is_ok());
    }

    /// 记录已刷新或销售单再次变更时拒绝，提示刷新后重新登记。
    #[test]
    fn stale_record_or_changed_sales_is_rejected() {
        let update = record();

        let stale = ensure_record_current(&update, update.base.version + 1, Some("rev-2")).unwrap_err();
        let changed = ensure_record_current(&update, update.base.version, Some("rev-3")).unwrap_err();

        assert!(matches!(stale, Error::ConflictError(message) if message.contains("采购依据已更新")));
        assert!(matches!(changed, Error::ConflictError(message) if message.contains("销售单已再次变更")));
    }
}
