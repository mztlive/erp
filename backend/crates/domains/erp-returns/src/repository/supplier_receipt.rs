//! 供应商实收通知消费的原采购退货阻断事实，不能用原入库数量忽略已实际退货。

use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use crate::Result;
use crate::repository::ReturnsExt;

/// 判断原采购是否已有实际退货或退货完成事实。
/// # 参数
/// 服务端原采购身份及当前收货核验事务。
/// # 返回
/// 有任何实际退货时返回真；首期不猜测其在供应商叶子之间的数量归属。
/// # 错误
/// 读取失败时拒绝新增收货通知。
pub async fn has_actual_return(db: &Database, purchase: &str, executor: &mut dyn Executor) -> Result<bool> {
    Ok(db
        .purchase_return_orders()
        .exists(
            doc! {
                "purchase_order_id":purchase,"status":{"$in":["returned","completed"]},"deleted_at":0_i64,
            },
            executor,
        )
        .await?)
}
