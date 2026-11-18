use chrono::Local;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{SkuId, StockMovementId, WarehouseId};
use erp_core::money::Quantity;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::repository::find_active_by_field_in;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Repository, Result, aggregate, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::shared::{
    authorized_projection_search, both_dec, both_inc, cross_inc, ids_to_strings, scoped_base_filter,
    sort_doc, to_decimal128, with_id_tie_breaker,
};
use super::{InventoryRepository, STOCK_BALANCES};
use crate::entity::inventory::StockBalance;
use crate::ports::InventoryQuery;
use crate::repository::InventoryExt;
use crate::{Error as InventoryError, Result as InventoryResult};

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
    /// Service 解析的搜索条件；只允许服务端构造。
    pub search: super::InventorySearch,
    /// 业务仓库筛选；`None` 不收窄候选，授权由独立凭证限定。
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

impl Default for StockBalanceFilter {
    // 初始化库存余额筛选：第一页、每页 20 条，业务筛选默认留空，默认降序排序。
    fn default() -> Self {
        Self {
            search: super::InventorySearch::default(),
            warehouse_ids: None,
            sku_id: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for StockBalanceFilter {
    // 转换为 MongoDB 查询条件（余额不设业务软删除，恒为未删除）。
    // 基底来自 `scoped_base_filter`（未删除，以及 `warehouse_ids`：`None` 不限仓库，空集合保持空 `$in`）。有值时精确匹配 `sku_id`，
    // 最后并入 `search`。
    fn to_doc(&self) -> Document {
        let mut filter = scoped_base_filter(self.warehouse_ids.as_deref());
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        self.search.apply(&mut filter);
        filter
    }
}

impl Pagination for StockBalanceFilter {
    // 向仓储分页传递页码与单页条数。
    // 原样返回 `page`，并把 `page_size` 从 `u32` 拓宽为 `u64`；不在此处把小于 1 的页码归一。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 库存余额集合上的领域查询与原子条件写。
#[allow(async_fn_in_trait)]
pub trait StockBalanceRepositoryExt {
    /// 分页检索库存余额列表（投影查询）。
    /// 只返回 [`StockBalanceRow`] 所需的列表字段，不加载整文档；排序字段走白名单映射（`sku_id`/`created_at`）。
    /// 返回当前页投影行与满足筛选条件的总数。
    /// 完整授权条件下的页与总数；缺资格返回空页。
    /// `executor` 为数据访问执行器，由 Service 决定是否位于事务中 filter 为业务筛选，access 为当前动作凭证，executor 为原读取执行器。
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。按统一库存模型凭证分页查询；候选条件只收窄结果。模型错配、复验或数据库失败返回错误。
    async fn search_authorized_stock_balances(
        &self,
        filter: &StockBalanceFilter,
        access: Option<&InventoryQuery>,
        executor: &mut dyn Executor,
    ) -> InventoryResult<PageResult<StockBalanceRow>>;

    /// 读写仓库库存前，以全局唯一的仓库/SKU 组合读取余额。找不到返回 `None`；传播 MongoDB 查询错误。
    async fn find_by_dimensions(
        &self,
        warehouse_id: &WarehouseId,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<StockBalance>>;

    /// 入库时对指定余额原子增加正数现存量和可用量，维持 `available = on_hand - reserved`；不先读旧量。
    /// 余额不存在返回 `false`，传播 MongoDB 写入错误。需要更新后的内存实体时重新读取。
    async fn increase_on_hand(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 登记正数预占时，用单文档条件写检查 `available_quantity >= q`，再增加预占并等量减少可用量。
    /// 余额不存在或数量不足时返回 `false`，不修改文档；传播写入错误，不同步调用方的内存实体。
    async fn reserve_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 出库或盘亏时用单文档条件写检查 `available_quantity >= q`，再等量扣减正数现存量和可用量。
    /// 余额不存在或数量不足返回 `false`，不修改文档；传播写入错误，不同步调用方的内存实体。
    async fn deduct_available(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 释放正数预占时原子检查 `reserved_quantity >= q`，再从预占转回可用量。
    /// 余额不存在或预占不足返回 `false`，不修改文档；传播写入错误，不同步调用方的内存实体。
    async fn release_reserved(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 余额增减成功后沿同一执行器登记本次正式流水 ID，供台账展示最后变动。
    /// 余额不存在或已删除时返回 `false`；传播 MongoDB 写入错误。
    async fn apply_last_movement(
        &self,
        id: &str,
        movement_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool>;
}

impl StockBalanceRepositoryExt for Repository<'_, StockBalance> {
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
    async fn search_authorized_stock_balances(
        &self,
        filter: &StockBalanceFilter,
        access: Option<&InventoryQuery>,
        executor: &mut dyn Executor,
    ) -> InventoryResult<PageResult<StockBalanceRow>> {
        let options = FindOptions::builder()
            .sort(stock_balance_sort(filter))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(stock_balance_projection())
            .build();
        authorized_projection_search(&self.collection(), filter.to_doc(), options, access, executor).await
    }

    async fn find_by_dimensions(
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
    async fn increase_on_hand(
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
    async fn reserve_quantity(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity_bson = to_decimal128(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "available_quantity": { "$gte": quantity_bson },
        };
        let update = cross_inc(quantity_bson, "reserved_quantity", "available_quantity");
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

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
    async fn deduct_available(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity_bson = to_decimal128(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "available_quantity": { "$gte": quantity_bson },
        };
        let update = both_dec(quantity_bson, "on_hand_quantity", "available_quantity");
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

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
    async fn release_reserved(
        &self,
        id: &str,
        quantity: Quantity,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let quantity_bson = to_decimal128(quantity)?;
        let filter = doc! {
            "id": id,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            "reserved_quantity": { "$gte": quantity_bson },
        };
        let update = cross_inc(quantity_bson, "available_quantity", "reserved_quantity");
        let result = mongo_ops::update_one(&self.collection(), filter, update, false, executor).await?;
        Ok(result.matched_count > 0)
    }

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
    async fn apply_last_movement(
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

// 为余额分页追加唯一主键 tie-breaker，避免相同主排序值跨页重复或遗漏。
fn stock_balance_sort(filter: &StockBalanceFilter) -> Document {
    with_id_tie_breaker(
        sort_doc(filter.sort_by.as_deref(), filter.sort_ascending, &["sku_id", "created_at"]),
        filter.sort_ascending,
    )
}

impl<'a> InventoryRepository<'a> {
    /// 在调用方执行器上批量读取未删除的库存余额。
    /// 返回匹配的未删除余额；结果顺序不代表调用方校验顺序。
    /// `ids` 为余额主键集合；空集合不执行查询；`executor` 为调用方事务或非事务执行器。
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn stock_balances_by_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<StockBalance>> {
        find_active_by_field_in(&self.db.collection(STOCK_BALANCES), "id", ids, executor).await
    }

    /// 按主键读取未删除的库存余额。
    /// 返回匹配余额；不存在时返回 `None`。
    /// `id` 为库存余额主键；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询错误。
    pub async fn stock_balance(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<StockBalance>> {
        self.db.stock_balances().find_by_id(id, executor).await
    }

    // 按详情凭证读取完整余额，缺资格不访问业务集合。
    // 返回授权范围内的余额；不存在或无资格时返回 None。
    // id 为真实余额身份，access 为当前详情凭证，executor 为原快照执行器。
    // 模型错配、凭证失效及数据库读取失败时返回错误。
    pub(crate) async fn authorized_stock_balance(
        &self,
        id: &str,
        access: Option<&InventoryQuery>,
        executor: &mut dyn Executor,
    ) -> InventoryResult<Option<StockBalance>> {
        let Some(access) = access else { return Ok(None) };
        if access.collection() != STOCK_BALANCES {
            return Err(InventoryError::ValidationError("库存授权模型与查询集合不匹配".into()));
        }
        access.ensure_current(executor)?;

        let mut stages = access.stages().to_vec();
        stages.push(doc! { "$match": { "id": id } });
        stages.push(doc! { "$limit": 1 });
        let collection = self.db.collection::<StockBalance>(STOCK_BALANCES);
        let mut rows = aggregate::rows(&collection, stages, executor).await?;

        access.ensure_current(executor)?;
        Ok(rows.pop())
    }

    /// 按 SKU 集合批量读取存在可用量的库存余额。
    /// 返回可用数量大于零的全部余额，按仓库与 SKU 稳定排序。
    /// `sku_ids` 为 SKU 主键集合；沿用调用方执行器 `executor`。
    /// 传播 MongoDB 查询或游标读取错误。
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
            FindOptions::builder().sort(doc! { "warehouse_id": 1, "sku_id": 1, "id": 1 }).build(),
            executor,
        )
        .await
    }
}

// 库存余额列表投影字段。
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
    use erp_core::ids::WarehouseId;
    use mongodb::bson::{Bson, doc};
    use persistence_core::QueryFilter;

    use super::{StockBalanceFilter, stock_balance_sort};

    fn filter(warehouse_ids: Option<Vec<WarehouseId>>) -> StockBalanceFilter {
        StockBalanceFilter { warehouse_ids, ..Default::default() }
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
            filter(Some(Vec::new())).to_doc().get_document("warehouse_id").unwrap().get_array("$in").unwrap(),
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
