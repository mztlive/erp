//! PurchaseReturnOrder 的本域构造与头、首行持久化。
use erp_core::ids::{PurchaseReturnLineId, PurchaseReturnOrderId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::dto::CreatePurchaseReturnOrderRequest;
use crate::entity::returns::{
    PurchaseReturnLine, PurchaseReturnLineData, PurchaseReturnOrder, PurchaseReturnOrderData,
};
use crate::repository::ReturnsExt;

/// 由已校验请求构造采购退货单及首条明细，保留原 ID 分配顺序。
///
/// # Errors
/// 原单号或首行的实体不变量不满足时返回原错误。
pub fn build_purchase_return_order_and_line(
    req: CreatePurchaseReturnOrderRequest,
    created_by: &str,
) -> Result<(PurchaseReturnOrderId, PurchaseReturnOrder, PurchaseReturnLine)> {
    let order_id = PurchaseReturnOrderId::new(next_id());
    let order = PurchaseReturnOrder::new(
        order_id.clone(),
        PurchaseReturnOrderData {
            purchase_return_no: req.purchase_return_no,
            purchase_order_id: req.purchase_order_id,
            sales_return_case_id: req.sales_return_case_id,
            return_mode: req.return_mode,
        },
        created_by,
    )?;
    let line = PurchaseReturnLine::new(
        PurchaseReturnLineId::new(next_id()),
        PurchaseReturnLineData {
            purchase_return_order_id: order_id.clone(),
            purchase_order_revision_line_id: req.lines[0].purchase_order_revision_line_id.clone(),
            return_quantity: req.lines[0].return_quantity,
            warehouse_id: req.lines[0].warehouse_id.clone(),
        },
    )?;
    Ok((order_id, order, line))
}

/// 在调用方 Executor 上依次写入退货头与首条明细。
///
/// # Errors
/// 头或明细写入失败时返回原仓储错误；事务由外层根持有。
pub async fn persist_purchase_return_order_with_line(
    db: &Database,
    order: &PurchaseReturnOrder,
    line: &PurchaseReturnLine,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.returns().create_purchase_return_with_line(order, line, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{PurchaseOrderId, PurchaseOrderRevisionLineId, WarehouseId};
    use erp_core::money::Quantity;
    use validator::Validate;

    use super::build_purchase_return_order_and_line;
    use crate::dto::{CreatePurchaseReturnLineRequest, CreatePurchaseReturnOrderRequest};
    use crate::entity::returns::ReturnMode;

    #[test]
    fn creation_keeps_only_the_first_requested_purchase_return_line() {
        let req = CreatePurchaseReturnOrderRequest {
            purchase_return_no: "PR-1".into(),
            purchase_order_id: PurchaseOrderId::new("purchase-order"),
            sales_return_case_id: None,
            return_mode: ReturnMode::CompanyWarehouseToSupplier,
            lines: vec![
                CreatePurchaseReturnLineRequest {
                    purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("first-line"),
                    return_quantity: Quantity::from_str("2").unwrap(),
                    warehouse_id: Some(WarehouseId::new("warehouse")),
                },
                CreatePurchaseReturnLineRequest {
                    purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("ignored-second-line"),
                    return_quantity: Quantity::from_str("0").unwrap(),
                    warehouse_id: None,
                },
            ],
        };
        req.validate().unwrap();
        let (id, order, line) = build_purchase_return_order_and_line(req, "actor").unwrap();
        assert_eq!(id.as_ref(), order.base.id);
        assert_eq!(line.purchase_return_order_id, id);
        assert_eq!(line.purchase_order_revision_line_id.as_ref(), "first-line");
        assert_eq!(line.return_quantity, Quantity::from_str("2").unwrap());
        assert_eq!(line.warehouse_id.as_ref().map(|id| id.as_ref()), Some("warehouse"));
    }
}
