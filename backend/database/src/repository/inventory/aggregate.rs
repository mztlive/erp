use entities::catalog::{Sku, SkuRevision};
use entities::document_registry::BusinessDocument;
use entities::fulfillment::PurchaseReceipt;
use entities::ids::SkuId;
use entities::warehouse::{Warehouse, WarehouseRevision};
use mongodb::bson::doc;

use super::super::extensions::{CatalogExt, DocumentRegistryExt, FulfillmentExt, WarehouseExt};
use super::InventoryRepository;
use crate::executor::Executor;
use crate::Result;

impl<'a> InventoryRepository<'a> {
    /// 按主键读取已注册业务单据（库存水合事实包入口，不拥有该集合）。
    ///
    /// 委托 `document_registry` 域拥有者 `business_documents` 访问器查询，
    /// 本类型不直接触碰 `business_documents` 集合。
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
        self.db.business_documents().find_by_id(id, executor).await
    }

    /// 按主键集合批量读取采购入库单（库存水合事实包入口，不拥有该集合）。
    ///
    /// 委托 `fulfillment` 域拥有者 `purchase_receipts` 访问器查询，
    /// 本类型不直接触碰 `purchase_receipts` 集合。
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
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .purchase_receipts()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await
    }

    /// 按主键集合批量读取仓库（库存水合事实包入口，不拥有该集合）。
    ///
    /// 委托 `warehouse` 域拥有者 `warehouses` 访问器查询，
    /// 本类型不直接触碰 `warehouses` 集合。
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
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .warehouses()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await
    }

    /// 按主键集合批量读取仓库修订（库存水合事实包入口，不拥有该集合）。
    ///
    /// 委托 `warehouse` 域拥有者 `warehouse_revisions` 访问器查询，
    /// 本类型不直接触碰 `warehouse_revisions` 集合。
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
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .warehouse_revisions()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await
    }

    /// 按主键集合批量读取 SKU（库存水合事实包入口，不拥有该集合）。
    ///
    /// 委托 `catalog` 域拥有者 `skus` 访问器的既有 `find_by_ids` 查询，
    /// 本类型不直接触碰 `skus` 集合。
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
        let sku_ids = ids.iter().map(|id| SkuId::new(id.clone())).collect::<Vec<_>>();
        self.db.skus().find_by_ids(&sku_ids, executor).await
    }

    /// 按主键集合批量读取 SKU 修订（库存水合事实包入口，不拥有该集合）。
    ///
    /// 委托 `catalog` 域拥有者 `sku_revisions` 访问器查询，
    /// 本类型不直接触碰 `sku_revisions` 集合。
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
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .sku_revisions()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await
    }
}
