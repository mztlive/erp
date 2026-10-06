//! 定向目录的有界集合联查与字段允许列表。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_catalog::CatalogExt;
use erp_supply::portal::PortalSupplyExt;
use erp_supply::repository::SupplierOfferingExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;
use serde::Deserialize;

use super::super::catalog::PortalCatalogSku;
use super::catalog_facts::{catalog_projection, current_catalog_facts, join};
use super::query::{PortalQuery, aggregate};
use crate::Result;

#[derive(Deserialize)]
struct CatalogPage {
    items: Vec<PortalCatalogSku>,
    count: Vec<CatalogCount>,
}
#[derive(Deserialize)]
struct CatalogCount {
    total: i64,
}

/// 从本供应商开放关系出发读取，不扩大到全公司商品。
/// # 参数
/// `db` 为数据库，`supplier_id` 为验证绑定，`query` 为有界筛选，`executor` 为当前执行器。
/// # 返回
/// 返回同一资格过滤下的外部目录和总数。
/// # 错误
/// 联查、聚合或允许列表解析失败时拒绝。
pub(in crate::supplier_portal) async fn catalog_page(
    db: &Database,
    supplier_id: &str,
    query: &PortalQuery,
    executor: &mut dyn Executor,
) -> Result<(Vec<PortalCatalogSku>, i64)> {
    let collection = db.portal_quote_grants().collection().clone_with_type::<Document>();
    let page = aggregate::<CatalogPage>(collection, catalog_pipeline(supplier_id, query), executor)
        .await?
        .into_iter()
        .next();
    let Some(page) = page else {
        return Ok((Vec::new(), 0));
    };
    let items = page.items.into_iter().map(PortalCatalogSku::freeze_current).collect();
    Ok((items, page.count.first().map_or(0, |count| count.total)))
}

/// 开放、正式状态与搜索在分页前执行，保持目录和统计一致。
fn catalog_pipeline(supplier_id: &str, query: &PortalQuery) -> Vec<Document> {
    let mut pipeline = vec![
        doc! {"$match":{"supplier_id":supplier_id,"active":true,"deleted_at":NOT_DELETED_TIMESTAMP_BSON}},
    ];
    pipeline.extend(join(<Database as CatalogExt>::SKUS, "sku_id", "sku"));
    pipeline.extend(current_catalog_facts());
    if let Some(q) = &query.q {
        let regex = regex::escape(q);
        pipeline.push(doc! {"$match":{"$or":[{"sku.sku_no":{"$regex":&regex,"$options":"i"}},{"revision.name":{"$regex":&regex,"$options":"i"}},{"sku.specification_signature":{"$regex":regex,"$options":"i"}}]}});
    }
    pipeline.push(own_offering(supplier_id));
    pipeline.push(catalog_projection());
    pipeline.push(doc! {"$sort":{"sku_no":1,"id":1}});
    pipeline.push(doc! {"$facet":{"items":[{"$skip":query.skip},{"$limit":i64::from(query.page_size)}],"count":[{"$count":"total"}]}});
    pipeline
}

/// 只联查本供应商供给编号，不读取或投影任何其他报价。
fn own_offering(supplier_id: &str) -> Document {
    doc! {"$lookup":{"from":<Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS,"let":{"sku_id":"$sku.id"},"pipeline":[{"$match":{"supplier_id":supplier_id,"deleted_at":NOT_DELETED_TIMESTAMP_BSON,"$expr":{"$eq":["$sku_id","$$sku_id"]}}},{"$sort":{"id":1}},{"$limit":1},{"$project":{"_id":0,"id":1}}],"as":"own_offering"}}
}

/// 图片来源复用目录的当前修订和启用关联，不能扩大到任意历史图。
/// # 参数
/// `db` 为数据库，`sku_id` 为已授权来源，`executor` 为授权读取执行器。
/// # 返回
/// 返回精确当前启用事实，失效身份或指针返回空。
/// # 错误
/// 联查或解析失败时拒绝。
pub(in crate::supplier_portal) async fn current_sku(
    db: &Database,
    sku_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PortalCatalogSku>> {
    let mut pipeline = vec![doc! {"$match":{"id":sku_id}}, doc! {"$replaceWith":{"sku":"$$ROOT"}}];
    pipeline.extend(current_catalog_facts());
    pipeline.push(catalog_projection());
    let collection = db.skus().collection().clone_with_type::<Document>();
    Ok(aggregate::<PortalCatalogSku>(collection, pipeline, executor)
        .await?
        .into_iter()
        .next()
        .map(PortalCatalogSku::freeze_current))
}
