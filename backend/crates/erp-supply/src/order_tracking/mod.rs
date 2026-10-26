//! API 订单的来源关联、只读补查检查点与不可变来源观察。

use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Result as PersistenceResult;
use persistence_core::indexes::{named_index, unique_index};
use repository::OrderTrackingExt;

mod coverage;
mod model;
pub mod repository;
pub use coverage::{DeliveryCoverageLine, complete_delivery_coverage};
pub use model::{OrderObservation, OrderShipment, OrderTracking, TrackingSourceLine, TrackingState};

/// 创建订单跟踪集合的唯一身份与有界调度索引。
/// # 参数
/// `db` 为当前部署业务库。
/// # 返回
/// 所有跟踪索引创建完成。
/// # 错误
/// 数据库拒绝索引或已有数据冲突时返回原错误。
pub async fn ensure_indexes(db: &Database) -> PersistenceResult<()> {
    db.collection::<Document>(Database::SUPPLIER_ORDER_TRACKING)
        .create_indexes([
            unique_index("uk_supplier_order_tracking_id", doc! {"id":1}),
            named_index("ix_supplier_order_tracking_due", doc! {"state":1,"poll_after":1,"id":1}),
            named_index("ix_supplier_order_tracking_purchase", doc! {"purchase_order_id":1,"id":1}),
        ])
        .await?;
    db.collection::<Document>(Database::SUPPLIER_ORDER_OBSERVATIONS)
        .create_indexes([
            unique_index("uk_supplier_order_observations_id", doc! {"id":1}),
            named_index("ix_supplier_order_observations_order", doc! {"order_id":1,"observed_at":-1,"id":1}),
        ])
        .await?;
    Ok(())
}

#[cfg(test)]
mod tests;
