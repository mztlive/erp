//! 当前精确 SKU、商品修订及公共轮播主图的共享只读关联。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_catalog::CatalogExt;
use mongodb::Database;
use mongodb::bson::{Document, doc};

/// 沿两个稳定身份的当前指针取数，拒绝不属于该身份的修订。
pub(super) fn current_catalog_facts() -> Vec<Document> {
    let mut pipeline =
        vec![doc! {"$match":{"sku.status":"active","sku.deleted_at":NOT_DELETED_TIMESTAMP_BSON}}];
    pipeline.extend(join(<Database as CatalogExt>::PRODUCTS, "sku.product_id", "product"));
    pipeline
        .push(doc! {"$match":{"product.status":"active","product.deleted_at":NOT_DELETED_TIMESTAMP_BSON}});
    pipeline.extend(join(<Database as CatalogExt>::SKU_REVISIONS, "sku.current_revision_id", "revision"));
    pipeline.push(doc! {"$match":{"revision.status":"active","revision.deleted_at":NOT_DELETED_TIMESTAMP_BSON,"$expr":{"$eq":["$revision.sku_id","$sku.id"]}}});
    pipeline.extend(join(
        <Database as CatalogExt>::PRODUCT_REVISIONS,
        "product.current_revision_id",
        "product_revision",
    ));
    pipeline.push(doc! {"$match":{"product_revision.status":"active","product_revision.deleted_at":NOT_DELETED_TIMESTAMP_BSON,"$expr":{"$eq":["$product_revision.product_id","$product.id"]}}});
    pipeline.extend(join(<Database as CatalogExt>::UNIT_OF_MEASURES, "sku.base_unit_id", "unit"));
    pipeline.push(doc! {"$match":{"unit.status":"active","unit.deleted_at":NOT_DELETED_TIMESTAMP_BSON}});
    pipeline.push(current_product_image());
    pipeline
}

/// 使用正式领域集合常量取得一对一展示事实。
pub(super) fn join(collection: &str, local: &str, alias: &str) -> Vec<Document> {
    vec![
        doc! {"$lookup":{"from":collection,"localField":local,"foreignField":"id","as":alias}},
        doc! {"$unwind":format!("${alias}")},
    ]
}

/// 公共图片仅来自精确当前商品修订的首张轮播图，附件和历史修订不参与。
fn current_product_image() -> Document {
    doc! {"$lookup":{"from":<Database as CatalogExt>::PRODUCT_REVISION_MEDIAS,"let":{"revision_id":"$product_revision.id"},"pipeline":[{"$match":{"media_role":"carousel","deleted_at":NOT_DELETED_TIMESTAMP_BSON,"$expr":{"$eq":["$product_revision_id","$$revision_id"]}}},{"$sort":{"sort_order":1,"id":1}},{"$limit":1},{"$project":{"_id":0,"file_asset_id":1}}],"as":"product_image"}}
}

/// 目录和图片来源解析共用相同的允许列表，冻结两个身份及当前修订版本。
pub(super) fn catalog_projection() -> Document {
    doc! {"$project":{"_id":0,"id":"$sku.id","sku_no":"$sku.sku_no","name":"$revision.name","specification_signature":"$sku.specification_signature","unit_id":"$unit.id","unit_name":"$unit.name","unit_precision":"$unit.quantity_scale","image_asset_id":"$revision.source_main_image_asset_id","product_image_asset_id":{"$ifNull":[{"$arrayElemAt":["$product_image.file_asset_id",0]},null]},"product_kind":"$product.product_kind","version":"$sku.version","product_id":"$product.id","listing_status":{"$ifNull":["$sku.listing_status","listed"]},"own_offering_id":{"$ifNull":[{"$arrayElemAt":["$own_offering.id",0]},null]},"target_version":{"sku_version":"$sku.version","sku_revision_id":"$revision.id","sku_revision_version":"$revision.version","product_id":"$product.id","product_version":"$product.version","product_revision_id":"$product_revision.id","product_revision_version":"$product_revision.version","unit_id":"$unit.id","unit_version":"$unit.version"}}}
}
