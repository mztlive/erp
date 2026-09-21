//! 组织停用使用的供应履约与结算未结事实。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};

use super::{SupplierFulfillmentExt, SupplierSettlementExt};
use crate::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus, RefundStatus};

/// 检查组织是否仍持有未完成履约、在途动作或未确认结算。
///
/// # 错误
/// 数据库读取失败时拒绝完成停用检查。
pub async fn has_unsettled_business_org(
    db: &Database,
    org: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if db
        .supplier_settlement_statements()
        .exists(
            doc! {
                "business_org_unit_id": org, "status": { "$nin": ["CONFIRMED", "VOIDED"] }
            },
            executor,
        )
        .await?
    {
        return Ok(true);
    }
    let orders =
        db.supplier_fulfillment_orders().find_many(doc! { "business_org_unit_id": org }, executor).await?;
    if orders
        .iter()
        .any(|order| unsettled_order(order.fulfillment_status, order.cancel_status, order.refund_status))
    {
        return Ok(true);
    }
    let ids = orders.into_iter().map(|order| order.base.id).collect::<Vec<_>>();
    db.supplier_order_actions()
        .exists(
            doc! {
                "supplier_fulfillment_order_id": { "$in": ids },
                "status": { "$nin": ["SUCCEEDED", "FAILED"] }
            },
            executor,
        )
        .await
}

fn unsettled_order(main: FulfillmentStatus, cancel: CancelStatus, refund: RefundStatus) -> bool {
    matches!(cancel, CancelStatus::CancelPending | CancelStatus::Manual)
        || matches!(refund, RefundStatus::RefundPending | RefundStatus::Manual)
        || (!matches!(main, FulfillmentStatus::Completed | FulfillmentStatus::Rejected)
            && cancel != CancelStatus::Canceled
            && refund != RefundStatus::Refunded)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unresolved_supplier_work_blocks_org_even_after_main_order_completes() {
        for main in
            [FulfillmentStatus::Received, FulfillmentStatus::ResultUnknown, FulfillmentStatus::Exception]
        {
            assert!(unsettled_order(main, CancelStatus::None, RefundStatus::None));
        }
        for main in [FulfillmentStatus::Completed, FulfillmentStatus::Rejected] {
            assert!(!unsettled_order(main, CancelStatus::None, RefundStatus::None));
            assert!(unsettled_order(main, CancelStatus::CancelPending, RefundStatus::None));
            assert!(unsettled_order(main, CancelStatus::None, RefundStatus::RefundPending));
            assert!(unsettled_order(main, CancelStatus::None, RefundStatus::Manual));
        }
        assert!(!unsettled_order(FulfillmentStatus::Accepted, CancelStatus::Canceled, RefundStatus::None));
        assert!(!unsettled_order(FulfillmentStatus::Accepted, CancelStatus::None, RefundStatus::Refunded));
        // 部分退款可为已完成订单的合法终态；后续在途退款另查动作事实。
        assert!(!unsettled_order(FulfillmentStatus::Completed, CancelStatus::None, RefundStatus::Partial));
    }
}
