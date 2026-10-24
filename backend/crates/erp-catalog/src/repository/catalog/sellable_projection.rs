//! 公司商品池预算投影：每个 SKU 一条，保存当天的可售行（不可售为空）。
//!
//! 列表按投影直接筛选、排序与分页，不再逐 SKU 关联修订与供给；投影由后台按源数据
//! 变化与日期切换重算，重算时复用实时资格聚合，资格口径只有一处。

use std::collections::BTreeSet;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::change_scan::{ChangeWatermarks, ChangeWindow};
use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use mongodb::bson::{Bson, Document, doc};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use persistence_core::dirty_marks::{self, DirtyMark};
use persistence_core::{Executor, PageResult, Result, mongo_ops, persisted_document};
use serde::{Deserialize, Serialize};

use super::sellable::{SellableSkuFilter, SellableSkuRow, SellableSort};
use crate::entity::catalog::SellablePriceKind;
use crate::repository::CatalogExt;

/// 投影口径版本；口径变化时递增，后台据此全量重建。
pub const SELLABLE_SKU_PROJECTION_VERSION: i32 = 1;

const PROJECTIONS: &str = <Database as CatalogExt>::SELLABLE_SKU_PROJECTIONS;
const STATE: &str = <Database as CatalogExt>::SELLABLE_SKU_PROJECTION_STATE;
const SKUS: &str = <Database as CatalogExt>::SKUS;
const SKU_REVISIONS: &str = <Database as CatalogExt>::SKU_REVISIONS;
const PRODUCTS: &str = <Database as CatalogExt>::PRODUCTS;
const PRODUCT_REVISIONS: &str = <Database as CatalogExt>::PRODUCT_REVISIONS;

/// 投影刷新进度。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SellableSkuProjectionState {
    /// 固定主键。
    #[serde(rename = "_id")]
    pub id: String,
    /// 已开始全量构建的口径版本。
    pub version: i32,
    /// 变更扫描进度。
    #[serde(flatten)]
    pub watermarks: ChangeWatermarks,
    /// 最近一次全量标记的业务日；资格按日期判定，换日后全部重算。
    pub refreshed_on: Option<BusinessDate>,
    /// 全量构建是否完成；完成前列表仍走实时聚合。
    pub ready: bool,
}

impl SellableSkuProjectionState {
    /// 刷新进度的固定主键。
    pub const ID: &'static str = "refresh";

    /// 判断是否需要全量重建：首次运行或口径版本低于当前版本。
    ///
    /// # 参数
    /// * `state` - 已保存的进度
    ///
    /// # 返回
    /// 需要重建时为真；较新版本写入的进度不触发重建。
    ///
    /// # 错误
    /// 无。
    pub fn needs_rebuild(state: Option<&Self>) -> bool {
        state.is_none_or(|state| state.version < SELLABLE_SKU_PROJECTION_VERSION)
    }

    /// 全量重建开始时的进度：尚未就绪，记为当天已全量标记。
    ///
    /// # 参数
    /// * `now` - 当前时间（Unix 秒）
    /// * `today` - 当前业务日
    ///
    /// # 返回
    /// 返回当前口径版本的进度。
    ///
    /// # 错误
    /// 无。
    pub fn rebuilt(now: i64, today: BusinessDate) -> Self {
        Self {
            id: Self::ID.to_string(),
            version: SELLABLE_SKU_PROJECTION_VERSION,
            watermarks: ChangeWatermarks::starting_at(now),
            refreshed_on: Some(today),
            ready: false,
        }
    }

    /// 判断是否换日后尚未全量标记。
    ///
    /// # 参数
    /// * `today` - 当前业务日
    ///
    /// # 返回
    /// 最近全量标记早于今天时为真。
    ///
    /// # 错误
    /// 无。
    pub fn needs_daily_refresh(&self, today: BusinessDate) -> bool {
        self.refreshed_on.is_none_or(|date| date < today)
    }

    /// 判断列表能否按投影查询。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 当前口径版本已完成全量构建时为真。
    ///
    /// # 错误
    /// 无。
    pub fn serves_list(&self) -> bool {
        self.ready && self.version == SELLABLE_SKU_PROJECTION_VERSION
    }
}

#[derive(Deserialize)]
struct ProjectionRow {
    row: Option<SellableSkuRow>,
}

#[derive(Deserialize)]
struct StableIdRow {
    id: String,
}

#[derive(Deserialize)]
struct SkuRefRow {
    sku_id: String,
}

#[derive(Deserialize)]
struct ProductRefRow {
    product_id: String,
}

/// 公司商品池预算投影访问器。
pub struct SellableSkuProjectionStore<'a> {
    db: &'a Database,
}

impl<'a> SellableSkuProjectionStore<'a> {
    /// 绑定数据库。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的访问器。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    fn documents(&self) -> Collection<Document> {
        self.db.collection(PROJECTIONS)
    }

    /// 标记 SKU 待重算；尚无投影的 SKU 插入不可售占位。
    ///
    /// # 参数
    /// * `sku_ids` - 公司 SKU，允许重复
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 标记完成时无返回值。
    ///
    /// # 错误
    /// MongoDB 读写失败时返回错误。
    pub async fn mark_skus(&self, sku_ids: &[String], executor: &mut dyn Executor) -> Result<()> {
        let pending = doc! { "row": Bson::Null, "computed_at": 0_i64 };
        dirty_marks::mark_ids(&self.documents(), sku_ids, |_| pending.clone(), executor).await
    }

    /// 标记全部投影待重算，用于换日。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回被标记的记录数。
    ///
    /// # 错误
    /// MongoDB 写入失败时返回错误。
    pub async fn mark_all(&self, executor: &mut dyn Executor) -> Result<u64> {
        dirty_marks::mark_matching(&self.documents(), doc! {}, executor).await
    }

    /// 读取一批待重算的 SKU。
    ///
    /// # 参数
    /// * `limit` - 本批上限
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回 SKU 与读取时的标记序号。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn dirty_batch(&self, limit: i64, executor: &mut dyn Executor) -> Result<Vec<DirtyMark>> {
        dirty_marks::dirty_batch(&self.documents(), limit, executor).await
    }

    /// 统计待重算的 SKU。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回待重算数量。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn dirty_count(&self, executor: &mut dyn Executor) -> Result<u64> {
        dirty_marks::dirty_count(&self.documents(), executor).await
    }

    /// 保存一个 SKU 的计算结果。
    ///
    /// # 参数
    /// * `sku_id` - 公司 SKU
    /// * `dirty_token` - 开始计算前读取的标记序号
    /// * `row` - 当天可售行；不可售为 `None`
    /// * `computed_at` - 计算时间（Unix 秒）
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 已保存时为真；计算期间又被标记时为假。
    ///
    /// # 错误
    /// 序列化或 MongoDB 写入失败时返回错误。
    pub async fn save(
        &self,
        sku_id: &str,
        dirty_token: i64,
        row: Option<&SellableSkuRow>,
        computed_at: i64,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let row = row.map(persisted_document).transpose()?.map_or(Bson::Null, Bson::Document);
        let fields = doc! { "row": row, "computed_at": computed_at };
        dirty_marks::save_clean(&self.documents(), sku_id, dirty_token, fields, executor).await
    }

    /// 读取刷新进度。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 尚未初始化时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn state(&self, executor: &mut dyn Executor) -> Result<Option<SellableSkuProjectionState>> {
        let collection = self.db.collection::<SellableSkuProjectionState>(STATE);
        mongo_ops::find_one(&collection, doc! { "_id": SellableSkuProjectionState::ID }, executor).await
    }

    /// 写入全量重建开始时的进度，覆盖旧版本的就绪标记。
    ///
    /// # 参数
    /// * `state` - 重建进度
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// 序列化或 MongoDB 写入失败时返回错误。
    pub async fn begin_rebuild(
        &self,
        state: &SellableSkuProjectionState,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut fields = mongodb::bson::serialize_to_document(state)?;
        fields.remove("_id");
        let collection = self.db.collection::<Document>(STATE);
        let filter = doc! { "_id": &state.id };
        mongo_ops::update_one(&collection, filter, doc! { "$set": fields }, true, executor).await?;
        Ok(())
    }

    /// 推进扫描进度并记录换日全量标记；只前进不后退。
    ///
    /// # 参数
    /// * `watermarks` - 新扫描进度
    /// * `refreshed_on` - 已全量标记的业务日
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// MongoDB 写入失败时返回错误。
    pub async fn advance(
        &self,
        watermarks: ChangeWatermarks,
        refreshed_on: Option<BusinessDate>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut max = doc! {
            "fresh_watermark": watermarks.fresh_watermark,
            "settled_watermark": watermarks.settled_watermark,
        };
        if let Some(date) = refreshed_on {
            max.insert("refreshed_on", date.to_string());
        }
        let collection = self.db.collection::<Document>(STATE);
        let filter = doc! { "_id": SellableSkuProjectionState::ID };
        mongo_ops::update_one(&collection, filter, doc! { "$max": max }, false, executor).await?;
        Ok(())
    }

    /// 全量构建完成后标记就绪；口径版本已被更新时不标记。
    ///
    /// # 参数
    /// * `version` - 完成构建的口径版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// MongoDB 写入失败时返回错误。
    pub async fn mark_ready(&self, version: i32, executor: &mut dyn Executor) -> Result<()> {
        let collection = self.db.collection::<Document>(STATE);
        let filter = doc! { "_id": SellableSkuProjectionState::ID, "version": version };
        mongo_ops::update_one(&collection, filter, doc! { "$set": { "ready": true } }, false, executor)
            .await?;
        Ok(())
    }

    /// 返回时间窗内 SKU、SKU 修订、商品或商品修订有变化的 SKU，含已删除记录。
    ///
    /// # 参数
    /// * `window` - 变更时间窗
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重后的 SKU。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn changed_sku_ids(
        &self,
        window: ChangeWindow,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let filter = window_filter(window);
        let ids = self.db.collection::<StableIdRow>(SKUS);
        let mut sku_ids = find_values(&ids, filter.clone(), "id", |row| row.id, executor).await?;
        let revisions = self.db.collection::<SkuRefRow>(SKU_REVISIONS);
        sku_ids.extend(find_values(&revisions, filter.clone(), "sku_id", |row| row.sku_id, executor).await?);
        let products = self.db.collection::<StableIdRow>(PRODUCTS);
        let mut product_ids = find_values(&products, filter.clone(), "id", |row| row.id, executor).await?;
        let product_revisions = self.db.collection::<ProductRefRow>(PRODUCT_REVISIONS);
        product_ids.extend(
            find_values(&product_revisions, filter, "product_id", |row| row.product_id, executor).await?,
        );
        if !product_ids.is_empty() {
            let filter = doc! { "product_id": { "$in": product_ids.into_iter().collect::<Vec<_>>() } };
            sku_ids.extend(find_values(&ids, filter, "id", |row| row.id, executor).await?);
        }
        Ok(sku_ids.into_iter().collect())
    }

    /// 返回全部未删除 SKU，供全量重建。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重后的 SKU。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn all_sku_ids(&self, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let ids = self.db.collection::<StableIdRow>(SKUS);
        let filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        Ok(find_values(&ids, filter, "id", |row| row.id, executor).await?.into_iter().collect())
    }

    /// 按投影查询公司商品池分页。
    ///
    /// # 参数
    /// * `filter` - 业务筛选、排序与分页
    /// * `restrict` - 只在这些 SKU 中查询（配送地区命中与勾选的交集）；`None` 表示不限
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页可售行与总数。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn search(
        &self,
        filter: &SellableSkuFilter,
        restrict: Option<&[String]>,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>> {
        let query = projection_filter(filter, restrict);
        let total = mongo_ops::count_documents(&self.documents(), query.clone(), executor).await?;
        let options = FindOptions::builder()
            .sort(projection_sort(filter.sort))
            .skip(filter.page.saturating_sub(1) * u64::from(filter.page_size))
            .limit(i64::from(filter.page_size))
            .projection(doc! { "_id": 0, "row": 1 })
            .build();
        let rows = self.db.collection::<ProjectionRow>(PROJECTIONS);
        let items = mongo_ops::find_many(&rows, query, options, executor).await?;
        Ok(PageResult { items: items.into_iter().filter_map(|item| item.row).collect(), total: total as i64 })
    }
}

/// 读取一列并去重。
async fn find_values<T: serde::de::DeserializeOwned + Send + Sync>(
    collection: &Collection<T>,
    filter: Document,
    field: &str,
    value: impl Fn(T) -> String,
    executor: &mut dyn Executor,
) -> Result<BTreeSet<String>> {
    let options = FindOptions::builder().projection(doc! { "_id": 0, field: 1 }).build();
    Ok(mongo_ops::find_many(collection, filter, options, executor).await?.into_iter().map(value).collect())
}

fn window_filter(window: ChangeWindow) -> Document {
    doc! { "updated_at": { "$gte": window.from, "$lt": window.to } }
}

/// 价格筛选口径对应的行字段。
fn price_field(kind: SellablePriceKind) -> &'static str {
    match kind {
        SellablePriceKind::Dropship => "row.sales_visible_price_gross",
        SellablePriceKind::Bulk => "row.bulk_price_gross",
        SellablePriceKind::Market => "row.market_price",
    }
}

/// 投影筛选条件：只取可售行，业务条件与实时聚合一致。
fn projection_filter(filter: &SellableSkuFilter, restrict: Option<&[String]>) -> Document {
    let mut query = doc! { "row": { "$type": "object" } };
    if let Some(ids) = restrict {
        query.insert("_id", doc! { "$in": ids });
    }
    if let Some(kind) = filter.product_kind {
        query.insert("row.product_kind", kind.as_str());
    }
    for (field, value) in [
        ("row.category_id", &filter.category_id),
        ("row.brand_id", &filter.brand_id),
        ("row.supplier_ids", &filter.supplier_id),
    ] {
        if let Some(value) = value.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
            query.insert(field, value);
        }
    }
    if let Some(maximum) = filter.max_supplier_count {
        query.insert("row.supplier_count", doc! { "$lte": i64::from(maximum) });
    }
    insert_price_range(&mut query, filter);
    insert_keyword(&mut query, filter.keyword.as_deref());
    query
}

fn insert_price_range(query: &mut Document, filter: &SellableSkuFilter) {
    if filter.sales_price_min.is_none() && filter.sales_price_max.is_none() {
        return;
    }
    let mut range = doc! { "$type": "decimal" };
    for (operator, amount) in [("$gte", filter.sales_price_min), ("$lte", filter.sales_price_max)] {
        if let Some(amount) = amount {
            range.insert(operator, decimal(amount));
        }
    }
    query.insert(price_field(filter.price_kind), range);
}

/// 金额转为与库内价格一致的 Decimal128。
fn decimal(amount: Amount) -> Bson {
    Bson::Decimal128(amount.to_string().parse().expect("合法 Amount 必须可转换为 MongoDB Decimal128"))
}

fn insert_keyword(query: &mut Document, keyword: Option<&str>) {
    let Some(keyword) = keyword.map(str::trim).filter(|value| !value.is_empty()) else {
        return;
    };
    let pattern = regex::escape(keyword);
    let fields =
        ["row.sku_no", "row.name", "row.specification", "row.barcode", "row.product_no", "row.product_name"];
    let clauses = fields
        .into_iter()
        .map(|field| Bson::Document(doc! { field: { "$regex": &pattern, "$options": "i" } }))
        .collect::<Vec<_>>();
    query.insert("$or", clauses);
}

/// 排序：所选字段后按 SKU 编号与 SKU 稳定排序。
fn projection_sort(sort: SellableSort) -> Document {
    let direction = if sort.descending { -1 } else { 1 };
    let mut keys = doc! { (format!("row.{}", sort.field.row_field())): direction };
    if !keys.contains_key("row.sku_no") {
        keys.insert("row.sku_no", 1);
    }
    keys.insert("_id", 1);
    keys
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::money::Amount;
    use mongodb::bson::{Bson, doc, serialize_to_bson};

    use super::{
        SELLABLE_SKU_PROJECTION_VERSION, SellableSkuProjectionState, projection_filter, projection_sort,
    };
    use crate::entity::catalog::{ProductKind, SellablePriceKind};
    use crate::repository::catalog::{SellableSkuFilter, SellableSort, SellableSortField};

    fn date(value: &str) -> BusinessDate {
        value.parse().unwrap()
    }

    fn filter() -> SellableSkuFilter {
        SellableSkuFilter::as_of(date("2026-10-14"))
    }

    #[test]
    fn rebuild_daily_refresh_and_readiness() {
        assert!(SellableSkuProjectionState::needs_rebuild(None));
        let mut state = SellableSkuProjectionState::rebuilt(1_000, date("2026-10-14"));
        assert!(!SellableSkuProjectionState::needs_rebuild(Some(&state)));
        assert!(!state.serves_list());
        state.ready = true;
        assert!(state.serves_list());
        assert!(!state.needs_daily_refresh(date("2026-10-14")));
        assert!(state.needs_daily_refresh(date("2026-10-15")));
        state.version = SELLABLE_SKU_PROJECTION_VERSION - 1;
        assert!(SellableSkuProjectionState::needs_rebuild(Some(&state)));
        assert!(!state.serves_list());
    }

    #[test]
    fn empty_filter_only_keeps_sellable_rows() {
        assert_eq!(projection_filter(&filter(), None), doc! { "row": { "$type": "object" } });
    }

    #[test]
    fn product_kind_filter_matches_persisted_codes() {
        for kind in
            [ProductKind::Physical, ProductKind::Virtual, ProductKind::OfflineService, ProductKind::Voucher]
        {
            let mut value = filter();
            value.product_kind = Some(kind);
            let query = projection_filter(&value, None);
            assert_eq!(query.get("row.product_kind"), Some(&serialize_to_bson(&kind).unwrap()));
        }
    }

    #[test]
    fn business_filters_map_to_row_fields() {
        let mut value = filter();
        value.product_kind = Some(ProductKind::Physical);
        value.category_id = Some("cat-1".into());
        value.brand_id = Some(" ".into());
        value.supplier_id = Some("supplier-1".into());
        value.max_supplier_count = Some(1);
        value.price_kind = SellablePriceKind::Bulk;
        value.sales_price_min = Some(Amount::from_str("10.00").unwrap());
        let restrict = vec!["sku-1".to_string()];
        let query = projection_filter(&value, Some(&restrict));
        assert_eq!(query.get_document("_id").unwrap(), &doc! { "$in": ["sku-1"] });
        assert_eq!(query.get_str("row.product_kind").unwrap(), "PHYSICAL");
        assert_eq!(query.get_str("row.category_id").unwrap(), "cat-1");
        assert!(!query.contains_key("row.brand_id"));
        assert_eq!(query.get_str("row.supplier_ids").unwrap(), "supplier-1");
        assert_eq!(query.get_document("row.supplier_count").unwrap(), &doc! { "$lte": 1_i64 });
        let price = query.get_document("row.bulk_price_gross").unwrap();
        assert_eq!(price.get_str("$type").unwrap(), "decimal");
        assert!(matches!(price.get("$gte"), Some(Bson::Decimal128(_))));
        assert!(!price.contains_key("$lte"));
    }

    #[test]
    fn keyword_is_literal_and_spans_sku_and_product_fields() {
        let mut value = filter();
        value.keyword = Some(" a.b ".into());
        let query = projection_filter(&value, None);
        let clauses = query.get_array("$or").unwrap();
        assert_eq!(clauses.len(), 6);
        assert_eq!(
            clauses[5].as_document().unwrap(),
            &doc! { "row.product_name": { "$regex": "a\\.b", "$options": "i" } }
        );
    }

    /// 投影行必须保留 Decimal128 价格，否则价格区间与排序会按字符串比较。
    #[test]
    fn stored_row_keeps_native_decimal_prices() {
        let row: crate::repository::catalog::SellableSkuRow = serde_json::from_value(serde_json::json!({
            "sku_id": "sku-1", "sku_version": 1, "sku_revision_id": "rev-1", "sku_revision_no": 1,
            "sku_no": "SKU-01", "product_id": "product-1", "product_no": "P-01", "product_kind": "PHYSICAL",
            "name": "礼盒", "specification_signature": "", "base_unit_id": "unit-1",
            "sales_visible_price_gross": "99.90", "bulk_price_gross": "80.00", "market_price": null,
            "effective_from": "2026-01-01", "supplier_count": 1
        }))
        .unwrap();
        let stored = persistence_core::persisted_document(&row).unwrap();
        assert!(matches!(stored.get("sales_visible_price_gross"), Some(Bson::Decimal128(_))));
        assert!(matches!(stored.get("bulk_price_gross"), Some(Bson::Decimal128(_))));
        assert_eq!(stored.get_str("product_kind").unwrap(), "PHYSICAL");
    }

    #[test]
    fn sort_falls_back_to_sku_number_and_identity() {
        assert_eq!(projection_sort(SellableSort::default()), doc! { "row.sku_no": 1, "_id": 1 });
        let price = SellableSort { field: SellableSortField::DropshipPrice, descending: true };
        assert_eq!(
            projection_sort(price),
            doc! { "row.sales_visible_price_gross": -1, "row.sku_no": 1, "_id": 1 }
        );
        let sku_desc = SellableSort { field: SellableSortField::SkuNo, descending: true };
        assert_eq!(projection_sort(sku_desc), doc! { "row.sku_no": -1, "_id": 1 });
    }
}
