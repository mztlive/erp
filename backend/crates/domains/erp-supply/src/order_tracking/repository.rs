use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use super::{OrderObservation, OrderTracking};

/// 供应商订单查询与应用证据的领域集合入口。
pub trait OrderTrackingExt {
    const SUPPLIER_ORDER_TRACKING: &'static str = "supplier_order_tracking";
    const SUPPLIER_ORDER_OBSERVATIONS: &'static str = "supplier_order_observations";
    /// 取得调度及应用检查点仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 跟踪记录仓储。
    /// # 错误
    /// 无。
    fn supplier_order_tracking(&self) -> Repository<'_, OrderTracking>;
    /// 取得不可变查询证据仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 来源观察仓储。
    /// # 错误
    /// 无。
    fn supplier_order_observations(&self) -> Repository<'_, OrderObservation>;
}

impl OrderTrackingExt for Database {
    fn supplier_order_tracking(&self) -> Repository<'_, OrderTracking> {
        Repository::new(self, Self::SUPPLIER_ORDER_TRACKING)
    }
    fn supplier_order_observations(&self) -> Repository<'_, OrderObservation> {
        Repository::new(self, Self::SUPPLIER_ORDER_OBSERVATIONS)
    }
}

/// 有界读取当前需要补查或应用的跟踪记录。
/// # 参数
/// `db` 为业务库；`now` 为调度时刻；执行器沿用调用方。
/// # 返回
/// 最多 20 条到期且未完成的记录。
/// # 错误
/// 查询失败时返回原仓储错误。
pub async fn due(db: &Database, now: Instant, executor: &mut dyn Executor) -> Result<Vec<OrderTracking>> {
    let seconds = now.unix_secs();
    mongo_ops::find_many(
        &db.supplier_order_tracking().collection(),
        doc! {
            "state":{"$in":["POLLING","MANUAL"]},"poll_after":{"$lte":seconds},
            "deleted_at":NOT_DELETED_TIMESTAMP_BSON,
            "$or":[{"lease_until":null},{"lease_until":{"$lte":seconds}}]
        },
        FindOptions::builder().sort(doc! {"poll_after":1,"id":1}).limit(20).build(),
        executor,
    )
    .await
}
