use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Bson, Document};
use serde::{Deserialize, Serialize};

use entities::catalog::{EnableStatus, ListingStatus, ProductKind, Sku};
use entities::common::time::BusinessDate;
use entities::money::Amount;
use entities::supplier_offering::{AvailabilityStatus, OfferingStatus};

use super::super::extensions::{CatalogExt, SupplierOfferingExt};
use super::super::PageResult;
use super::shared::{PRODUCT_REVISIONS, SKUS, SKU_REVISIONS, SUPPLIER_OFFERINGS};
use super::CatalogRepository;
use crate::executor::Executor;
use crate::Result;

/// `supplier_offering_revision` 集合名（公司商品池资格依赖的当前供给修订）。
const SUPPLIER_OFFERING_REVISIONS: &str =
    <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS;
/// `supplier_offering_availability` 集合名（公司商品池资格依赖的实时可供投影）。
const SUPPLIER_OFFERING_AVAILABILITIES: &str =
    <mongodb::Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES;

/// 公司商品池只读查询行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SellableSkuRow {
    /// 稳定 SKU ID。
    pub sku_id: String,
    /// 稳定 SKU 乐观锁版本。
    pub sku_version: u64,
    /// 当前且符合资格的 SKU 修订 ID。
    pub sku_revision_id: String,
    /// 当前 SKU 修订号。
    pub sku_revision_no: u32,
    /// SKU 编码。
    pub sku_no: String,
    /// 所属稳定商品 ID。
    pub product_id: String,
    /// 商品编码。
    pub product_no: String,
    /// 商品业务类型。
    pub product_kind: ProductKind,
    /// 公司审核后的 SKU 名称。
    pub name: String,
    /// 稳定 SKU 的规范化规格属性签名。
    pub specification_signature: String,
    /// 公司审核后的规格文案。
    pub specification: Option<String>,
    /// 条码。
    pub barcode: Option<String>,
    /// 基础单位 ID。
    pub base_unit_id: String,
    /// 基础单位编码。
    pub base_unit_code: Option<String>,
    /// 基础单位名称。
    pub base_unit_name: Option<String>,
    /// 公司销售可见含税价。
    pub sales_visible_price_gross: Amount,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// SKU 主图文件 ID。
    pub main_image_asset_id: Option<String>,
    /// 当前 SKU 修订生效开始日。
    pub effective_from: BusinessDate,
    /// 当前 SKU 修订生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 当前有效供给对应的去重供应商数量。
    pub supplier_count: u32,
    /// 当前有效供给的可供区域并集。
    #[serde(default)]
    pub supply_regions: Vec<String>,
}

/// 公司商品池聚合分页结果。
#[derive(Debug, Deserialize)]
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

impl<'a> CatalogRepository<'a> {
    /// 分页查询销售可用的公司 SKU。
    ///
    /// 资格固定为：稳定 SKU 启用、当前 SKU 修订启用且处于生效区间并已配置
    /// 销售可见价、至少一条启用且当前修订可供并处于有效区间的供给。结果不含
    /// 供应商身份、采购成本、税率或起订量。
    ///
    /// # 参数
    /// * `keyword` - 可选名称、编码或条码关键字
    /// * `product_kind` - 可选公司商品类型
    /// * `eligibility_as_of` - 服务端解释的销售资格业务日期
    /// * `page` - 从一开始的页码
    /// * `page_size` - 单页条数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页资格投影及资格结果总数。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn search_sellable_skus(
        &self,
        keyword: Option<&str>,
        product_kind: Option<ProductKind>,
        eligibility_as_of: BusinessDate,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SellableSkuRow>> {
        let match_doc = sellable_sku_match(keyword, product_kind, &[]);
        let pipeline = sellable_sku_pipeline(match_doc, eligibility_as_of, Some((page, page_size)));
        let facet = self.aggregate_sellable_skus(pipeline, executor).await?;
        Ok(PageResult {
            items: facet.items,
            total: facet.total.first().map_or(0, |row| row.count),
        })
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
        let pipeline = sellable_sku_pipeline(match_doc, eligibility_as_of, None);
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
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<SellableSkuFacet>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows.into_iter().next().unwrap_or(SellableSkuFacet {
            items: Vec::new(),
            total: Vec::new(),
        }))
    }
}

fn sellable_sku_match(
    keyword: Option<&str>,
    product_kind: Option<ProductKind>,
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
    if let Some(kind) = product_kind {
        filter.insert("__product_kind", bson_for(kind));
    }
    if let Some(value) = keyword.map(str::trim).filter(|value| !value.is_empty()) {
        filter.insert("__keyword", regex::escape(value));
    }
    filter
}

/// 把可序列化枚举转换为 BSON 标量。
fn bson_for<T: Serialize>(value: T) -> Bson {
    mongodb::bson::to_bson(&value).expect("域枚举必须可序列化为 BSON")
}

/// 构造公司商品池资格与只读投影聚合管道。
fn sellable_sku_pipeline(
    mut initial_match: Document,
    eligibility_as_of: BusinessDate,
    paging: Option<(u64, u32)>,
) -> Vec<Document> {
    let product_kind = initial_match.remove("__product_kind");
    let keyword = initial_match.remove("__keyword");
    let as_of = eligibility_as_of.to_string();
    let not_deleted = NOT_DELETED_TIMESTAMP_BSON;
    let zero_quantity = Bson::Decimal128("0".parse().expect("零必须可转换为 MongoDB Decimal128"));
    let mut after_lookups = Vec::new();
    if let Some(kind) = product_kind {
        after_lookups.push(doc! { "$match": { "product.product_kind": kind } });
    }
    if let Some(Bson::String(pattern)) = keyword {
        after_lookups.push(doc! {
            "$match": {
                "$or": [
                    { "sku_no": { "$regex": &pattern, "$options": "i" } },
                    { "sku_revision.name": { "$regex": &pattern, "$options": "i" } },
                    { "product.product_no": { "$regex": &pattern, "$options": "i" } },
                ]
            }
        });
    }

    let (skip, limit) = paging.map_or((0_i64, i64::MAX), |(page, page_size)| {
        (
            ((page.saturating_sub(1)) * u64::from(page_size)) as i64,
            i64::from(page_size),
        )
    });
    let mut item_stages = vec![doc! { "$sort": { "sku_no": 1, "id": 1 } }, doc! { "$skip": skip }];
    if paging.is_some() {
        item_stages.push(doc! { "$limit": limit });
    }
    item_stages.push(doc! {
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
            "sales_visible_price_gross": "$sku_revision.sales_visible_price_gross",
            "market_price": "$sku_revision.market_price",
            "main_image_asset_id": "$sku_revision.source_main_image_asset_id",
            "effective_from": "$sku_revision.effective_from",
            "effective_to": "$sku_revision.effective_to",
            "supplier_count": { "$size": "$supplier_ids" },
            "supply_regions": 1,
        }
    });

    let mut pipeline = vec![
        doc! { "$match": initial_match },
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
                        "effective_from": { "$lte": &as_of },
                        "$or": [
                            { "effective_to": null },
                            { "effective_to": { "$gt": &as_of } },
                        ]
                    }
                }],
                "as": "sku_revision"
            }
        },
        doc! { "$unwind": "$sku_revision" },
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
        },
        doc! { "$unwind": "$product" },
        doc! {
            "$lookup": {
                "from": PRODUCT_REVISIONS,
                "let": { "revision_id": "$product.current_revision_id" },
                "pipeline": [{
                    "$match": {
                        "$expr": { "$eq": ["$id", "$$revision_id"] },
                        "deleted_at": not_deleted,
                        "status": EnableStatus::Active.as_str(),
                        "effective_from": { "$lte": &as_of },
                        "$or": [
                            { "effective_to": null },
                            { "effective_to": { "$gt": &as_of } },
                        ]
                    }
                }],
                "as": "product_revision"
            }
        },
        doc! { "$unwind": "$product_revision" },
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
        },
        doc! { "$unwind": { "path": "$unit", "preserveNullAndEmptyArrays": true } },
        doc! {
            "$lookup": {
                "from": SUPPLIER_OFFERINGS,
                "let": { "sku_id": "$id" },
                "pipeline": [
                    {
                        "$match": {
                            "$expr": { "$eq": ["$sku_id", "$$sku_id"] },
                            "deleted_at": not_deleted,
                            "status": OfferingStatus::Active.as_str(),
                            "current_revision_id": { "$ne": null },
                        }
                    },
                    {
                        "$lookup": {
                            "from": SUPPLIER_OFFERING_REVISIONS,
                            "let": { "revision_id": "$current_revision_id" },
                            "pipeline": [{
                                "$match": {
                                    "$expr": { "$eq": ["$id", "$$revision_id"] },
                                    "deleted_at": not_deleted,
                                    "valid_from": { "$lte": &as_of },
                                    "$or": [
                                        { "valid_to": null },
                                        { "valid_to": { "$gt": &as_of } },
                                    ]
                                }
                            }],
                            "as": "revision"
                        }
                    },
                    { "$unwind": "$revision" },
                    {
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
                    },
                    { "$unwind": "$availability" },
                    { "$project": { "_id": 0, "supplier_id": 1, "supply_region": "$revision.supply_region" } },
                ],
                "as": "offerings"
            }
        },
        doc! { "$match": { "offerings.0": { "$exists": true } } },
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
        },
    ];
    pipeline.extend(after_lookups);
    pipeline.push(doc! {
        "$facet": {
            "items": item_stages,
            "total": [{ "$count": "count" }],
        }
    });
    pipeline
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::catalog::ProductKind;
    use entities::common::time::BusinessDate;

    /// 公司商品池管道同时约束 SKU 已上架、当前修订与有效供给，并禁止投影采购成本。
    #[test]
    fn sellable_sku_pipeline_is_fail_closed_and_cost_safe() {
        let date = BusinessDate::from_ymd(2026, 8, 8).unwrap();
        let filter = sellable_sku_match(Some("礼盒"), Some(ProductKind::Physical), &[]);
        let pipeline = sellable_sku_pipeline(filter, date, Some((2, 20)));
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
