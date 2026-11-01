//! 已证明直退凭据的本域读取，旧草稿不参与净履约。
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use crate::Result;
use crate::entity::returns::PurchaseReturnOrder;
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
/// 草稿和撤回记录不占用方案金额。
/// # 错误
/// 仓储读取失败时返回错误。
pub async fn purchase_settlements(
    db: &Database,
    purchase_id: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<crate::entity::returns::SupplierRefund>> {
    db.supplier_refunds()
        .find_many(
            doc! {"source_purchase_order_id": purchase_id, "status": {"$in": ["in_approval", "posted"]}},
            executor,
        )
        .await
        .map_err(Into::into)
}
