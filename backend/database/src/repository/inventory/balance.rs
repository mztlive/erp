use chrono::Local;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::ids::{SkuId, StockMovementId, WarehouseId};
use entities::inventory::StockBalance;
use entities::money::Quantity;

use super::shared::{active_entity_by_id, both_dec, both_inc, cross_inc, ids_to_strings, sort_doc, to_bson};
use super::{InventoryRepository, STOCK_BALANCES};
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter, Repository};
use crate::{mongo_ops, Result};

/// 库存余额列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockBalanceRow {
    /// 实体主键。
    pub id: String,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 账面现存。
    pub on_hand_quantity: Quantity,
    /// 有效预占。
    pub reserved_quantity: Quantity,
    /// 可用数量。
    pub available_quantity: Quantity,
    /// 已应用最后流水。
    pub last_movement_id: Option<StockMovementId>,
    /// 乐观锁版本。
    pub version: u64,
}

/// 库存余额列表筛选条件。
#[derive(Debug, Clone)]
pub struct StockBalanceFilter {
    /// Service 已证明可读取的仓库集合；`None` 表示公司级，空集合表示无范围。
    pub warehouse_ids: Option<Vec<WarehouseId>>,
    /// SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内；`None` 默认 `sku_id`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for StockBalanceFilter {
    /// 转换为 MongoDB 查询条件（余额不设业务软删除，恒为未删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(warehouse_ids) = &self.warehouse_ids {
            filter.insert(
                "warehouse_id",
                doc! { "$in": warehouse_ids.iter().map(ToString::to_string).collect::<Vec<_>>() },
            );
        }
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        filter
    }
}

impl Pagination for StockBalanceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, StockBalance> {
    /// 分页检索库存余额列表（投影查询）。
    ///
    /// 只返回 [`StockBalanceRow`] 所需的列表字段，不加载整文档；排序字段走
    /// 白名单映射（`sku_id`/`created_at`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.search_stock_balances",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_balances",
            db.operation.name = "search"
        )
    )]
    pub async fn search_stock_balances(
        &self,
        filter: &StockBalanceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<StockBalanceRow>> {
        let options = FindOptions::builder()
            .sort(stock_balance_sort(filter))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_balance_projection())
            .build();
        let collection = self.collection().clone_with_type::<StockBalanceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按库存维度查找余额（`(warehouse_id, sku_id)` 全局唯一）。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库
    /// * `sku_id` - SKU
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的余额；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_dimensions(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<StockBalance>> {
        self.find_one(
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "sku_id": sku_id.to_string(),
            },
            executor,
        )
        .await
    }

    /// ★原子条件写★：入库增加账面现存（`on_hand += q`、`available += q`）。
    ///
    /// 以 `id` 定位的单文档原子 `$inc`，不读取旧值；写入条件只要求余额行存在。
    /// 维护恒等式 `available = on_hand - reserved`（§6.7）。本方法不更新内存
    /// 实体，调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数入库数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 命中并完成增加返回 `true`；余额行不存在时返回 `false`（文档不变）。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.increase_on_hand",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_balances",
            db.operation.name = "update"
        )
    )]
    pub async fn increase_on_hand(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let filter = doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        let update = both_inc(quantity, "on_hand_quantity", "available_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

    /// ★原子条件写★：预占可用量（`reserved += q`、`available -= q`）。
    ///
    /// 写条件内联 `available_quantity >= q`（§6.7 可用量守恒），可用量不足时
    /// **直接拒绝**：返回 `Ok(false)` 且文档不变，不是「先读后写」的判断。
    /// 本方法不更新内存实体，调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数预占数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 可用量充足且命中时返回 `true`；可用量不足或余额行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.reserve_quantity",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_balances",
            db.operation.name = "update"
        )
    )]
    pub async fn reserve_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "available_quantity": { "$gte": &quantity },
        };
        let update = cross_inc(quantity, "reserved_quantity", "available_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

    /// ★原子条件写★：扣减可用量（`on_hand -= q`、`available -= q`）。
    ///
    /// 写条件内联 `available_quantity >= q`（§6.7 不产生负库存），可用量不足
    /// 时**直接拒绝**：返回 `Ok(false)` 且文档不变，不是「先读后写」的判断。
    /// 本方法不更新内存实体，调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数出库数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 可用量充足且命中时返回 `true`；可用量不足或余额行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.deduct_available",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_balances",
            db.operation.name = "update"
        )
    )]
    pub async fn deduct_available(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "available_quantity": { "$gte": &quantity },
        };
        let update = both_dec(quantity, "on_hand_quantity", "available_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

    /// ★原子条件写★：释放预占（`reserved -= q`、`available += q`）。
    ///
    /// 写条件内联 `reserved_quantity >= q`（不允许释放超过已预占），不足时
    /// **直接拒绝**：返回 `Ok(false)` 且文档不变。本方法不更新内存实体，
    /// 调用方需要最新余额时重新读取。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `quantity` - 正数释放数量
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 预占充足且命中时返回 `true`；预占不足或余额行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.release_reserved",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_balances",
            db.operation.name = "update"
        )
    )]
    pub async fn release_reserved(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity = to_bson(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "reserved_quantity": { "$gte": &quantity },
        };
        let update = cross_inc(quantity, "available_quantity", "reserved_quantity")?;
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

    /// 登记余额「已应用最后流水」（台账最后变动列）。
    ///
    /// 与数量增减同事务调用；行不存在或已软删除时返回 `false`。
    ///
    /// # 参数
    /// * `id` - 余额主键
    /// * `movement_id` - 刚应用的正式流水主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 命中并更新返回 `true`；余额行不存在时返回 `false`。
    ///
    /// # 错误
    /// 当 MongoDB 写入失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.apply_last_movement",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_balances",
            db.operation.name = "update"
        )
    )]
    pub async fn apply_last_movement(
        &self,
        id: &str,
        movement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let result = mongo_ops::update_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            doc! {
                "$set": {
                    "last_movement_id": mongodb::bson::serialize_to_bson(movement_id)?,
                    "updated_at": Local::now().timestamp(),
                },
                "$inc": { "version": 1 },
            },
            false,
            executor,
        )
        .await?;
        Ok(result.matched_count > 0)
    }
}

/// 为余额分页追加唯一主键 tie-breaker，避免相同主排序值跨页重复或遗漏。
fn stock_balance_sort(filter: &StockBalanceFilter) -> Document {
    let mut sort = sort_doc(
        filter.sort_by.as_deref(),
        filter.sort_ascending,
        &["sku_id", "created_at"],
    );
    sort.insert("id", if filter.sort_ascending { 1 } else { -1 });
    sort
}

impl<'a> InventoryRepository<'a> {
    /// 按主键读取未删除的库存余额。
    ///
    /// # 参数
    /// * `id` - 库存余额主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配余额；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn stock_balance(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<StockBalance>> {
        active_entity_by_id(self.db, STOCK_BALANCES, id, executor).await
    }

    /// 按库存维度读取唯一余额。
    ///
    /// # 参数
    /// * `warehouse_id` - 仓库主键
    /// * `sku_id` - SKU 主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配余额；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn balance_for_dimensions(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<StockBalance>> {
        mongo_ops::find_one(
            &self.db.collection::<StockBalance>(STOCK_BALANCES),
            doc! {
                "warehouse_id": warehouse_id.to_string(),
                "sku_id": sku_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 按 SKU 集合批量读取存在可用量的库存余额。
    ///
    /// # 参数
    /// * `sku_ids` - SKU 主键集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回可用数量大于零的全部余额，按仓库与 SKU 稳定排序。
    ///
    /// # 错误
    /// MongoDB 查询或游标读取失败时返回错误。
    #[tracing::instrument(
        name = "repository.inventory.available_balances_for_skus",
        skip_all,
        fields(
            layer = "repository",
            domain = "inventory",
            db.system.name = "mongodb",
            db.collection.name = "stock_balances",
            db.operation.name = "find"
        )
    )]
    pub async fn available_balances_for_skus(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockBalance>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        mongo_ops::find_many(
            &self.db.collection::<StockBalance>(STOCK_BALANCES),
            doc! {
                "sku_id": { "$in": ids_to_strings(sku_ids) },
                "available_quantity": { "$gt": 0 },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .sort(doc! { "warehouse_id": 1, "sku_id": 1, "id": 1 })
                .build(),
            executor,
        )
        .await
    }
}

/// 库存余额列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn stock_balance_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "sku_id": 1,
        "on_hand_quantity": 1,
        "reserved_quantity": 1,
        "available_quantity": 1,
        "last_movement_id": 1,
        "version": 1,
    }
}

#[cfg(test)]
mod filter_tests {
    use super::{stock_balance_sort, StockBalanceFilter};
    use crate::repository::QueryFilter;
    use entities::ids::WarehouseId;
    use mongodb::bson::{doc, Bson};

    fn filter(warehouse_ids: Option<Vec<WarehouseId>>) -> StockBalanceFilter {
        StockBalanceFilter {
            warehouse_ids,
            sku_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }

    #[test]
    fn authorized_warehouse_filter_is_applied_before_page_and_total_queries() {
        assert_eq!(
            filter(Some(vec![WarehouseId::new("warehouse-1")]))
                .to_doc()
                .get_document("warehouse_id")
                .unwrap(),
            &doc! { "$in": ["warehouse-1"] }
        );
        assert_eq!(
            filter(Some(Vec::new()))
                .to_doc()
                .get_document("warehouse_id")
                .unwrap()
                .get_array("$in")
                .unwrap(),
            &Vec::<Bson>::new()
        );
        assert!(!filter(None).to_doc().contains_key("warehouse_id"));
    }

    #[test]
    fn balance_sort_uses_unique_id_as_same_direction_tie_breaker() {
        let mut value = filter(None);
        value.sort_by = Some("sku_id".to_string());
        value.sort_ascending = true;
        assert_eq!(stock_balance_sort(&value), doc! { "sku_id": 1, "id": 1 });
        value.sort_ascending = false;
        assert_eq!(stock_balance_sort(&value), doc! { "sku_id": -1, "id": -1 });
    }
}
