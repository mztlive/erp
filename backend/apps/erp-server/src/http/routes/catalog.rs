//! 域 D10 `catalog` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/product-categories`、
//! `/admin/product-brands`、`/admin/unit-of-measures`、`/admin/sku-attributes`、
//! `/admin/sku-attribute-values`、`/admin/products`、`/admin/product-revisions`、
//! `/admin/skus`、`/admin/sku-revisions`、`/admin/voucher-category-profiles`（只读列表）、
//! `/admin/voucher-categories`（原子创建）、`/admin/voucher-categories/{sku_id}`（更新）；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_permissions as permissions;

use crate::http::handler::catalog;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
use crate::http::upload;

/// 返回商品目录管理路由，按资源合并各操作入口。
/// 返回逐路由绑定权限的完整管理端路由集合。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(product_category_routes())
        .merge(product_brand_collection_routes())
        .merge(product_brand_item_routes())
        .merge(unit_of_measure_routes())
        .merge(sku_attribute_routes())
        .merge(sku_attribute_value_routes())
        .merge(product_collection_routes())
        .merge(product_import_upload_routes())
        .merge(product_import_job_routes())
        .merge(product_item_routes())
        .merge(product_source_diff_routes())
        .merge(product_handover_routes())
        .merge(product_revision_query_routes())
        .merge(product_asset_routes())
        .merge(product_status_routes())
        .merge(product_revision_routes())
        .merge(sku_routes())
        .merge(sellable_sku_routes())
        .merge(sku_revision_routes())
        .merge(voucher_category_routes())
}

// 为父路由挂载商品分类管理路由。
fn product_category_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/product-categories",
            permission_route(
                get(catalog::product_category_list),
                &permissions::catalog::PRODUCT_CATEGORY_LIST,
            ),
        )
        .route(
            "/product-categories",
            permission_route(
                post(catalog::product_category_create),
                &permissions::catalog::PRODUCT_CATEGORY_CREATE,
            ),
        )
        .route(
            "/product-categories/{id}",
            permission_route(
                put(catalog::product_category_update),
                &permissions::catalog::PRODUCT_CATEGORY_UPDATE,
            ),
        )
        .route(
            "/product-categories/{id}/parent",
            permission_route(
                put(catalog::product_category_move),
                &permissions::catalog::PRODUCT_CATEGORY_UPDATE,
            ),
        )
        .route(
            "/product-categories/{id}",
            permission_route(
                delete(catalog::product_category_delete),
                &permissions::catalog::PRODUCT_CATEGORY_DELETE,
            ),
        )
}

// 为父路由挂载品牌列表与创建管理路由。
// 对 multipart 方法保留独立的准入及请求体上限。
fn product_brand_collection_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/product-brands",
            permission_route(get(catalog::product_brand_list), &permissions::catalog::PRODUCT_BRAND_LIST),
        )
        .route(
            "/product-brands",
            permission_route(
                post(catalog::product_brand_create),
                &permissions::catalog::PRODUCT_BRAND_CREATE,
            ),
        )
        .route(
            "/product-brands/import/jobs",
            permission_route(
                post(catalog::brand_import::submit),
                &permissions::catalog::PRODUCT_BRAND_CREATE,
            ),
        )
        .route(
            "/product-brands/import/jobs/{id}/failures",
            permission_route(
                get(catalog::brand_import::failures),
                &permissions::catalog::PRODUCT_BRAND_CREATE,
            ),
        )
        .route(
            "/product-brands/with-assets",
            permission_route(
                upload::multipart_route(
                    post(catalog::product_brand_create_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::catalog::PRODUCT_BRAND_CREATE,
            ),
        )
}

// 为父路由挂载品牌更新与删除管理路由。
// 对 multipart 方法保留独立的准入及请求体上限。
fn product_brand_item_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/product-brands/{id}",
            permission_route(put(catalog::product_brand_update), &permissions::catalog::PRODUCT_BRAND_UPDATE),
        )
        .route(
            "/product-brands/{id}/with-assets",
            permission_route(
                upload::multipart_route(
                    put(catalog::product_brand_update_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::catalog::PRODUCT_BRAND_UPDATE,
            ),
        )
        .route(
            "/product-brands/{id}",
            permission_route(
                delete(catalog::product_brand_delete),
                &permissions::catalog::PRODUCT_BRAND_DELETE,
            ),
        )
}

// 为父路由挂载计量单位管理路由。
fn unit_of_measure_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/unit-of-measures",
            permission_route(get(catalog::unit_of_measure_list), &permissions::catalog::UNIT_OF_MEASURE_LIST),
        )
        .route(
            "/unit-of-measures",
            permission_route(
                post(catalog::unit_of_measure_create),
                &permissions::catalog::UNIT_OF_MEASURE_CREATE,
            ),
        )
        .route(
            "/unit-of-measures/{id}",
            permission_route(
                put(catalog::unit_of_measure_update),
                &permissions::catalog::UNIT_OF_MEASURE_UPDATE,
            ),
        )
        .route(
            "/unit-of-measures/{id}",
            permission_route(
                delete(catalog::unit_of_measure_delete),
                &permissions::catalog::UNIT_OF_MEASURE_DELETE,
            ),
        )
}

// 为父路由挂载规格属性管理路由。
fn sku_attribute_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sku-attributes",
            permission_route(get(catalog::sku_attribute_list), &permissions::catalog::SKU_ATTRIBUTE_LIST),
        )
        .route(
            "/sku-attributes",
            permission_route(
                post(catalog::sku_attribute_create),
                &permissions::catalog::SKU_ATTRIBUTE_CREATE,
            ),
        )
        .route(
            "/sku-attributes/{id}",
            permission_route(put(catalog::sku_attribute_update), &permissions::catalog::SKU_ATTRIBUTE_UPDATE),
        )
        .route(
            "/sku-attributes/{id}",
            permission_route(
                delete(catalog::sku_attribute_delete),
                &permissions::catalog::SKU_ATTRIBUTE_DELETE,
            ),
        )
}

// 为父路由挂载规格属性值管理路由。
fn sku_attribute_value_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sku-attribute-values",
            permission_route(
                get(catalog::sku_attribute_value_list),
                &permissions::catalog::SKU_ATTRIBUTE_VALUE_LIST,
            ),
        )
        .route(
            "/sku-attribute-values",
            permission_route(
                post(catalog::sku_attribute_value_create),
                &permissions::catalog::SKU_ATTRIBUTE_VALUE_CREATE,
            ),
        )
        .route(
            "/sku-attribute-values/{id}",
            permission_route(
                put(catalog::sku_attribute_value_update),
                &permissions::catalog::SKU_ATTRIBUTE_VALUE_UPDATE,
            ),
        )
        .route(
            "/sku-attribute-values/{id}",
            permission_route(
                delete(catalog::sku_attribute_value_delete),
                &permissions::catalog::SKU_ATTRIBUTE_VALUE_DELETE,
            ),
        )
}

// 为父路由挂载商品列表与创建管理路由。
// 对 multipart 方法保留独立的准入及请求体上限。
fn product_collection_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products",
            permission_route(
                get(catalog::product::product_list),
                &permissions::catalog::PRODUCT_PRODUCT_LIST,
            ),
        )
        .route(
            "/products",
            permission_route(
                post(catalog::product::product_create),
                &permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
        .route(
            "/products/with-assets",
            permission_route(
                upload::multipart_route(
                    post(catalog::product::product_create_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
}

// 为父路由挂载商品导入上传管理路由。
// 对 multipart 方法保留独立的准入及请求体上限。
fn product_import_upload_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products/import",
            permission_route(
                upload::multipart_route(
                    post(catalog::import::product_import_submit),
                    upload::MAX_PRODUCT_IMPORT_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
        .route(
            "/products/import-uploads",
            permission_route(
                post(catalog::import::product_import_direct_upload_init),
                &permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}/parts/{part_number}",
            permission_route(
                get(catalog::import::product_import_direct_upload_part_url),
                &permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}/complete",
            permission_route(
                post(catalog::import::product_import_direct_upload_complete),
                &permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}",
            permission_route(
                delete(catalog::import::product_import_direct_upload_abort),
                &permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
}

// 为父路由挂载商品导入任务管理路由。
fn product_import_job_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products/import-jobs",
            permission_route(
                get(catalog::import::product_import_job_list),
                &permissions::catalog::PRODUCT_PRODUCT_LIST,
            ),
        )
        .route(
            "/products/import-jobs/{id}",
            permission_route(
                get(catalog::import::product_import_job_detail),
                &permissions::catalog::PRODUCT_PRODUCT_LIST,
            ),
        )
        .route(
            "/products/import-jobs/{id}/items",
            permission_route(
                get(catalog::import::product_import_job_items),
                &permissions::catalog::PRODUCT_PRODUCT_LIST,
            ),
        )
}

// 为父路由挂载商品详情与更新管理路由。
fn product_item_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}",
            permission_route(
                put(catalog::product::product_update),
                &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
        .route(
            "/products/{id}",
            permission_route(
                get(catalog::product::product_detail),
                &permissions::catalog::PRODUCT_PRODUCT_DETAIL,
            ),
        )
}

// 为父路由挂载商品维护责任交接管理路由。
fn product_handover_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/handover",
            permission_route(
                post(catalog::product::product_handover),
                &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
        .route(
            "/products/{id}/handover-candidates",
            permission_route(
                get(catalog::product::product_handover_candidates),
                &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
}

// 为父路由挂载商品与 SKU 修订查询管理路由。
fn product_revision_query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/revisions",
            permission_route(
                get(catalog::product::product_detail_revisions),
                &permissions::catalog::PRODUCT_PRODUCT_DETAIL,
            ),
        )
        .route(
            "/products/{id}/skus",
            permission_route(
                get(catalog::product::product_detail_skus),
                &permissions::catalog::PRODUCT_PRODUCT_DETAIL,
            ),
        )
        .route(
            "/products/{id}/sku-revisions",
            permission_route(
                get(catalog::product::product_detail_sku_revisions),
                &permissions::catalog::PRODUCT_PRODUCT_DETAIL,
            ),
        )
}

// 为父路由挂载商品与媒体原子更新管理路由。
// 对 multipart 方法保留独立的准入及请求体上限。
fn product_asset_routes() -> Router<HttpState> {
    Router::new().route(
        "/products/{id}/with-assets",
        permission_route(
            upload::multipart_route(
                put(catalog::product::product_update_with_assets),
                upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
            ),
            &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
        ),
    )
}

// 为父路由挂载商品状态管理路由。
fn product_status_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/disable",
            permission_route(
                put(catalog::product::product_disable),
                &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
        .route(
            "/products/{id}/listing-status",
            permission_route(
                put(catalog::product::product_listing_update),
                &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
}

// 为父路由挂载商品修订列表管理路由。
fn product_revision_routes() -> Router<HttpState> {
    Router::new().route(
        "/product-revisions",
        permission_route(
            get(catalog::product::product_revision_list),
            &permissions::catalog::PRODUCT_PRODUCT_REVISION_LIST,
        ),
    )
}

// 为父路由挂载SKU 列表与状态管理路由。
fn sku_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/skus",
            permission_route(get(catalog::product::sku_list), &permissions::catalog::PRODUCT_SKU_LIST),
        )
        .route(
            "/skus/{id}/listing-status",
            permission_route(
                put(catalog::product::sku_listing_update),
                &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
}

// 为父路由挂载可售 SKU管理路由。
fn sellable_sku_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sellable-skus",
            permission_route(
                get(catalog::product::sellable_sku_list),
                &permissions::catalog::PRODUCT_SELLABLE_SKU_LIST,
            ),
        )
        .route(
            "/sellable-skus/delivery-coverage",
            permission_route(
                post(catalog::delivery_coverage::delivery_coverage),
                &permissions::catalog::PRODUCT_SELLABLE_SKU_LIST,
            ),
        )
}

// 为父路由挂载SKU 修订管理路由。
fn sku_revision_routes() -> Router<HttpState> {
    Router::new().route(
        "/sku-revisions",
        permission_route(
            get(catalog::product::sku_revision_list),
            &permissions::catalog::PRODUCT_SKU_REVISION_LIST,
        ),
    )
}

// 为父路由挂载卡券类目管理路由。
fn voucher_category_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/voucher-category-profiles",
            permission_route(
                get(catalog::product::voucher_category_profile_list),
                &permissions::catalog::PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST,
            ),
        )
        .route(
            "/voucher-categories",
            permission_route(
                post(catalog::product::voucher_category_create),
                &permissions::catalog::PRODUCT_VOUCHER_CATEGORY_CREATE,
            ),
        )
        .route(
            "/voucher-categories/{sku_id}",
            permission_route(
                put(catalog::product::voucher_category_update),
                &permissions::catalog::PRODUCT_VOUCHER_CATEGORY_UPDATE,
            ),
        )
}

// 商品来源资料读取与确认路由，分别复用商品详情和维护权限。
// 两条具有独立动作权限的管理路由。
fn product_source_diff_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/source-diff",
            permission_route(
                get(catalog::source_diff::product_source_diff),
                &permissions::catalog::PRODUCT_PRODUCT_DETAIL,
            ),
        )
        .route(
            "/products/{id}/source-diff/resolve",
            permission_route(
                post(catalog::source_diff::resolve_product_source_diff),
                &permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
}
