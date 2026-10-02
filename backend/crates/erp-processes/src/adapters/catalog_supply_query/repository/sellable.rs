use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_catalog::entity::catalog::{EnableStatus, ListingStatus, ProductKind, Sku};
use erp_catalog::repository::{CatalogExt, SellableSkuFilter, SellableSkuRow};
use erp_core::common::time::BusinessDate;
use erp_core::money::Amount;
use erp_supply::entity::supplier_offering::{AvailabilityStatus, OfferingStatus};
use erp_supply::repository::SupplierOfferingExt;
use futures_util::TryStreamExt;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::{Executor, PageResult, Result};
use serde::{Deserialize, Serialize};

use super::CatalogSupplyRepository;
use super::shared::{PRODUCT_REVISIONS, SKU_REVISIONS, SKUS, SUPPLIER_OFFERINGS};

/// `supplier_offering_revision` 集合名（公司商品池资格依赖的当前供给修订）。
const SUPPLIER_OFFERING_REVISIONS: &str =
    <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS;
/// `supplier_offering_availability` 集合名（公司商品池资格依赖的实时可供投影）。
const SUPPLIER_OFFERING_AVAILABILITIES: &str =
    <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES;

/// 公司商品池聚合分页结果。
#[derive(Debug, Default, Deserialize)]
struct SellableSkuFacet {
    /// 当前页数据。
    items: Vec<SellableSkuRow>,
    /// 总数聚合行。
    total: Vec<SellableSkuTotal>,
}

/// 公司商品池总数聚合行。
#[derive(Debug, Deserialize)]
struct SellableSkuTotal {
    /// 符合资格的 SKU 数量。
    count: i64,
}

impl CatalogSupplyRepository<'_> {
    /// 分页查询销售可用的公司 SKU。
    ///
    /// 资格固定为：稳定 SKU 启用、当前 SKU 修订启用且处于生效区间并已配置
    /// 销售可见价、至少一条启用且当前修订可供并处于有效区间的供给。结果不含
    /// 采购成本、进项税率或供给起订量；供应商身份仅用于补齐业务编号。
    ///
    /// # 参数
    /// * `filter` - 关键字、类型、分类、品牌、供应商、区域、供应保障、销售价与分页
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页资格投影及资格结果总数。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn search_sellable_skus(
        &self,
        filter: &SellableSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>> {
        let match_doc = sellable_sku_match(None, None, &[]);
        let pipeline = sellable_sku_pipeline(match_doc, filter, Some((filter.page, filter.page_size)));
        let facet = self.aggregate_sellable_skus(pipeline, executor).await?;
        Ok(PageResult { items: facet.items, total: facet.total.first().map_or(0, |row| row.count) })
    }

    /// 批量校验销售单引用的精确公司 SKU 修订是否仍具备销售资格。
    ///
    /// # 参数
    /// * `refs` - 销售单锁定的稳定 SKU 与修订 ID 集合
    /// * `eligibility_as_of` - 服务端解释的销售资格业务日期
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回仍符合资格的精确引用；调用方必须与请求集合逐项比对并 fail-closed。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn find_sellable_sku_refs(
        &self,
        refs: &[(String, String)],
        eligibility_as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>> {
        if refs.is_empty() {
            return Ok(Vec::new());
        }
        let match_doc = sellable_sku_match(None, None, refs);
        let filter = SellableSkuFilter::as_of(eligibility_as_of);
        let pipeline = sellable_sku_pipeline(match_doc, &filter, None);
        Ok(self.aggregate_sellable_skus(pipeline, executor).await?.items)
    }

    /// 按稳定 SKU 身份取出当前可售修订。
    ///
    /// # 参数
    /// * `sku_ids` - 稳定 SKU 身份
    /// * `eligibility_as_of` - 资格业务日期
    /// * `executor` - 执行器
    ///
    /// # 返回
    /// 返回仍具备资格的行；缺失项不出现在结果中。
    ///
    /// # 错误
    /// 聚合失败。
    pub async fn find_sellable_skus_by_ids(
        &self,
        sku_ids: &[String],
        eligibility_as_of: BusinessDate,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SellableSkuRow>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        let mut match_doc = sellable_sku_match(None, None, &[]);
        match_doc.insert("id", doc! { "$in": sku_ids });
        let filter = SellableSkuFilter::as_of(eligibility_as_of);
        let pipeline = sellable_sku_pipeline(match_doc, &filter, None);
        Ok(self.aggregate_sellable_skus(pipeline, executor).await?.items)
    }

    /// 执行公司商品池类型化聚合并收集唯一的 facet 结果。
    async fn aggregate_sellable_skus(
        &self,
        pipeline: Vec<Document>,
        executor: &mut dyn Executor,
    ) -> Result<SellableSkuFacet> {
        let collection = self.db.collection::<Sku>(SKUS);
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SellableSkuFacet>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            },
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SellableSkuFacet>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            },
        };
        Ok(rows.into_iter().next().unwrap_or_default())
    }
}

fn sellable_sku_match(
    _keyword: Option<&str>,
    _product_kind: Option<ProductKind>,
    refs: &[(String, String)],
) -> Document {
    let mut filter = doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "status": EnableStatus::Active.as_str(),
        "listing_status": { "$in": [ListingStatus::Listed.as_str(), null] },
        "current_revision_id": { "$ne": null },
    };
    if !refs.is_empty() {
        filter.insert(
            "$or",
            refs.iter()
                .map(|(sku_id, sku_revision_id)| {
                    Bson::Document(doc! {
                        "id": sku_id,
                        "current_revision_id": sku_revision_id,
                    })
                })
                .collect::<Vec<_>>(),
        );
    }
    filter
}

/// 把可序列化枚举转换为 BSON 标量。
fn bson_for<T: Serialize>(value: T) -> Bson {
    mongodb::bson::serialize_to_bson(&value).expect("域枚举必须可序列化为 BSON")
}

/// 把查询金额转换为与库内价格一致的 BSON Decimal128。
///
/// # 参数
/// * `amount` - 已通过服务层校验的非负金额
///
/// # 返回
/// 返回可直接写入 `$match` 区间的 Decimal128 BSON。
///
/// # 错误
/// 无；合法 `Amount` 必须可转换为 Decimal128，否则在测试阶段即失败。
fn amount_filter_bson(amount: Amount) -> Bson {
    Bson::Decimal128(amount.to_string().parse().expect("合法 Amount 必须可转换为 MongoDB Decimal128"))
}

/// 在资格聚合完成后追加可选业务筛选。
///
/// # 参数
/// * `pipeline` - 已完成资格 lookup 的聚合阶段列表
/// * `filter` - 调用方可选筛选
///
/// # 返回
/// 无；就地追加 `$match` 阶段。
///
/// # 错误
/// 无。
fn append_sellable_optional_filters(pipeline: &mut Vec<Document>, filter: &SellableSkuFilter) {
    if let Some(kind) = filter.product_kind {
        pipeline.push(doc! { "$match": { "product.product_kind": bson_for(kind) } });
    }
    if let Some(category_id) = filter.category_id.as_deref().map(str::trim).filter(|value| !value.is_empty())
    {
        pipeline.push(doc! { "$match": { "product_revision.category_id": category_id } });
    }
    if let Some(brand_id) = filter.brand_id.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        pipeline.push(doc! { "$match": { "product_revision.brand_id": brand_id } });
    }
    if let Some(supplier_id) = filter.supplier_id.as_deref().map(str::trim).filter(|value| !value.is_empty())
    {
        // 仅匹配资格供给中的供应商；响应投影仍不返回供应商身份。
        pipeline.push(doc! { "$match": { "supplier_ids": supplier_id } });
    }
    if let Some(region) = filter.supply_region.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        pipeline.push(doc! { "$match": { "supply_regions": region } });
    }
    if filter.nationwide_only {
        pipeline.push(doc! { "$match": { "supply_regions": "全国" } });
    }
    if let Some(max_supplier_count) = filter.max_supplier_count {
        pipeline.push(doc! {
            "$match": {
                "$expr": {
                    "$lte": [
                        { "$size": { "$ifNull": ["$supplier_ids", []] } },
                        i64::from(max_supplier_count),
                    ]
                }
            }
        });
    }
    if filter.sales_price_min.is_some() || filter.sales_price_max.is_some() {
        let mut range = Document::new();
        if let Some(minimum) = filter.sales_price_min {
            range.insert("$gte", amount_filter_bson(minimum));
        }
        if let Some(maximum) = filter.sales_price_max {
            range.insert("$lte", amount_filter_bson(maximum));
        }
        pipeline.push(doc! {
            "$match": { "sku_revision.sales_visible_price_gross": range }
        });
    }
    if let Some(value) = filter.keyword.as_deref().map(str::trim).filter(|value| !value.is_empty()) {
        let pattern = regex::escape(value);
        pipeline.push(doc! {
            "$match": {
                "$or": [
                    { "sku_no": { "$regex": &pattern, "$options": "i" } },
                    { "sku_revision.name": { "$regex": &pattern, "$options": "i" } },
                    { "sku_revision.specification": { "$regex": &pattern, "$options": "i" } },
                    { "sku_revision.barcode": { "$regex": &pattern, "$options": "i" } },
                    { "product.product_no": { "$regex": &pattern, "$options": "i" } },
                    { "product_revision.name": { "$regex": &pattern, "$options": "i" } },
                ]
            }
        });
    }
}

/// 构造公司商品池资格与只读投影聚合管道。
///
/// # 参数
/// * `initial_match` - 稳定 SKU 初始匹配（含可选精确修订引用）
/// * `filter` - 可选业务筛选与资格业务日
/// * `paging` - 分页；`None` 表示返回全部匹配行（资格复核路径）
///
/// # 返回
/// 返回含 facet 分页与总数的聚合管道。
///
/// # 错误
/// 无。
fn sellable_sku_pipeline(
    initial_match: Document,
    filter: &SellableSkuFilter,
    paging: Option<(u64, u32)>,
) -> Vec<Document> {
    let as_of = filter.eligibility_as_of.to_string();
    let not_deleted = NOT_DELETED_TIMESTAMP_BSON;
    let mut pipeline = vec![
        doc! { "$match": initial_match },
        sellable_sku_revision_lookup(&as_of, not_deleted),
        doc! { "$unwind": "$sku_revision" },
        sellable_product_lookup(not_deleted),
        doc! { "$unwind": "$product" },
        sellable_product_revision_lookup(&as_of, not_deleted),
        doc! { "$unwind": "$product_revision" },
        sellable_unit_lookup(not_deleted),
        doc! { "$unwind": { "path": "$unit", "preserveNullAndEmptyArrays": true } },
        sellable_offering_lookup(&as_of, not_deleted),
        doc! { "$match": { "offerings.0": { "$exists": true } } },
        sellable_supply_summary(),
    ];
    append_sellable_optional_filters(&mut pipeline, filter);
    pipeline.push(doc! {
        "$facet": {
            "items": sellable_sku_item_stages(paging),
            "total": [{ "$count": "count" }],
        }
    });
    pipeline
}

/// 构造商品池 facet 当前页的稳定排序、分页与四价投影阶段。
///
/// # 参数
/// * `paging` - 页码与每页条数；缺省只保留原零偏移，不限制条数
///
/// # 返回
/// 返回按原顺序排列的当前页聚合阶段。
///
/// # 错误
/// 无。
fn sellable_sku_item_stages(paging: Option<(u64, u32)>) -> Vec<Document> {
    let (skip, limit) = paging.map_or((0_i64, i64::MAX), |(page, page_size)| {
        (((page.saturating_sub(1)) * u64::from(page_size)) as i64, i64::from(page_size))
    });
    let mut stages = vec![doc! { "$sort": { "sku_no": 1, "id": 1 } }, doc! { "$skip": skip }];
    if paging.is_some() {
        stages.push(doc! { "$limit": limit });
    }
    stages.push(sellable_sku_projection());
    stages
}

/// 投影公司 SKU 的四种含税参考价、集采起订量与内部供应商身份。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_sku_projection() -> Document {
    doc! {
        "$project": {
            "_id": 0,
            "sku_id": "$id",
            "sku_version": "$version",
            "sku_revision_id": "$sku_revision.id",
            "sku_revision_no": "$sku_revision.revision_no",
            "sku_no": 1,
            "product_id": 1,
            "product_no": "$product.product_no",
            "product_kind": "$product.product_kind",
            "name": "$sku_revision.name",
            "specification_signature": 1,
            "specification": "$sku_revision.specification",
            "barcode": "$sku_revision.barcode",
            "base_unit_id": 1,
            "base_unit_code": "$unit.unit_code",
            "base_unit_name": "$unit.name",
            "factory_price_gross": "$sku_revision.factory_price_gross",
            "sales_visible_price_gross": "$sku_revision.sales_visible_price_gross",
            "bulk_price_gross": "$sku_revision.bulk_price_gross",
            "bulk_min_quantity": "$sku_revision.bulk_min_quantity",
            "market_price": "$sku_revision.market_price",
            "main_image_asset_id": "$sku_revision.source_main_image_asset_id",
            "category_id": "$product_revision.category_id",
            "effective_from": "$sku_revision.effective_from",
            "effective_to": "$sku_revision.effective_to",
            "supplier_count": { "$size": "$supplier_ids" },
            "supplier_ids": 1,
            "supply_regions": 1,
        }
    }
}

/// 关联已启用、已维护一件代发价且在业务日期生效的当前 SKU 修订。
///
/// # 参数
/// * `as_of` - 资格业务日期
/// * `not_deleted` - 未软删除标记
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_sku_revision_lookup(as_of: &str, not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": SKU_REVISIONS,
            "let": { "revision_id": "$current_revision_id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$id", "$$revision_id"] },
                    "deleted_at": not_deleted,
                    "status": EnableStatus::Active.as_str(),
                    "sales_visible_price_gross": { "$ne": null },
                    "effective_from": { "$lte": as_of },
                    "$or": [
                        { "effective_to": null },
                        { "effective_to": { "$gt": as_of } },
                    ]
                }
            }],
            "as": "sku_revision"
        }
    }
}

/// 关联启用且具有当前修订的公司商品。
///
/// # 参数
/// * `not_deleted` - 未软删除标记
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_product_lookup(not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": <mongodb::Database as CatalogExt>::PRODUCTS,
            "let": { "product_id": "$product_id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$id", "$$product_id"] },
                    "deleted_at": not_deleted,
                    "status": EnableStatus::Active.as_str(),
                    "current_revision_id": { "$ne": null },
                }
            }],
            "as": "product"
        }
    }
}

/// 关联业务日期有效且已启用的当前商品修订。
///
/// # 参数
/// * `as_of` - 资格业务日期
/// * `not_deleted` - 未软删除标记
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_product_revision_lookup(as_of: &str, not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": PRODUCT_REVISIONS,
            "let": { "revision_id": "$product.current_revision_id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$id", "$$revision_id"] },
                    "deleted_at": not_deleted,
                    "status": EnableStatus::Active.as_str(),
                    "effective_from": { "$lte": as_of },
                    "$or": [
                        { "effective_to": null },
                        { "effective_to": { "$gt": as_of } },
                    ]
                }
            }],
            "as": "product_revision"
        }
    }
}

/// 关联未软删除的基础计量单位。
///
/// # 参数
/// * `not_deleted` - 未软删除标记
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_unit_lookup(not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": <mongodb::Database as CatalogExt>::UNIT_OF_MEASURES,
            "let": { "unit_id": "$base_unit_id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$id", "$$unit_id"] },
                    "deleted_at": not_deleted,
                }
            }],
            "as": "unit"
        }
    }
}

/// 关联业务日期有效的当前供应商供给条款修订。
///
/// # 参数
/// * `as_of` - 资格业务日期
/// * `not_deleted` - 未软删除标记
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_offering_revision_lookup(as_of: &str, not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": SUPPLIER_OFFERING_REVISIONS,
            "let": { "revision_id": "$current_revision_id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$id", "$$revision_id"] },
                    "deleted_at": not_deleted,
                    "valid_from": { "$lte": as_of },
                    "$or": [
                        { "valid_to": null },
                        { "valid_to": { "$gt": as_of } },
                    ]
                }
            }],
            "as": "revision"
        }
    }
}

/// 关联当前可供且可供数量为空或大于零的供给状态。
///
/// # 参数
/// * `not_deleted` - 未软删除标记
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_availability_lookup(not_deleted: i64) -> Document {
    let zero_quantity = Bson::Decimal128("0".parse().expect("零必须可转换为 MongoDB Decimal128"));
    doc! {
        "$lookup": {
            "from": SUPPLIER_OFFERING_AVAILABILITIES,
            "let": { "offering_id": "$id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$supplier_offering_id", "$$offering_id"] },
                    "deleted_at": not_deleted,
                    "availability_status": AvailabilityStatus::Available.as_str(),
                    "$or": [
                        { "available_quantity": null },
                        { "available_quantity": { "$gt": &zero_quantity } },
                    ],
                }
            }],
            "as": "availability"
        }
    }
}

/// 按原顺序关联启用供给、有效条款与实时可供状态。
///
/// # 参数
/// * `as_of` - 资格业务日期
/// * `not_deleted` - 未软删除标记
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_offering_lookup(as_of: &str, not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": SUPPLIER_OFFERINGS,
            "let": { "sku_id": "$id" },
            "pipeline": [
                doc! {
                    "$match": {
                        "$expr": { "$eq": ["$sku_id", "$$sku_id"] },
                        "deleted_at": not_deleted,
                        "status": OfferingStatus::Active.as_str(),
                        "current_revision_id": { "$ne": null },
                    }
                },
                sellable_offering_revision_lookup(as_of, not_deleted),
                doc! { "$unwind": "$revision" },
                sellable_availability_lookup(not_deleted),
                doc! { "$unwind": "$availability" },
                doc! { "$project": { "_id": 0, "supplier_id": 1, "supply_region": "$revision.supply_region" } },
            ],
            "as": "offerings"
        }
    }
}

/// 从已符合资格的供给中汇总去重供应商与可供区域。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回保持既有字段与资格条件的聚合阶段。
///
/// # 错误
/// 无。
fn sellable_supply_summary() -> Document {
    doc! {
        "$set": {
            "supplier_ids": { "$setUnion": ["$offerings.supplier_id", []] },
            "supply_regions": {
                "$reduce": {
                    "input": "$offerings.supply_region",
                    "initialValue": [],
                    "in": { "$setUnion": ["$$value", "$$this"] }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_catalog::entity::catalog::ProductKind;
    use erp_core::common::time::BusinessDate;
    use erp_core::money::Amount;

    use super::*;

    /// 公司商品池管道同时约束 SKU 已上架、当前修订与有效供给，并禁止投影采购成本。
    #[test]
    fn sellable_sku_pipeline_is_fail_closed_and_cost_safe() {
        let date = BusinessDate::from_ymd(2026, 8, 8).unwrap();
        let filter = SellableSkuFilter {
            nationwide_only: false,
            keyword: Some("礼盒".to_string()),
            product_kind: Some(ProductKind::Physical),
            category_id: Some("cat-1".to_string()),
            brand_id: Some("brand-1".to_string()),
            supplier_id: Some("supplier-1".to_string()),
            supply_region: Some("全国".to_string()),
            max_supplier_count: Some(1),
            sales_price_min: Some(Amount::from_str("10.00").unwrap()),
            sales_price_max: Some(Amount::from_str("100.00").unwrap()),
            eligibility_as_of: date,
            page: 2,
            page_size: 20,
        };
        let match_doc = sellable_sku_match(None, None, &[]);
        let pipeline = sellable_sku_pipeline(match_doc, &filter, Some((2, 20)));
        let json = format!("{pipeline:?}");

        assert!(json.contains("sales_visible_price_gross"));
        assert!(json.contains("listing_status"));
        assert!(json.contains("listed"));
        assert!(json.contains("availability_status"));
        assert!(json.contains("available_quantity"));
        assert!(json.contains("$gt"));
        assert!(json.contains("AVAILABLE"));
        assert!(json.contains("product_revision"));
        assert!(json.contains("specification_signature"));
        assert!(json.contains("supplier_count"));
        assert!(json.contains("category_id"));
        assert!(json.contains("brand_id"));
        assert!(json.contains("supplier_ids"));
        assert!(json.contains("supply_regions"));
        assert!(json.contains("$size"));
        assert!(json.contains("barcode"));
        assert!(json.contains("eligibility") || json.contains("valid_from"));
        assert!(!json.contains("dropship_supply_price"));
        assert!(!json.contains("bulk_supply_price"));
        assert!(!json.contains("input_tax_rate"));
    }

    /// 销售单资格复核按稳定 SKU 与当前修订成对匹配，禁止只按 SKU 猜修订。
    #[test]
    fn sellable_sku_reference_filter_matches_exact_pair() {
        let refs = vec![("sku-1".to_string(), "sku-rev-2".to_string())];
        let filter = sellable_sku_match(None, None, &refs);
        let serialized = format!("{filter:?}");

        assert!(serialized.contains("sku-1"));
        assert!(serialized.contains("sku-rev-2"));
        assert!(serialized.contains("current_revision_id"));
    }
}
