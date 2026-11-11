//! 域 D10 `catalog` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/product-categories`、
//! `/admin/product-brands`、`/admin/unit-of-measures`、`/admin/sku-attributes`、
//! `/admin/sku-attribute-values`、`/admin/products`、`/admin/product-revisions`、
//! `/admin/skus`、`/admin/sku-revisions`、`/admin/voucher-category-profiles`（只读列表）、
//! `/admin/voucher-categories`（原子创建）、`/admin/voucher-categories/{sku_id}`（更新）；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::catalog;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::http::upload;
use crate::permissions;

/// 返回商品目录管理路由，按资源合并各操作入口。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐路由绑定权限的完整管理端路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .merge(product_category_routes(rbac))
        .merge(product_brand_collection_routes(rbac))
        .merge(product_brand_item_routes(rbac))
        .merge(unit_of_measure_routes(rbac))
        .merge(sku_attribute_routes(rbac))
        .merge(sku_attribute_value_routes(rbac))
        .merge(product_collection_routes(rbac))
        .merge(product_import_upload_routes(rbac))
        .merge(product_import_job_routes(rbac))
        .merge(product_item_routes(rbac))
        .merge(product_source_diff_routes(rbac))
        .merge(product_handover_routes(rbac))
        .merge(product_revision_query_routes(rbac))
        .merge(product_asset_routes(rbac))
        .merge(product_status_routes(rbac))
        .merge(product_revision_routes(rbac))
        .merge(sku_routes(rbac))
        .merge(sellable_sku_routes(rbac))
        .merge(sku_revision_routes(rbac))
        .merge(voucher_category_routes(rbac))
}

/// 商品分类管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_category_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/product-categories",
            with_permission(
                get(catalog::product_category_list),
                rbac,
                permissions::catalog::PRODUCT_CATEGORY_LIST,
            ),
        )
        .route(
            "/product-categories",
            with_permission(
                post(catalog::product_category_create),
                rbac,
                permissions::catalog::PRODUCT_CATEGORY_CREATE,
            ),
        )
        .route(
            "/product-categories/{id}",
            with_permission(
                put(catalog::product_category_update),
                rbac,
                permissions::catalog::PRODUCT_CATEGORY_UPDATE,
            ),
        )
        .route(
            "/product-categories/{id}/parent",
            with_permission(
                put(catalog::product_category_move),
                rbac,
                permissions::catalog::PRODUCT_CATEGORY_MOVE,
            ),
        )
        .route(
            "/product-categories/{id}",
            with_permission(
                delete(catalog::product_category_delete),
                rbac,
                permissions::catalog::PRODUCT_CATEGORY_DELETE,
            ),
        )
}

/// 品牌列表与创建管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_brand_collection_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/product-brands",
            with_permission(get(catalog::product_brand_list), rbac, permissions::catalog::PRODUCT_BRAND_LIST),
        )
        .route(
            "/product-brands",
            with_permission(
                post(catalog::product_brand_create),
                rbac,
                permissions::catalog::PRODUCT_BRAND_CREATE,
            ),
        )
        .route(
            "/product-brands/import/jobs",
            with_permission(
                post(catalog::brand_import::submit),
                rbac,
                permissions::catalog::PRODUCT_BRAND_IMPORT,
            ),
        )
        .route(
            "/product-brands/import/jobs/{id}/failures",
            with_permission(
                get(catalog::brand_import::failures),
                rbac,
                permissions::catalog::PRODUCT_BRAND_IMPORT_FAILURES,
            ),
        )
        .route(
            "/product-brands/with-assets",
            with_permission(
                upload::multipart_route(
                    post(catalog::product_brand_create_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::catalog::PRODUCT_BRAND_CREATE_WITH_ASSETS,
            ),
        )
}

/// 品牌更新与删除管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_brand_item_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/product-brands/{id}",
            with_permission(
                put(catalog::product_brand_update),
                rbac,
                permissions::catalog::PRODUCT_BRAND_UPDATE,
            ),
        )
        .route(
            "/product-brands/{id}/with-assets",
            with_permission(
                upload::multipart_route(
                    put(catalog::product_brand_update_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::catalog::PRODUCT_BRAND_UPDATE_WITH_ASSETS,
            ),
        )
        .route(
            "/product-brands/{id}",
            with_permission(
                delete(catalog::product_brand_delete),
                rbac,
                permissions::catalog::PRODUCT_BRAND_DELETE,
            ),
        )
}

/// 计量单位管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn unit_of_measure_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/unit-of-measures",
            with_permission(
                get(catalog::unit_of_measure_list),
                rbac,
                permissions::catalog::UNIT_OF_MEASURE_LIST,
            ),
        )
        .route(
            "/unit-of-measures",
            with_permission(
                post(catalog::unit_of_measure_create),
                rbac,
                permissions::catalog::UNIT_OF_MEASURE_CREATE,
            ),
        )
        .route(
            "/unit-of-measures/{id}",
            with_permission(
                put(catalog::unit_of_measure_update),
                rbac,
                permissions::catalog::UNIT_OF_MEASURE_UPDATE,
            ),
        )
        .route(
            "/unit-of-measures/{id}",
            with_permission(
                delete(catalog::unit_of_measure_delete),
                rbac,
                permissions::catalog::UNIT_OF_MEASURE_DELETE,
            ),
        )
}

/// 规格属性管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn sku_attribute_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sku-attributes",
            with_permission(get(catalog::sku_attribute_list), rbac, permissions::catalog::SKU_ATTRIBUTE_LIST),
        )
        .route(
            "/sku-attributes",
            with_permission(
                post(catalog::sku_attribute_create),
                rbac,
                permissions::catalog::SKU_ATTRIBUTE_CREATE,
            ),
        )
        .route(
            "/sku-attributes/{id}",
            with_permission(
                put(catalog::sku_attribute_update),
                rbac,
                permissions::catalog::SKU_ATTRIBUTE_UPDATE,
            ),
        )
        .route(
            "/sku-attributes/{id}",
            with_permission(
                delete(catalog::sku_attribute_delete),
                rbac,
                permissions::catalog::SKU_ATTRIBUTE_DELETE,
            ),
        )
}

/// 规格属性值管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn sku_attribute_value_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sku-attribute-values",
            with_permission(
                get(catalog::sku_attribute_value_list),
                rbac,
                permissions::catalog::SKU_ATTRIBUTE_VALUE_LIST,
            ),
        )
        .route(
            "/sku-attribute-values",
            with_permission(
                post(catalog::sku_attribute_value_create),
                rbac,
                permissions::catalog::SKU_ATTRIBUTE_VALUE_CREATE,
            ),
        )
        .route(
            "/sku-attribute-values/{id}",
            with_permission(
                put(catalog::sku_attribute_value_update),
                rbac,
                permissions::catalog::SKU_ATTRIBUTE_VALUE_UPDATE,
            ),
        )
        .route(
            "/sku-attribute-values/{id}",
            with_permission(
                delete(catalog::sku_attribute_value_delete),
                rbac,
                permissions::catalog::SKU_ATTRIBUTE_VALUE_DELETE,
            ),
        )
}

/// 商品列表与创建管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_collection_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products",
            with_permission(
                get(catalog::product::product_list),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_LIST,
            ),
        )
        .route(
            "/products",
            with_permission(
                post(catalog::product::product_create),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_CREATE,
            ),
        )
        .route(
            "/products/with-assets",
            with_permission(
                upload::multipart_route(
                    post(catalog::product::product_create_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_CREATE_WITH_ASSETS,
            ),
        )
}

/// 商品导入上传管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_import_upload_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products/import",
            with_permission(
                upload::multipart_route(
                    post(catalog::import::product_import_submit),
                    upload::MAX_PRODUCT_IMPORT_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_SUBMIT,
            ),
        )
        .route(
            "/products/import-uploads",
            with_permission(
                post(catalog::import::product_import_direct_upload_init),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_INIT,
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}/parts/{part_number}",
            with_permission(
                get(catalog::import::product_import_direct_upload_part_url),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_PART_URL,
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}/complete",
            with_permission(
                post(catalog::import::product_import_direct_upload_complete),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_COMPLETE,
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}",
            with_permission(
                delete(catalog::import::product_import_direct_upload_abort),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_DIRECT_UPLOAD_ABORT,
            ),
        )
}

/// 商品导入任务管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_import_job_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products/import-jobs",
            with_permission(
                get(catalog::import::product_import_job_list),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_JOB_LIST,
            ),
        )
        .route(
            "/products/import-jobs/{id}",
            with_permission(
                get(catalog::import::product_import_job_detail),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_JOB_DETAIL,
            ),
        )
        .route(
            "/products/import-jobs/{id}/items",
            with_permission(
                get(catalog::import::product_import_job_items),
                rbac,
                permissions::catalog::IMPORT_PRODUCT_IMPORT_JOB_ITEMS,
            ),
        )
}

/// 商品详情与更新管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_item_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}",
            with_permission(
                put(catalog::product::product_update),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_UPDATE,
            ),
        )
        .route(
            "/products/{id}",
            with_permission(
                get(catalog::product::product_detail),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_DETAIL,
            ),
        )
}

/// 商品维护责任交接管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_handover_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/handover",
            with_permission(
                post(catalog::product::product_handover),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_HANDOVER,
            ),
        )
        .route(
            "/products/{id}/handover-candidates",
            with_permission(
                get(catalog::product::product_handover_candidates),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_HANDOVER_CANDIDATES,
            ),
        )
}

/// 商品与 SKU 修订查询管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_revision_query_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/revisions",
            with_permission(
                get(catalog::product::product_detail_revisions),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_DETAIL_REVISIONS,
            ),
        )
        .route(
            "/products/{id}/skus",
            with_permission(
                get(catalog::product::product_detail_skus),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_DETAIL_SKUS,
            ),
        )
        .route(
            "/products/{id}/sku-revisions",
            with_permission(
                get(catalog::product::product_detail_sku_revisions),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_DETAIL_SKU_REVISIONS,
            ),
        )
}

/// 商品与媒体原子更新管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_asset_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new().route(
        "/products/{id}/with-assets",
        with_permission(
            upload::multipart_route(
                put(catalog::product::product_update_with_assets),
                upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
            ),
            rbac,
            permissions::catalog::PRODUCT_PRODUCT_UPDATE_WITH_ASSETS,
        ),
    )
}

/// 商品状态管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_status_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/disable",
            with_permission(
                put(catalog::product::product_disable),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_DISABLE,
            ),
        )
        .route(
            "/products/{id}/listing-status",
            with_permission(
                put(catalog::product::product_listing_update),
                rbac,
                permissions::catalog::PRODUCT_PRODUCT_LISTING_UPDATE,
            ),
        )
}

/// 商品修订列表管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_revision_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new().route(
        "/product-revisions",
        with_permission(
            get(catalog::product::product_revision_list),
            rbac,
            permissions::catalog::PRODUCT_PRODUCT_REVISION_LIST,
        ),
    )
}

/// SKU 列表与状态管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn sku_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/skus",
            with_permission(get(catalog::product::sku_list), rbac, permissions::catalog::PRODUCT_SKU_LIST),
        )
        .route(
            "/skus/{id}/listing-status",
            with_permission(
                put(catalog::product::sku_listing_update),
                rbac,
                permissions::catalog::PRODUCT_SKU_LISTING_UPDATE,
            ),
        )
}

/// 可售 SKU管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn sellable_sku_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sellable-skus",
            with_permission(
                get(catalog::product::sellable_sku_list),
                rbac,
                permissions::catalog::PRODUCT_SELLABLE_SKU_LIST,
            ),
        )
        .route(
            "/sellable-skus/delivery-coverage",
            with_permission(
                post(catalog::delivery_coverage::delivery_coverage),
                rbac,
                permissions::catalog::DELIVERY_COVERAGE_DELIVERY_COVERAGE,
            ),
        )
}

/// SKU 修订管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn sku_revision_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new().route(
        "/sku-revisions",
        with_permission(
            get(catalog::product::sku_revision_list),
            rbac,
            permissions::catalog::PRODUCT_SKU_REVISION_LIST,
        ),
    )
}

/// 卡券类目管理路由。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐操作保留权限校验与上传限制的路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
fn voucher_category_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/voucher-category-profiles",
            with_permission(
                get(catalog::product::voucher_category_profile_list),
                rbac,
                permissions::catalog::PRODUCT_VOUCHER_CATEGORY_PROFILE_LIST,
            ),
        )
        .route(
            "/voucher-categories",
            with_permission(
                post(catalog::product::voucher_category_create),
                rbac,
                permissions::catalog::PRODUCT_VOUCHER_CATEGORY_CREATE,
            ),
        )
        .route(
            "/voucher-categories/{sku_id}",
            with_permission(
                put(catalog::product::voucher_category_update),
                rbac,
                permissions::catalog::PRODUCT_VOUCHER_CATEGORY_UPDATE,
            ),
        )
}

/// 商品来源资料读取与确认路由，分别复用商品详情和维护权限。
/// # 参数
/// `rbac` 为认证后的权限解析服务。
/// # 返回
/// 两条具有独立动作权限的管理路由。
/// # 错误
/// 无；业务错误由处理器返回。
fn product_source_diff_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/products/{id}/source-diff",
            with_permission(
                get(catalog::source_diff::product_source_diff),
                rbac,
                permissions::catalog::SOURCE_DIFF_PRODUCT_SOURCE_DIFF,
            ),
        )
        .route(
            "/products/{id}/source-diff/resolve",
            with_permission(
                post(catalog::source_diff::resolve_product_source_diff),
                rbac,
                permissions::catalog::SOURCE_DIFF_RESOLVE_PRODUCT_SOURCE_DIFF,
            ),
        )
}
