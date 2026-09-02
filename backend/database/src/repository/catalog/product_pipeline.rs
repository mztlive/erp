use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};

use entities::catalog::{EnableStatus, ProductListingStatus, SkuCoverageStatus};
use entities::money::Amount;
use entities::supplier_offering::OfferingStatus;

use super::super::{Pagination, QueryFilter};
use super::listing::sku_is_listed_expr;
use super::shared::{sort_doc, PRODUCT_REVISIONS, SKUS, SKU_REVISIONS, SUPPLIER_OFFERINGS};
use super::ProductFilter;

/// 构造商品列表的当前修订与 SKU 聚合管道。
pub(super) fn product_list_pipeline(filter: &ProductFilter) -> Vec<Document> {
    let not_deleted = NOT_DELETED_TIMESTAMP_BSON;
    let mut pipeline = vec![
        doc! { "$match": filter.to_doc() },
        product_revision_lookup(not_deleted),
        doc! { "$unwind": { "path": "$product_revision", "preserveNullAndEmptyArrays": true } },
    ];
    append_product_revision_filters(&mut pipeline, filter);
    pipeline.extend([
        product_sku_lookup(not_deleted),
        doc! {
            "$set": {
                "sku_count": { "$size": "$skus" },
                "listed_sku_count": {
                    "$size": {
                        "$filter": {
                            "input": "$skus",
                            "as": "sku",
                            "cond": "$$sku.is_listed",
                        }
                    }
                },
                "supplied_sku_count": {
                    "$size": {
                        "$filter": {
                            "input": "$skus",
                            "as": "sku",
                            "cond": "$$sku.is_supplied",
                        }
                    }
                },
                "priced_sku_count": {
                    "$size": {
                        "$filter": {
                            "input": "$skus",
                            "as": "sku",
                            "cond": "$$sku.is_priced",
                        }
                    }
                },
            }
        },
        doc! {
            "$set": {
                "listing_status": {
                    "$switch": {
                        "branches": [
                            {
                                "case": {
                                    "$and": [
                                        { "$gt": ["$sku_count", 0] },
                                        { "$eq": ["$listed_sku_count", "$sku_count"] },
                                    ]
                                },
                                "then": ProductListingStatus::Listed.as_str(),
                            },
                            {
                                "case": { "$gt": ["$listed_sku_count", 0] },
                                "then": ProductListingStatus::PartiallyListed.as_str(),
                            },
                        ],
                        "default": ProductListingStatus::Unlisted.as_str(),
                    }
                }
            }
        },
    ]);
    append_product_aggregate_filters(&mut pipeline, filter);
    pipeline.push(product_facet(filter));
    pipeline
}

/// 构造商品当前修订关联，列表只读取稳定商品指向的当前版本。
fn product_revision_lookup(not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": PRODUCT_REVISIONS,
            "let": { "revision_id": "$current_revision_id" },
            "pipeline": [{
                "$match": {
                    "$expr": { "$eq": ["$id", "$$revision_id"] },
                    "deleted_at": not_deleted,
                }
            }],
            "as": "product_revision",
        }
    }
}

/// 构造当前启用 SKU、当前 SKU 修订与有效供给关系的批量关联。
fn product_sku_lookup(not_deleted: i64) -> Document {
    doc! {
        "$lookup": {
            "from": SKUS,
            "let": { "product_id": "$id" },
            "pipeline": [
                {
                    "$match": {
                        "$expr": { "$eq": ["$product_id", "$$product_id"] },
                        "deleted_at": not_deleted,
                        "status": EnableStatus::Active.as_str(),
                    }
                },
                {
                    "$lookup": {
                        "from": SKU_REVISIONS,
                        "let": { "revision_id": "$current_revision_id" },
                        "pipeline": [{
                            "$match": {
                                "$expr": { "$eq": ["$id", "$$revision_id"] },
                                "deleted_at": not_deleted,
                            }
                        }],
                        "as": "revision",
                    }
                },
                { "$unwind": { "path": "$revision", "preserveNullAndEmptyArrays": true } },
                {
                    "$lookup": {
                        "from": SUPPLIER_OFFERINGS,
                        "let": { "sku_id": "$id" },
                        "pipeline": [{
                            "$match": {
                                "$expr": { "$eq": ["$sku_id", "$$sku_id"] },
                                "deleted_at": not_deleted,
                                "status": OfferingStatus::Active.as_str(),
                                "current_revision_id": { "$ne": null },
                            }
                        }, {
                            "$project": { "_id": 0, "supplier_id": 1 }
                        }],
                        "as": "offerings",
                    }
                },
                {
                    "$project": {
                        "_id": 0,
                        "sku_no": 1,
                        "specification_signature": 1,
                        "revision": 1,
                        "sales_price": "$revision.sales_visible_price_gross",
                        "supplier_ids": { "$setUnion": ["$offerings.supplier_id", []] },
                        "is_listed": sku_is_listed_expr(),
                        "is_supplied": { "$gt": [{ "$size": "$offerings" }, 0] },
                        "is_priced": {
                            "$ne": [
                                { "$ifNull": ["$revision.sales_visible_price_gross", null] },
                                null,
                            ]
                        },
                    }
                },
            ],
            "as": "skus",
        }
    }
}

/// 追加只依赖当前商品修订的分类与品牌筛选。
fn append_product_revision_filters(pipeline: &mut Vec<Document>, filter: &ProductFilter) {
    if let Some(category_id) = &filter.category_id {
        pipeline.push(doc! { "$match": { "product_revision.category_id": category_id } });
    }
    if let Some(brand_id) = &filter.brand_id {
        pipeline.push(doc! { "$match": { "product_revision.brand_id": brand_id } });
    }
}

/// 追加统一关键字与 SKU 聚合状态筛选。
fn append_product_aggregate_filters(pipeline: &mut Vec<Document>, filter: &ProductFilter) {
    if let Some(keyword) = filter.keyword.as_deref() {
        let pattern = regex::escape(keyword);
        pipeline.push(doc! {
            "$match": {
                "$or": [
                    { "product_no": { "$regex": &pattern, "$options": "i" } },
                    { "product_revision.name": { "$regex": &pattern, "$options": "i" } },
                    { "skus.sku_no": { "$regex": &pattern, "$options": "i" } },
                    { "skus.specification_signature": { "$regex": &pattern, "$options": "i" } },
                    { "skus.revision.name": { "$regex": &pattern, "$options": "i" } },
                    { "skus.revision.specification": { "$regex": &pattern, "$options": "i" } },
                    { "skus.revision.barcode": { "$regex": &pattern, "$options": "i" } },
                ]
            }
        });
    }
    if let Some(status) = filter.listing_status {
        pipeline.push(doc! { "$match": { "listing_status": status.as_str() } });
    }
    if let Some(supplier_id) = &filter.supplier_id {
        pipeline.push(doc! { "$match": { "skus.supplier_ids": supplier_id } });
    }
    append_coverage_filter(pipeline, "supplied_sku_count", filter.supply_coverage);
    append_sales_price_filter(pipeline, filter.sales_price_min, filter.sales_price_max);
}

/// 追加销售价闭区间筛选；商品下至少一个当前启用 SKU 的销售价必须落入区间。
fn append_sales_price_filter(pipeline: &mut Vec<Document>, minimum: Option<Amount>, maximum: Option<Amount>) {
    if minimum.is_none() && maximum.is_none() {
        return;
    }
    let mut range = Document::new();
    if let Some(minimum) = minimum {
        range.insert("$gte", amount_filter_bson(minimum));
    }
    if let Some(maximum) = maximum {
        range.insert("$lte", amount_filter_bson(maximum));
    }
    pipeline.push(doc! {
        "$match": {
            "skus": {
                "$elemMatch": { "sales_price": range }
            }
        }
    });
}

/// 把查询金额转换为与库内价格一致的 BSON Decimal128。
fn amount_filter_bson(amount: Amount) -> Bson {
    Bson::Decimal128(
        amount
            .to_string()
            .parse()
            .expect("合法 Amount 必须可转换为 MongoDB Decimal128"),
    )
}

/// 追加完整、部分或无覆盖筛选；空 SKU 集合只属于无覆盖。
fn append_coverage_filter(
    pipeline: &mut Vec<Document>,
    count_field: &str,
    coverage: Option<SkuCoverageStatus>,
) {
    let count_ref = format!("${count_field}");
    let expression = match coverage {
        Some(SkuCoverageStatus::Complete) => doc! {
            "$and": [
                { "$gt": ["$sku_count", 0] },
                { "$eq": [count_ref, "$sku_count"] },
            ]
        },
        Some(SkuCoverageStatus::Partial) => doc! {
            "$and": [
                { "$gt": [&count_ref, 0] },
                { "$lt": [count_ref, "$sku_count"] },
            ]
        },
        Some(SkuCoverageStatus::None) => doc! { "$eq": [count_ref, 0] },
        None => return,
    };
    pipeline.push(doc! { "$match": { "$expr": expression } });
}

/// 构造商品列表分页、排序与投影 facet。
fn product_facet(filter: &ProductFilter) -> Document {
    let item_stages = vec![
        doc! { "$sort": product_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending) },
        doc! { "$skip": filter.skip() as i64 },
        doc! { "$limit": filter.limit() },
        doc! {
            "$project": {
                "_id": 0,
                "id": 1,
                "product_no": 1,
                "product_kind": 1,
                "name": "$product_revision.name",
                "category_id": "$product_revision.category_id",
                "brand_id": "$product_revision.brand_id",
                "status": 1,
                "listing_status": 1,
                "listed_sku_count": 1,
                "sku_count": 1,
                "supplied_sku_count": 1,
                "priced_sku_count": 1,
                "current_revision_id": 1,
                "version": 1,
                "created_at": 1,
            }
        },
    ];
    doc! {
        "$facet": {
            "items": item_stages,
            "total": [{ "$count": "count" }],
        }
    }
}

/// 构建商品排序文档（白名单：`created_at`/`product_no`）。
fn product_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("product_no") => "product_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::catalog::{EnableStatus, ProductKind, ProductListingStatus, SkuCoverageStatus};
    use entities::money::Amount;
    use std::str::FromStr;

    /// 商品列表统一搜索必须覆盖商品与 SKU 字段，并在分页前应用聚合筛选。
    #[test]
    fn product_list_pipeline_applies_keyword_and_sku_coverage_filters() {
        let filter = ProductFilter {
            product_no: None,
            keyword: Some("礼盒.*".to_string()),
            product_kind: Some(ProductKind::Physical),
            category_id: Some("category-1".to_string()),
            brand_id: Some("brand-1".to_string()),
            supplier_id: Some("supplier-1".to_string()),
            status: Some(EnableStatus::Active),
            listing_status: Some(ProductListingStatus::PartiallyListed),
            supply_coverage: Some(SkuCoverageStatus::Complete),
            sales_price_min: Some(Amount::from_str("100.00").unwrap()),
            sales_price_max: Some(Amount::from_str("200.00").unwrap()),
            page: 2,
            page_size: 20,
            sort_by: Some("product_no".to_string()),
            sort_ascending: true,
        };
        let pipeline = product_list_pipeline(&filter);
        let json = format!("{pipeline:?}");

        assert!(json.contains("product_revision.name"));
        assert!(json.contains("skus.sku_no"));
        assert!(json.contains("skus.revision.barcode"));
        assert!(json.contains("supplied_sku_count"));
        assert!(json.contains("priced_sku_count"));
        assert!(json.contains("sales_price"));
        assert!(json.contains("$gte"));
        assert!(json.contains("$lte"));
        let Bson::Decimal128(minimum) = amount_filter_bson(Amount::from_str("100.00").unwrap()) else {
            panic!("销售价筛选值必须使用 Decimal128");
        };
        assert_eq!(minimum.to_string(), "100.00");
        assert!(json.contains("partially_listed"));
        assert!(json.contains("$ifNull"));
        assert!(json.contains("listing_status"));
        assert!(json.contains("category-1"));
        assert!(json.contains("brand-1"));
        assert!(json.contains("supplier_ids"));
        assert!(json.contains("supplier-1"));
        assert!(json.contains("礼盒"));
        assert!(!json.contains("礼盒.*"));
        assert!(json.contains("$facet"));
    }
}
