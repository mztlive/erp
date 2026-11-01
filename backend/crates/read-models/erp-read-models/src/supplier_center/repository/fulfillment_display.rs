//! 供应商履约明细的冻结供给修订、公司 SKU 与计量单位批量读取。

use std::collections::HashMap;

use erp_catalog::CatalogExt;
use erp_core::ids::SkuId;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentItem;
use erp_supply::repository::SupplierOfferingExt;
use erp_supply::repository::prelude::*;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::Executor;

use crate::Result;

#[derive(Default)]
pub(crate) struct ItemNames {
    pub(crate) revisions: HashMap<String, (String, u32)>,
    pub(crate) offerings: HashMap<String, String>,
    pub(crate) skus: HashMap<String, (Option<String>, String)>,
    pub(crate) sku_revisions: HashMap<String, SkuRevisionName>,
    pub(crate) units: HashMap<String, String>,
}

/// 当前 SKU 修订的展示名称及其拥有的稳定身份。
pub(crate) struct SkuRevisionName {
    pub(crate) sku_id: SkuId,
    pub(crate) name: String,
}

/// 读取已授权履约明细关联的当前展示资料。
///
/// # 参数
/// `db` 为领域仓储入口，`items` 为当前订单明细，`executor` 沿用调用方查询上下文。
/// # 返回
/// 返回按冻结修订与 SKU 关联链索引的名称资料，缺失关联保留空值。
/// # 错误
/// 关联仓储读取失败时返回错误。
pub(crate) async fn item_names(
    db: &Database,
    items: &[SupplierFulfillmentItem],
    executor: &mut dyn Executor,
) -> Result<ItemNames> {
    if items.is_empty() {
        return Ok(ItemNames::default());
    }
    let ids = items.iter().map(|item| item.supplier_offering_revision_id.to_string()).collect::<Vec<_>>();
    let revisions = db.supplier_offering_revisions().list_by_ids(&ids, executor).await?;
    let offering_ids = revisions.iter().map(|item| item.supplier_offering_id.clone()).collect::<Vec<_>>();
    let offerings = db.supplier_offerings().list_by_ids(&offering_ids, executor).await?;
    let sku_ids = offerings.iter().map(|item| item.sku_id.to_string()).collect::<Vec<_>>();
    let skus = db.skus().find_many(doc! { "id": { "$in": sku_ids } }, executor).await?;
    let revision_ids =
        skus.iter().filter_map(|item| item.stable.current_revision_id.clone()).collect::<Vec<_>>();
    let unit_ids = skus.iter().map(|item| item.base_unit_id.to_string()).collect::<Vec<_>>();
    let sku_revisions =
        db.sku_revisions().find_many(doc! { "id": { "$in": revision_ids } }, executor).await?;
    let units = db.unit_of_measures().find_many(doc! { "id": { "$in": unit_ids } }, executor).await?;
    let names = ItemNames {
        revisions: revisions
            .into_iter()
            .map(|item| (item.base.id, (item.supplier_offering_id.to_string(), item.revision.revision_no)))
            .collect(),
        offerings: offerings.into_iter().map(|item| (item.base.id, item.sku_id.to_string())).collect(),
        skus: skus
            .into_iter()
            .map(|item| (item.base.id, (item.stable.current_revision_id, item.base_unit_id.to_string())))
            .collect(),
        sku_revisions: sku_revisions
            .into_iter()
            .map(|item| (item.base.id, SkuRevisionName { sku_id: item.sku_id, name: item.name }))
            .collect(),
        units: units.into_iter().map(|item| (item.base.id, item.name)).collect(),
    };
    Ok(names)
}
