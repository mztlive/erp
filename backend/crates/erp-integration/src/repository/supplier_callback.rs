use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::Instant;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, mongo_ops};

use crate::entity::supplier_callback::SupplierCallbackReceipt;

pub trait SupplierCallbackExt {
    const SUPPLIER_CALLBACK_RECEIPTS: &'static str = "supplier_callback_receipts";
    /// 取得供应商原始接收证据仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本域接收仓储。
    /// # 错误
    /// 无。
    fn supplier_callback_receipts(&self) -> Repository<'_, SupplierCallbackReceipt>;
}
impl SupplierCallbackExt for Database {
    fn supplier_callback_receipts(&self) -> Repository<'_, SupplierCallbackReceipt> {
        Repository::new(self, Self::SUPPLIER_CALLBACK_RECEIPTS)
    }
}

#[allow(async_fn_in_trait)]
pub trait SupplierCallbackRepositoryExt {
    /// 按绑定连接读取有界待核验队列；不包含其他连接。
    /// # 参数
    /// `connection_id` 为授权连接；`executor` 为调用方执行器。
    /// # 返回
    /// 最早的最多 50 条 received 记录。
    /// # 错误
    /// 仓储查询失败时保留原分类。
    async fn pending(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCallbackReceipt>>;
}
impl SupplierCallbackRepositoryExt for Repository<'_, SupplierCallbackReceipt> {
    async fn pending(
        &self,
        connection_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCallbackReceipt>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {"connection_id":connection_id,"status":"received","deleted_at":NOT_DELETED_TIMESTAMP_BSON},
            FindOptions::builder().sort(doc! {"received_at":1,"id":1}).limit(50).build(),
            executor,
        )
        .await
    }
}

/// 读取后台订单消费者的有界队列；原始正文保持密文。
/// # 参数
/// `db` 为当前部署库，执行器由调用方提供。
/// # 返回
/// 最早的至多 20 条订单回调接收记录；商品状态由独立队列处理。
/// # 错误
/// 数据库读取失败时返回原错误。
pub async fn pending_order_callbacks(
    db: &Database,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCallbackReceipt>> {
    mongo_ops::find_many(
        &db.supplier_callback_receipts().collection(),
        doc! {"status":"received","route_kind":"order","deleted_at":NOT_DELETED_TIMESTAMP_BSON,
        "$or":[{"next_attempt_at":null},{"next_attempt_at":{"$lte":Instant::now().unix_secs()}}]},
        FindOptions::builder().sort(doc! {"next_attempt_at":1,"received_at":1,"id":1}).limit(20).build(),
        executor,
    )
    .await
}

/// 读取到期商品回调；与订单队列共享既有按路由和延期时间的索引。
/// # 参数
/// 当前数据库和调用方执行器。
/// # 返回
/// 最早的至多五条商品回调；实际领取还须校验记录版本。
/// # 错误
/// 数据库查询失败时返回原错误。
pub async fn pending_catalog_callbacks(
    db: &Database,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCallbackReceipt>> {
    mongo_ops::find_many(
        &db.supplier_callback_receipts().collection(),
        doc! {"status":"received","route_kind":{"$in":["catalog","status","product","cities","price"]},
        "deleted_at":NOT_DELETED_TIMESTAMP_BSON,
        "$or":[{"next_attempt_at":null},{"next_attempt_at":{"$lte":Instant::now().unix_secs()}}]},
        FindOptions::builder().sort(doc! {"next_attempt_at":1,"received_at":1,"id":1}).limit(5).build(),
        executor,
    )
    .await
}
