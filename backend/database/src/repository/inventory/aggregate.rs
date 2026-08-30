use entities::catalog::{Sku, SkuRevision};
use entities::document_registry::BusinessDocument;
use entities::fulfillment::PurchaseReceipt;
use entities::warehouse::{Warehouse, WarehouseRevision};

use super::shared::{active_entity_by_id, entities_by_ids};
use super::{
    InventoryRepository, BUSINESS_DOCUMENTS, PURCHASE_RECEIPTS, SKUS, SKU_REVISIONS, WAREHOUSES,
    WAREHOUSE_REVISIONS,
};
use crate::executor::Executor;
use crate::Result;

impl<'a> InventoryRepository<'a> {
    /// 按主键读取已注册业务单据。
    ///
    /// # 参数
    /// * `id` - 业务单据主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配注册行；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn business_document(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BusinessDocument>> {
        active_entity_by_id(self.db, BUSINESS_DOCUMENTS, id, executor).await
    }

    /// 按主键集合批量读取采购入库单。
    ///
    /// # 参数
    /// * `ids` - 入库单主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除入库单。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn purchase_receipts_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchaseReceipt>> {
        entities_by_ids(self.db, PURCHASE_RECEIPTS, ids, executor).await
    }

    /// 按主键集合批量读取仓库。
    ///
    /// # 参数
    /// * `ids` - 仓库主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除仓库。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn warehouses_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Warehouse>> {
        entities_by_ids(self.db, WAREHOUSES, ids, executor).await
    }

    /// 按主键集合批量读取仓库修订。
    ///
    /// # 参数
    /// * `ids` - 仓库修订主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除修订。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn warehouse_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<WarehouseRevision>> {
        entities_by_ids(self.db, WAREHOUSE_REVISIONS, ids, executor).await
    }

    /// 按主键集合批量读取 SKU。
    ///
    /// # 参数
    /// * `ids` - SKU 主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除 SKU。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn skus_by_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<Sku>> {
        entities_by_ids(self.db, SKUS, ids, executor).await
    }

    /// 按主键集合批量读取 SKU 修订。
    ///
    /// # 参数
    /// * `ids` - SKU 修订主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除修订。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    pub async fn sku_revisions_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        entities_by_ids(self.db, SKU_REVISIONS, ids, executor).await
    }
}
