//! 已证明直退凭据的本域读取，旧草稿不参与净履约。
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;

use crate::Result;
use crate::entity::returns::{PurchaseReturnOrder, SupplierRefund, SupplierRefundStatus};
use crate::repository::ReturnsExt;

/// 读取单张采购的已核实直退记录。
/// # 参数
/// * `db` - 数据库。
/// * `purchase_id` - 已授权采购身份。
/// * `executor` - 调用方执行器。
/// # 返回
/// 返回已退货或已完成且具有凭据的记录。
/// # 错误
/// 仓储失败时返回错误。
pub async fn confirmed_direct_returns(
    db: &Database,
    purchase_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseReturnOrder>> {
    Ok(db
        .purchase_return_orders()
        .find_many(
            doc! {
                "purchase_order_id": purchase_id,
                "status": { "$in": ["returned", "completed"] },
                "direct_return": { "$ne": null }
            },
            executor,
        )
        .await?)
}

/// 按全局唯一退货号读取原提交，供同载荷重放核对。
/// # 参数
/// * `db` - 数据库。
/// * `number` - 原命令派生的退货号。
/// * `executor` - 同一事务执行器。
/// # 返回
/// 原退货记录或空。
/// # 错误
/// 仓储失败时返回错误。
pub async fn direct_return_command(
    db: &Database,
    number: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseReturnOrder>> {
    Ok(db.purchase_return_orders().find_one_by_field("purchase_return_no", number, executor).await?)
}

/// 查询采购处置方案已提交或已入账的款项处理记录。
/// # 参数
/// * `db` - 数据库。
/// * `purchase_id` - 已授权采购。
/// * `executor` - 原事务。
/// # 返回
/// 仅审批中和已过账记录占用方案金额；统一仓储自动排除软删除记录。
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn purchase_settlements(
    db: &Database,
    purchase_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierRefund>> {
    db.supplier_refunds()
        .find_many(purchase_settlement_filter(purchase_id), executor)
        .await
        .map_err(Into::into)
}

/// 采购方案额度只由审批中和已过账的真实持久化状态占用。
fn purchase_settlement_filter(purchase_id: &str) -> Document {
    let statuses = SupplierRefundStatus::SETTLEMENT_OCCUPYING.map(|status| status.as_str());
    doc! { "source_purchase_order_id": purchase_id, "status": { "$in": statuses } }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::serialize_to_bson;

    use super::*;

    #[test]
    fn settlement_filter_matches_persisted_occupying_states_only() {
        let filter = purchase_settlement_filter("purchase-1");
        assert_eq!(filter.get_str("source_purchase_order_id").unwrap(), "purchase-1");
        let statuses = filter.get_document("status").unwrap().get_array("$in").unwrap();
        for status in [SupplierRefundStatus::InApproval, SupplierRefundStatus::Posted] {
            assert!(statuses.contains(&serialize_to_bson(&status).unwrap()));
        }
        for status in
            [SupplierRefundStatus::Draft, SupplierRefundStatus::Rejected, SupplierRefundStatus::Reversed]
        {
            assert!(!statuses.contains(&serialize_to_bson(&status).unwrap()));
        }
        assert_eq!(statuses.len(), 2);
    }
}
