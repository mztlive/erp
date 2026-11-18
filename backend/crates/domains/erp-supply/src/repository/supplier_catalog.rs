//! 供应商商品观察与扫描检查点；所有查询限定连接且使用同一筛选口径。
use std::str::FromStr;

use mongodb::Database;
use mongodb::bson::{Bson, Decimal128, Document, doc, serialize_to_bson};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, aggregate, insert_literal_regex_filter, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::dto::supplier_catalog::{CatalogFilter, CatalogSelectionStatus};
use crate::entity::supplier_api::SupplierConnectorType;
use crate::entity::supplier_catalog::{
    CATALOG_CHECKPOINT_VERSION, CatalogIdentityReviewReceipt, SupplierCatalogItem, SupplierCatalogRun,
};
use crate::ports::connector::offer::CatalogAvailability;

pub const ITEMS: &str = "supplier_catalog_items";
pub const RUNS: &str = "supplier_catalog_runs";
pub const IDENTITY_REVIEWS: &str = "supplier_catalog_identity_reviews";

/// 来源分组消息只能补查本连接已有目录，不推导或合并公司商品身份。
pub enum CatalogMessageTarget<'a> {
    Product(&'a str),
    Brand(&'a str),
}

/// 按稳定目录主键继续分组回调，不因品牌包含超过五十个规格而截断。
/// # 参数
/// `connection` 为绑定连接，`target` 为来源分组，`after` 为已提交主键。
/// # 返回
/// 下一个规格；没有剩余时返回 None。
/// # 错误
/// 查询失败返回原仓储错误。
pub async fn next_callback_target(
    db: &Database,
    connection: &str,
    target: CatalogMessageTarget<'_>,
    after: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierCatalogItem>> {
    let mut filter = doc! {"connection_id":connection,"deleted_at":0_i64};
    match target {
        CatalogMessageTarget::Product(id) => {
            filter.insert("product_id", id);
        },
        CatalogMessageTarget::Brand(id) => {
            filter.insert("brand_id", id);
        },
    }
    if let Some(after) = after {
        filter.insert("id", doc! {"$gt":after});
    }

    let mut items = mongo_ops::find_many(
        &db.collection::<SupplierCatalogItem>(ITEMS),
        filter,
        FindOptions::builder().sort(doc! {"id":1}).limit(1).build(),
        executor,
    )
    .await?;
    Ok(items.pop())
}

/// 有界读取来源商品或品牌消息涉及的当前目录规格。
/// # 参数
/// `connection` 来自服务端绑定；`target` 是适配器解码的来源分组。
/// # 返回
/// 最多51条记录；调用方只允许一次处理50条，超限须保留消息并完整重建基线。
/// # 错误
/// 仓储查询失败返回原错误。
pub async fn message_targets(
    db: &Database,
    connection: &str,
    target: CatalogMessageTarget<'_>,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCatalogItem>> {
    let mut filter = doc! {"connection_id":connection,"deleted_at":0_i64};
    match target {
        CatalogMessageTarget::Product(id) => {
            filter.insert("product_id", id);
        },
        CatalogMessageTarget::Brand(id) => {
            filter.insert("brand_id", id);
        },
    }
    mongo_ops::find_many(
        &db.collection::<SupplierCatalogItem>(ITEMS),
        filter,
        FindOptions::builder().limit(51).build(),
        executor,
    )
    .await
}

/// 直接读取当前配置与映射版本的成功检查点，不受任务展示条数限制。
/// # 参数
/// `full_only` 只读取无跳过项的成功全量；普通查询保留跳过项以阻止错误增量。
/// # 返回
/// 冻结截止时间最近的一条已成功结算记录，缺失返回 None。
/// # 错误
/// 数据库读取失败时返回错误。
pub async fn successful_run(
    db: &Database,
    connection: &str,
    config_version: u64,
    mapping_version: u32,
    full_only: bool,
    executor: &mut dyn Executor,
) -> Result<Option<SupplierCatalogRun>> {
    let config_version = i64::try_from(config_version)
        .map_err(|_| persistence_core::Error::EntityMetadataOutOfRange("技术配置版本超限"))?;
    let mut filter = doc! { "connection_id": connection, "technical_config_version": config_version,
    "mapping_version": i64::from(mapping_version), "checkpoint_version": i64::from(CATALOG_CHECKPOINT_VERSION), "succeeded": true, "finished": true, "deleted_at": 0_i64 };
    if full_only {
        filter.insert("sync_mode", "FULL");
        filter.insert("skipped_count", 0_i64);
    }

    let mut runs = mongo_ops::find_many(
        &db.collection::<SupplierCatalogRun>(RUNS),
        filter,
        FindOptions::builder().sort(doc! { "scan_through": -1, "id": -1 }).limit(1).build(),
        executor,
    )
    .await?;
    Ok(runs.pop())
}

/// 有界读取到期的来源图片处理任务。
/// # 参数
/// `now` 为服务器当前秒数，执行器不跨外部请求。
/// # 返回
/// 按到期时间排序的最多十条目录记录。
/// # 错误
/// 数据库读取失败时返回错误。
pub async fn due_media(
    db: &Database,
    now: i64,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCatalogItem>> {
    mongo_ops::find_many(
        &db.collection::<SupplierCatalogItem>(ITEMS),
        doc! {"media_retry_at": {"$type": "number", "$lte": now}, "deleted_at": 0_i64},
        FindOptions::builder().sort(doc! {"media_retry_at": 1, "id": 1}).limit(10).build(),
        executor,
    )
    .await
}

/// 供应商目录仓储访问入口。
pub trait SupplierCatalogExt {
    /// 取得来源身份人工核对命令回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn supplier_catalog_identity_reviews(&self) -> Repository<'_, CatalogIdentityReviewReceipt>;
    /// 取得目录记录仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn supplier_catalog_items(&self) -> Repository<'_, SupplierCatalogItem>;
    /// 取得扫描检查点仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前数据库仓储。
    /// # 错误
    /// 不返回错误。
    fn supplier_catalog_runs(&self) -> Repository<'_, SupplierCatalogRun>;
}
impl SupplierCatalogExt for Database {
    fn supplier_catalog_identity_reviews(&self) -> Repository<'_, CatalogIdentityReviewReceipt> {
        Repository::new(self, IDENTITY_REVIEWS)
    }
    fn supplier_catalog_items(&self) -> Repository<'_, SupplierCatalogItem> {
        Repository::new(self, ITEMS)
    }
    fn supplier_catalog_runs(&self) -> Repository<'_, SupplierCatalogRun> {
        Repository::new(self, RUNS)
    }
}

/// 分页读取当前连接目录，稳定按名称和身份排序。
/// # 参数
/// 分页已由应用边界验证；执行器沿用调用方事务。
/// # 返回
/// 目录记录与本连接总数。
/// # 错误
/// 数据库查询失败时返回错误。
pub async fn page(
    db: &Database,
    connection: &str,
    page: u32,
    size: u32,
    executor: &mut dyn Executor,
) -> Result<(Vec<SupplierCatalogItem>, u64)> {
    filtered_page(db, connection, &CatalogFilter::default(), page, size, executor).await
}

/// 使用统一条件读取分页与准确总数，供列表及选品预览共用。
/// # 参数
/// 筛选和分页由应用边界校验，连接由服务端授权上下文提供。
/// # 返回
/// 过滤后的当前页和同口径总数。
/// # 错误
/// 数据库读取或金额转换失败时返回错误。
pub async fn filtered_page(
    db: &Database,
    connection: &str,
    query: &CatalogFilter,
    page: u32,
    size: u32,
    executor: &mut dyn Executor,
) -> Result<(Vec<SupplierCatalogItem>, u64)> {
    let collection = db.collection::<SupplierCatalogItem>(ITEMS);
    let filter = catalog_filter(connection, query)?;

    let total = mongo_ops::count_documents(&collection, filter.clone(), executor).await?;

    let options = FindOptions::builder()
        .sort(doc! { "name": 1, "id": 1 })
        .skip(u64::from(page.saturating_sub(1)) * u64::from(size))
        .limit(i64::from(size))
        .build();
    let items = mongo_ops::find_many(&collection, filter, options, executor).await?;
    Ok((items, total))
}

fn catalog_filter(connection: &str, query: &CatalogFilter) -> Result<Document> {
    let mut filter = doc! { "connection_id": connection, "deleted_at": 0_i64 };
    for (field, values) in [("category_id", query.categories()), ("brand_id", query.brands())] {
        if !values.is_empty() {
            filter.insert(field, doc! { "$in": values });
        }
    }

    if let Some(keyword) = query.keyword.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        let alternatives = ["name", "specification", "product_id", "spec_id", "brand_name"].map(|field| {
            let mut item = Document::new();
            insert_literal_regex_filter(&mut item, field, Some(keyword));
            item
        });
        filter.insert("$or", alternatives.to_vec());
    }

    availability_filter(&mut filter, query.availability);
    let selection = if query.only_unadopted {
        CatalogSelectionStatus::Unselected
    } else {
        query.selection_status.unwrap_or_default()
    };
    match selection {
        CatalogSelectionStatus::All => {},
        CatalogSelectionStatus::Selected => {
            filter.insert("offering_id", doc! { "$exists": true, "$ne": Bson::Null });
        },
        CatalogSelectionStatus::Unselected => {
            filter.insert("offering_id", Bson::Null);
        },
    }

    price_filter(&mut filter, query)?;
    Ok(filter)
}

fn availability_filter(filter: &mut Document, availability: Option<CatalogAvailability>) {
    if let Some(state) = availability {
        let value = serde_json::to_value(state).expect("无参数枚举能够序列化");
        if state == CatalogAvailability::Unknown {
            filter.insert("$and", vec![doc! { "$or": [{ "availability": value.as_str().expect("枚举为字符串") }, {"availability": {"$exists": false}}] }]);
        } else {
            filter.insert("availability", value.as_str().expect("枚举为字符串"));
        }
    }
}

fn price_filter(filter: &mut Document, query: &CatalogFilter) -> Result<()> {
    let mut conditions = Vec::new();
    let numeric = doc! { "$convert": { "input": "$purchase_price", "to": "decimal", "onError": Bson::Null, "onNull": Bson::Null } };
    for (operator, value) in [("$gte", query.price_min), ("$lte", query.price_max)] {
        if let Some(value) = value {
            let price = Decimal128::from_str(&value.to_string()).map_err(|_| {
                persistence_core::Error::EntityMetadataOutOfRange("目录报价超过Decimal128范围")
            })?;
            conditions.push(doc! { operator: [numeric.clone(), Bson::Decimal128(price)] });
        }
    }

    if !conditions.is_empty() {
        conditions.insert(0, doc! { "$ne": [numeric, Bson::Null] });
        filter.insert("purchase_price", doc! { "$ne": Bson::Null });
        filter.insert("$expr", doc! { "$and": conditions });
    }
    Ok(())
}

/// 按稳定来源身份遍历完整筛选集合，不因网页页长截断批次范围。
/// # 参数
/// 连接与筛选须由调用方完成当前授权，`after_id` 为上页末项，`size` 为 1—500。
/// # 返回
/// 来源编号升序的有限一页；空页表示已遍历完成。
/// # 错误
/// 页长非法、查询条件转换或数据库读取失败时返回错误。
pub async fn filtered_after(
    db: &Database,
    connection: &str,
    query: &CatalogFilter,
    after_id: Option<&str>,
    size: u32,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCatalogItem>> {
    if !(1..=500).contains(&size) {
        return Err(persistence_core::Error::EntityMetadataOutOfRange("目录遍历页长必须为1至500"));
    }
    let mut filter = catalog_filter(connection, query)?;
    if let Some(after_id) = after_id {
        filter.insert("id", doc! { "$gt": after_id });
    }
    mongo_ops::find_many(
        &db.collection::<SupplierCatalogItem>(ITEMS),
        filter,
        FindOptions::builder().sort(doc! { "id": 1 }).limit(i64::from(size)).build(),
        executor,
    )
    .await
}

/// 同口径计算来源商品和规格数量，多个规格归入同一来源商品。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct CatalogCounts {
    pub product_total: u64,
    pub sku_total: u64,
}

/// 查询完整结果的商品数及 SKU 数，不以当前页数据推断全量。
/// # 参数
/// 连接、筛选和权限由调用方核验，执行器沿用当前读取上下文。
/// # 返回
/// 分别按来源商品身份去重、按来源 SKU 计数；无匹配时均为零。
/// # 错误
/// 条件无法编码或数据库聚合失败时返回错误。
pub async fn filtered_counts(
    db: &Database,
    connection: &str,
    query: &CatalogFilter,
    executor: &mut dyn Executor,
) -> Result<CatalogCounts> {
    let pipeline = vec![
        doc! { "$match": catalog_filter(connection, query)? },
        doc! { "$group": { "_id": "$product_id", "source_count": { "$sum": 1_i64 } } },
        doc! { "$group": { "_id": Bson::Null, "product_total": { "$sum": 1_i64 }, "sku_total": { "$sum": "$source_count" } } },
        doc! { "$project": { "_id": 0, "product_total": 1, "sku_total": 1 } },
    ];
    let collection = db.collection::<Document>(ITEMS);
    let counts = aggregate::rows(&collection.clone_with_type::<CatalogCounts>(), pipeline, executor)
        .await?
        .into_iter()
        .next();
    Ok(counts.unwrap_or_default())
}

/// 分类或品牌的来源筛选项，不建立公司字典。
#[derive(Debug, Deserialize, Serialize)]
pub struct CatalogFilterOption {
    pub id: String,
    pub name: String,
}

/// 有界读取当前连接真实来源分类或品牌。
/// # 参数
/// `category` 为 true 时取分类，否则取品牌；不接受外部字段名。
/// # 返回
/// 稳定排序的最多 500 个来源选项；未知名称显示来源编号。
/// # 错误
/// 数据库聚合或反序列化失败时返回错误。
pub async fn filter_options(
    db: &Database,
    connection: &str,
    category: bool,
    executor: &mut dyn Executor,
) -> Result<Vec<CatalogFilterOption>> {
    let (id, name) = if category { ("category_id", "category_name") } else { ("brand_id", "brand_name") };
    let pipeline = vec![
        doc! { "$match": { "connection_id": connection, "deleted_at": 0_i64, id: { "$type": "string", "$ne": "" } } },
        doc! { "$group": { "_id": format!("${id}"), "name": { "$max": format!("${name}") } } },
        doc! { "$sort": { "_id": 1 } },
        doc! { "$limit": 500_i64 },
        doc! { "$project": { "_id": 0, "id": "$_id", "name": { "$ifNull": ["$name", "$_id"] } } },
    ];
    let collection = db.collection::<Document>(ITEMS);
    aggregate::rows(&collection.clone_with_type::<CatalogFilterOption>(), pipeline, executor).await
}

/// 有界遍历已明确类型的供应商连接，调度方继续核对既往采集授权。
/// # 参数
/// `after` 为上一批最后连接主键，`kinds` 为注册表提供的候选协议；空集合不查询。
/// # 返回
/// 主键升序的最多二十条当前连接。
/// # 错误
/// 存储读取失败时返回错误。
pub async fn connection_page(
    db: &Database,
    after: &str,
    kinds: &[SupplierConnectorType],
    executor: &mut dyn Executor,
) -> Result<Vec<crate::entity::supplier_api::SupplierApiConnection>> {
    if kinds.is_empty() {
        return Ok(Vec::new());
    }
    let kinds =
        kinds.iter().map(|kind| serialize_to_bson(kind).expect("协议枚举可序列化")).collect::<Vec<_>>();
    use crate::repository::SupplierApiExt;
    let collection = db.collection::<crate::entity::supplier_api::SupplierApiConnection>(
        <Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS,
    );
    mongo_ops::find_many(
        &collection,
        doc! {"id": {"$gt": after}, "deleted_at":0_i64, "connector_type": {"$in": kinds}},
        FindOptions::builder().sort(doc! {"id":1}).limit(20).build(),
        executor,
    )
    .await
}

/// 使超过明确核验有效期的 API 观察停止参与新增销售及采购。
/// # 参数
/// 当前服务器时间与调用方执行器。
/// # 返回
/// 本轮最多一百条过期投影的持久化数量。
/// # 错误
/// 查询或乐观锁更新失败时返回错误，已更新条目保留。
pub async fn expire_availability(
    db: &Database,
    now: erp_core::common::time::Instant,
    executor: &mut dyn Executor,
) -> Result<usize> {
    use crate::entity::supplier_offering::{AvailabilityStatus, SupplierOfferingAvailability};
    use crate::repository::SupplierOfferingExt;
    let collection = db.collection::<SupplierOfferingAvailability>(
        <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES,
    );
    let rows = mongo_ops::find_many(&collection,
        doc! {"availability_status":"AVAILABLE", "valid_until":{"$type":"number", "$lte":now.unix_secs()}, "deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"valid_until":1,"id":1}).limit(100).build(),executor).await?;

    let count = rows.len();
    for mut row in rows {
        row.availability_status = AvailabilityStatus::Stale;
        db.supplier_offering_availabilities().update(&mut row, executor).await?;
    }
    Ok(count)
}

/// 一致性读取只投影来源身份，用于受理时冻结成员集合。
/// # 参数
/// 筛选或明确身份只能提供一种，调用方已验证结构与连接权限。
/// # 返回
/// 当前事务快照中属于该连接的身份列表。
/// # 错误
/// 筛选转换或数据库查询失败。
pub async fn selection_ids(
    db: &Database,
    connection: &str,
    query: Option<&CatalogFilter>,
    ids: Option<&[String]>,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct Identity {
        id: String,
    }
    let mut filter = catalog_filter(connection, query.unwrap_or(&CatalogFilter::default()))?;
    if let Some(ids) = ids {
        filter.insert("id", doc! {"$in":ids});
    }
    let rows = mongo_ops::find_many(
        &db.collection::<Identity>(ITEMS),
        filter,
        FindOptions::builder().projection(doc! {"_id":0,"id":1}).sort(doc! {"id":1}).build(),
        executor,
    )
    .await?;
    Ok(rows.into_iter().map(|row| row.id).collect())
}

/// 按固定成员身份批量读取当前来源资料，不重跑原筛选。
/// # 参数
/// `ids` 为已保存的有限分块，连接由调用方授权。
/// # 返回
/// 当前仍有效的来源资料；调用方必须核对数量，不能跳过缺失成员。
/// # 错误
/// 数据库读取失败。
pub async fn selection_sources(
    db: &Database,
    connection: &str,
    ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierCatalogItem>> {
    mongo_ops::find_many(
        &db.collection::<SupplierCatalogItem>(ITEMS),
        doc! {"connection_id":connection,"id":{"$in":ids},"deleted_at":0_i64},
        FindOptions::builder().sort(doc! {"id":1}).build(),
        executor,
    )
    .await
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use mongodb::bson::{Bson, doc};

    use super::catalog_filter;
    use crate::dto::supplier_catalog::{CatalogFilter, CatalogSelectionStatus};
    use crate::entity::supplier_offering::SupplierOfferingAvailability;

    #[test]
    fn combined_filters_keep_connection_scope_and_or_within_each_dimension() {
        let query: CatalogFilter = serde_json::from_value(serde_json::json!({
            "category_ids": ["rice", "oil"], "brand_ids": ["first", "second"],
            "availability": "AVAILABLE", "only_unadopted": true, "keyword": "gift.*"
        }))
        .unwrap();
        let filter = catalog_filter("connection", &query).unwrap();
        assert_eq!(filter.get_str("connection_id").unwrap(), "connection");
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(filter.get_document("category_id").unwrap(), &doc! {"$in":["oil", "rice"]});
        assert_eq!(filter.get_document("brand_id").unwrap(), &doc! {"$in":["first", "second"]});
        assert_eq!(filter.get_str("availability").unwrap(), "AVAILABLE");
        assert_eq!(filter.get("offering_id"), Some(&Bson::Null));
        assert_eq!(filter.get_array("$or").unwrap().len(), 5);
    }

    #[test]
    fn max_only_range_excludes_missing_or_invalid_quotes_and_uses_closed_decimal_bounds() {
        let max_only: CatalogFilter =
            serde_json::from_value(serde_json::json!({"price_max":"10.1001"})).unwrap();
        let filter = catalog_filter("connection", &max_only).unwrap();
        let conditions = filter.get_document("$expr").unwrap().get_array("$and").unwrap();
        assert!(conditions[0].as_document().unwrap().contains_key("$ne"));
        let upper = conditions[1].as_document().unwrap().get_array("$lte").unwrap();
        let Bson::Decimal128(price) = &upper[1] else {
            panic!("range bound must be an exact BSON decimal");
        };
        assert_eq!(price.to_string(), "10.1001");

        let closed: CatalogFilter =
            serde_json::from_value(serde_json::json!({"price_min":"0","price_max":"0"})).unwrap();
        let filter = catalog_filter("connection", &closed).unwrap();
        let conditions = filter.get_document("$expr").unwrap().get_array("$and").unwrap();
        assert!(conditions[1].as_document().unwrap().contains_key("$gte"));
        assert!(conditions[2].as_document().unwrap().contains_key("$lte"));
        assert_eq!(conditions.len(), 3);
    }

    #[test]
    fn adopted_filter_uses_real_offering_binding_and_unknown_availability_keeps_legacy_rows() {
        let query: CatalogFilter = serde_json::from_value(
            serde_json::json!({"selection_status":"SELECTED", "availability":"UNKNOWN"}),
        )
        .unwrap();
        assert_eq!(query.selection_status, Some(CatalogSelectionStatus::Selected));
        let filter = catalog_filter("connection", &query).unwrap();
        assert_eq!(filter.get_document("offering_id").unwrap(), &doc! {"$exists":true,"$ne":Bson::Null});
        assert!(!filter.contains_key("sku_id"));
        assert!(filter.get_array("$and").is_ok());
    }

    #[test]
    fn old_bson_timestamp_is_compatible_without_fabricating_missing_source_time() {
        let base = mongodb::bson::doc! {"id":"a","created_at":1_i64,"updated_at":1_i64,"deleted_at":0_i64,"version":1_i64,"supplier_offering_id":"o","availability_status":"AVAILABLE","available_quantity":mongodb::bson::Bson::Null,"source_updated_at":10_i64,"received_at":11_i64,"source_revision_token":mongodb::bson::Bson::Null,"updated_by":"user"};
        let old: SupplierOfferingAvailability =
            mongodb::bson::deserialize_from_document(base.clone()).unwrap();
        assert_eq!(old.source_updated_at, Some(Instant::from_unix_secs(10)));
        assert!(old.valid_until.is_none());
        assert!(old.is_available());

        let mut missing = base;
        missing.remove("source_updated_at");
        let missing: SupplierOfferingAvailability =
            mongodb::bson::deserialize_from_document(missing).unwrap();
        assert!(missing.source_updated_at.is_none());
        assert!(missing.verified_at.is_none());
    }
}
