use entity_core::NOT_DELETED_TIMESTAMP_BSON;
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
