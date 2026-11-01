//! 采购提交、最终审批与采购变更生效前的来源复验（第一期 §6.3“销售变更对采购的影响”
//! 第 7 条）。
//!
//! 采购仍引用当前销售版本时直接通过；引用旧版本时按当前事实判定，只有受影响才拒绝。

use erp_core::ids::SalesOrderId;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_read_models::purchase_center::source_impact::{PurchaseSourceState, load_purchase_source_state};
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 确认采购不受来源销售变更影响。
///
/// # 参数
/// * `db` - 业务数据库
/// * `order` - 已持久化当前提交或版本的采购单
/// * `message` - 受影响时返回给用户的下一步说明
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 采购引用当前销售版本或未受影响时返回 `Ok(())`。
///
/// # 错误
/// 来源销售单不存在、覆盖事实不完整、仓储失败，或采购受影响时返回错误。
pub(super) async fn ensure_source_unaffected(
    db: &Database,
    order: &PurchaseOrder,
    message: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let sales = db
        .sales_orders()
        .find_by_id(&SalesOrderId::new(order.sales_order_id.as_ref()), executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    if references_current(sales.current_revision_id(), order) {
        return Ok(());
    }
    let state = load_purchase_source_state(db, &sales, executor).await?;
    reject_if_affected(&state, order, message)
}

/// 采购是否仍引用销售当前版本；是则无需重算覆盖。
fn references_current(current_revision_id: Option<&str>, order: &PurchaseOrder) -> bool {
    current_revision_id == Some(order.sales_order_revision_id.as_ref())
}

/// 按当前事实判定，采购受影响时拒绝并返回下一步说明。
fn reject_if_affected(state: &PurchaseSourceState, order: &PurchaseOrder, message: &str) -> Result<()> {
    if state.impact_of(&order.base.id).is_some() {
        return Err(Error::ConflictError(message.to_string()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use erp_procurement::entity::facts::{
        FactIdentity, PaymentTermFact, SalesCustomerSnapshotFact, SalesRevisionFact,
    };
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, ProcurementCoverageFacts, ProcurementCoverageSummary, PurchaseOrder,
        PurchaseOrderData, PurchaseSourceImpact, PurchaseType, SalesProcurementCoverage, SourceImpactKind,
        SourceImpactReason,
    };
    use erp_read_models::purchase_center::source_impact::PurchaseSourceState;

    use super::{references_current, reject_if_affected};
    use crate::Error;

    fn purchase(id: &str) -> PurchaseOrder {
        PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: format!("PO-{id}"),
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
        .unwrap()
    }

    fn state(affected: &[&str]) -> PurchaseSourceState {
        let zero = Quantity::from_str("0").unwrap();
        PurchaseSourceState {
            facts: ProcurementCoverageFacts::default(),
            coverage: SalesProcurementCoverage {
                revision: SalesRevisionFact {
                    base: FactIdentity { id: "rev-2".into() },
                    customer_snapshot: SalesCustomerSnapshotFact { customer_name: "客户".into() },
                    contract_snapshot: None,
                },
                lines: Vec::new(),
                summary: ProcurementCoverageSummary::new(zero, zero).unwrap(),
            },
            impacts: affected
                .iter()
                .map(|id| PurchaseSourceImpact {
                    purchase_order_id: (*id).into(),
                    reasons: vec![SourceImpactReason {
                        sales_order_line_id: "sol-1".into(),
                        line_no: 1,
                        item_name: "礼盒".into(),
                        kind: SourceImpactKind::Zeroed {
                            baseline_quantity: Quantity::from_str("2").unwrap(),
                        },
                    }],
                })
                .collect(),
        }
    }

    /// 引用当前销售版本时直接通过，不重算覆盖。
    #[test]
    fn current_revision_skips_impact_check() {
        let order = purchase("po-1");

        assert!(references_current(Some("rev-1"), &order));
        assert!(!references_current(Some("rev-2"), &order));
        assert!(!references_current(None, &order));
    }

    /// 引用旧版本时只有受影响才拒绝，并返回调用方给出的下一步说明。
    #[test]
    fn old_revision_is_rejected_only_when_affected() {
        let order = purchase("po-1");

        assert!(reject_if_affected(&state(&["po-2"]), &order, "请修改原单").is_ok());
        let error = reject_if_affected(&state(&["po-1"]), &order, "请修改原单").unwrap_err();

        assert!(matches!(error, Error::ConflictError(message) if message == "请修改原单"));
    }
}
