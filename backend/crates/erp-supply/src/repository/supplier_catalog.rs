//! 供应商商品观察与扫描检查点；所有查询限定连接且使用同一筛选口径。
use std::str::FromStr;

use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::{Decimal128, Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Repository, Result, insert_literal_regex_filter, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::dto::supplier_catalog::{CatalogFilter, CatalogSelectionStatus};
use crate::entity::supplier_catalog::{
    CatalogIdentityReviewReceipt, SupplierCatalogItem, SupplierCatalogRun,
};

pub const ITEMS: &str = "supplier_catalog_items";
pub const RUNS: &str = "supplier_catalog_runs";
pub const IDENTITY_REVIEWS: &str = "supplier_catalog_identity_reviews";

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
    for (field, value) in [("category_id", &query.category_id), ("brand_id", &query.brand_id)] {
        if let Some(value) = value.as_deref().filter(|value| !value.trim().is_empty()) {
            filter.insert(field, value);
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
    if let Some(state) = query.availability {
        let value = serde_json::to_value(state).expect("无参数枚举能够序列化");
        if state == crate::ports::connector::offer::CatalogAvailability::Unknown {
            filter.insert("$and", vec![doc! { "$or": [{ "availability": value.as_str().expect("枚举为字符串") }, {"availability": {"$exists": false}}] }]);
        } else {
            filter.insert("availability", value.as_str().expect("枚举为字符串"));
        }
    }
    match query.selection_status.unwrap_or_default() {
        CatalogSelectionStatus::All => {},
        CatalogSelectionStatus::Selected => {
            filter.insert("offering_id", doc! { "$exists": true, "$ne": mongodb::bson::Bson::Null });
        },
        CatalogSelectionStatus::Unselected => {
            filter.insert("offering_id", mongodb::bson::Bson::Null);
        },
    }
    price_filter(&mut filter, query)?;
    Ok(filter)
}

fn price_filter(filter: &mut Document, query: &CatalogFilter) -> Result<()> {
    let mut conditions = Vec::new();
    for (operator, value) in [("$gte", query.price_min), ("$lte", query.price_max)] {
        if let Some(value) = value {
            let price = Decimal128::from_str(&value.to_string()).map_err(|_| {
                persistence_core::Error::EntityMetadataOutOfRange("目录报价超过Decimal128范围")
            })?;
            let numeric = doc! { "$convert": { "input": "$purchase_price", "to": "decimal", "onError": mongodb::bson::Bson::Null, "onNull": mongodb::bson::Bson::Null } };
            conditions.push(doc! { operator: [numeric, mongodb::bson::Bson::Decimal128(price)] });
        }
    }
    if !conditions.is_empty() {
        filter.insert("purchase_price", doc! { "$ne": mongodb::bson::Bson::Null });
        filter.insert("$expr", doc! { "$and": conditions });
    }
    Ok(())
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
    match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<CatalogFilterOption>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect()
                .await
        },
        None => collection.aggregate(pipeline).with_type::<CatalogFilterOption>().await?.try_collect().await,
    }
    .map_err(persistence_core::Error::from)
}

/// 有界遍历已明确类型的供应商连接，调度方继续核对既往采集授权。
/// # 参数
/// `after` 为上一批最后连接主键，空串从头开始。
/// # 返回
/// 主键升序的最多二十条当前连接。
/// # 错误
/// 存储读取失败时返回错误。
pub async fn connection_page(
    db: &Database,
    after: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<crate::entity::supplier_api::SupplierApiConnection>> {
    use crate::repository::SupplierApiExt;
    let collection = db.collection::<crate::entity::supplier_api::SupplierApiConnection>(
        <Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS,
    );
    mongo_ops::find_many(
        &collection,
        doc! {"id": {"$gt": after}, "deleted_at":0_i64, "connector_type":"dangaoshushu"},
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

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use crate::entity::supplier_offering::SupplierOfferingAvailability;
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
