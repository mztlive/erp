//! 门户账号及定向开放的固定供应商聚合分页。

use application_core::PageView;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_catalog::CatalogExt;
use erp_identity::PortalAccountView;
use erp_identity::repository::AccessControlExt;
use erp_identity::repository::portal::PortalIdentityExt;
use erp_supply::portal::PortalSupplyExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::Executor;
use serde::Deserialize;
use serde::de::DeserializeOwned;

use super::super::management::PortalGrantView;
use super::query::{PortalQuery, aggregate};
use crate::Result;

#[derive(Deserialize)]
struct ManagementPage<T> {
    items: Vec<T>,
    count: Vec<ManagementCount>,
}

#[derive(Deserialize)]
struct ManagementCount {
    total: i64,
}

/// 固定绑定供应商并只投影安全账号字段。
pub(in crate::supplier_portal) async fn accounts_page(
    db: &Database,
    supplier_id: &str,
    query: &PortalQuery,
    active: Option<bool>,
    executor: &mut dyn Executor,
) -> Result<PageView<PortalAccountView>> {
    let collection = db.portal_bindings().collection().clone_with_type::<Document>();
    let mut pipeline = account_pipeline(supplier_id, db.accounts().collection().name());
    pipeline.extend(management_filter(query, active, &["account", "name"]));
    pipeline.push(doc! {"$sort":{"account":1,"account_id":1}});
    pipeline.push(page_stage(query));
    management_page(aggregate(collection, pipeline, executor).await?, query)
}

/// 固定开放供应商并保留可撤销的失效SKU关系。
pub(in crate::supplier_portal) async fn grants_page(
    db: &Database,
    supplier_id: &str,
    query: &PortalQuery,
    active: Option<bool>,
    executor: &mut dyn Executor,
) -> Result<PageView<PortalGrantView>> {
    let collection = db.portal_quote_grants().collection().clone_with_type::<Document>();
    let mut pipeline = grant_pipeline(supplier_id);
    pipeline.extend(management_filter(query, active, &["sku_no", "name", "specification"]));
    pipeline.push(doc! {"$sort":{"sku_no":1,"id":1}});
    pipeline.push(page_stage(query));
    management_page(aggregate(collection, pipeline, executor).await?, query)
}

/// 启停和搜索都在分页与统计之前执行。
fn management_filter(query: &PortalQuery, active: Option<bool>, fields: &[&str]) -> Vec<Document> {
    let mut stages = Vec::new();
    if let Some(active) = active {
        stages.push(doc! {"$match":{"active":active}});
    }
    if let Some(q) = &query.q {
        let pattern = regex::escape(q);
        let branches =
            fields.iter().map(|field| doc! { *field: {"$regex":&pattern,"$options":"i"}}).collect::<Vec<_>>();
        stages.push(doc! {"$match":{"$or":branches}});
    }
    stages
}

/// 只从供应商绑定联查外部账号，停用仍可被管理。
fn account_pipeline(supplier_id: &str, accounts: &str) -> Vec<Document> {
    vec![
        doc! {"$match":{"supplier_id":supplier_id,"deleted_at":NOT_DELETED_TIMESTAMP_BSON}},
        doc! {"$lookup":{"from":accounts,"localField":"account_id","foreignField":"id","as":"account"}},
        doc! {"$unwind":"$account"},
        doc! {"$match":{"account.kind":"supplier","account.deleted_at":NOT_DELETED_TIMESTAMP_BSON}},
        doc! {"$project":{"_id":0,"account_id":1,"account":"$account.account","name":"$account.name","supplier_id":1,"role":1,"active":{"$and":["$active",{"$eq":["$account.status","active"]}]},"account_version":"$account.version","binding_version":"$version"}},
    ]
}

/// 缺失或停用 SKU 仍保留可撤销的关系身份。
fn grant_pipeline(supplier_id: &str) -> Vec<Document> {
    vec![
        doc! {"$match":{"supplier_id":supplier_id,"deleted_at":NOT_DELETED_TIMESTAMP_BSON}},
        doc! {"$lookup":{"from":<Database as CatalogExt>::SKUS,"localField":"sku_id","foreignField":"id","as":"sku"}},
        doc! {"$unwind":{"path":"$sku","preserveNullAndEmptyArrays":true}},
        doc! {"$lookup":{"from":<Database as CatalogExt>::SKU_REVISIONS,"localField":"sku.current_revision_id","foreignField":"id","as":"revision"}},
        doc! {"$unwind":{"path":"$revision","preserveNullAndEmptyArrays":true}},
        doc! {"$project":{"_id":0,"id":1,"supplier_id":1,"sku_id":1,"active":1,"version":1,"sku_no":"$sku.sku_no","name":"$revision.name","specification":"$revision.specification"}},
    ]
}

/// 同一筛选集合同时生成页与总数。
fn page_stage(query: &PortalQuery) -> Document {
    doc! {"$facet":{"items":[{"$skip":query.skip},{"$limit":i64::from(query.page_size)}],"count":[{"$count":"total"}]}}
}

/// 空集合也返回稳定的分页信封。
fn management_page<T: DeserializeOwned>(
    pages: Vec<ManagementPage<T>>,
    query: &PortalQuery,
) -> Result<PageView<T>> {
    let (items, total) = pages
        .into_iter()
        .next()
        .map(|page| (page.items, page.count.first().map_or(0, |count| count.total)))
        .unwrap_or_default();
    Ok(PageView { items, total, page: query.page, page_size: query.page_size })
}
