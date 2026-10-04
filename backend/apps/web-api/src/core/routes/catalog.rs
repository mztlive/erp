//! 域 D10 `catalog` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/product-categories`、
//! `/admin/product-brands`、`/admin/unit-of-measures`、`/admin/sku-attributes`、
//! `/admin/sku-attribute-values`、`/admin/products`、`/admin/product-revisions`、
//! `/admin/skus`、`/admin/sku-revisions`、`/admin/voucher-category-profiles`（只读列表）、
//! `/admin/voucher-categories`（原子创建）、`/admin/voucher-categories/{sku_id}`（更新）；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::catalog;
use crate::core::middleware::with_permission;
use crate::core::upload;

/// 返回商品目录管理路由，按资源合并各操作入口。
///
/// # 参数
/// * `rbac` - 共享权限服务
/// # 返回
/// 返回逐路由绑定权限的完整管理端路由集合。
/// # 错误
/// 无；业务错误由处理器返回。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
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
fn product_category_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/product-categories",
            with_permission(
                get(catalog::product_category_list),
                rbac,
                catalog::product_category_list_permission_key(),
            ),
        )
        .route(
            "/product-categories",
            with_permission(
                post(catalog::product_category_create),
                rbac,
                catalog::product_category_create_permission_key(),
            ),
        )
        .route(
            "/product-categories/{id}",
            with_permission(
                put(catalog::product_category_update),
                rbac,
                catalog::product_category_update_permission_key(),
            ),
        )
        .route(
            "/product-categories/{id}/parent",
            with_permission(
                put(catalog::product_category_move),
                rbac,
                catalog::product_category_move_permission_key(),
            ),
        )
        .route(
            "/product-categories/{id}",
            with_permission(
                delete(catalog::product_category_delete),
                rbac,
                catalog::product_category_delete_permission_key(),
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
fn product_brand_collection_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/product-brands",
            with_permission(
                get(catalog::product_brand_list),
                rbac,
                catalog::product_brand_list_permission_key(),
            ),
        )
        .route(
            "/product-brands",
            with_permission(
                post(catalog::product_brand_create),
                rbac,
                catalog::product_brand_create_permission_key(),
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
                catalog::product_brand_create_with_assets_permission_key(),
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
fn product_brand_item_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/product-brands/{id}",
            with_permission(
                put(catalog::product_brand_update),
                rbac,
                catalog::product_brand_update_permission_key(),
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
                catalog::product_brand_update_with_assets_permission_key(),
            ),
        )
        .route(
            "/product-brands/{id}",
            with_permission(
                delete(catalog::product_brand_delete),
                rbac,
                catalog::product_brand_delete_permission_key(),
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
fn unit_of_measure_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/unit-of-measures",
            with_permission(
                get(catalog::unit_of_measure_list),
                rbac,
                catalog::unit_of_measure_list_permission_key(),
            ),
        )
        .route(
            "/unit-of-measures",
            with_permission(
                post(catalog::unit_of_measure_create),
                rbac,
                catalog::unit_of_measure_create_permission_key(),
            ),
        )
        .route(
            "/unit-of-measures/{id}",
            with_permission(
                put(catalog::unit_of_measure_update),
                rbac,
                catalog::unit_of_measure_update_permission_key(),
            ),
        )
        .route(
            "/unit-of-measures/{id}",
            with_permission(
                delete(catalog::unit_of_measure_delete),
                rbac,
                catalog::unit_of_measure_delete_permission_key(),
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
fn sku_attribute_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sku-attributes",
            with_permission(
                get(catalog::sku_attribute_list),
                rbac,
                catalog::sku_attribute_list_permission_key(),
            ),
        )
        .route(
            "/sku-attributes",
            with_permission(
                post(catalog::sku_attribute_create),
                rbac,
                catalog::sku_attribute_create_permission_key(),
            ),
        )
        .route(
            "/sku-attributes/{id}",
            with_permission(
                put(catalog::sku_attribute_update),
                rbac,
                catalog::sku_attribute_update_permission_key(),
            ),
        )
        .route(
            "/sku-attributes/{id}",
            with_permission(
                delete(catalog::sku_attribute_delete),
                rbac,
                catalog::sku_attribute_delete_permission_key(),
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
fn sku_attribute_value_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sku-attribute-values",
            with_permission(
                get(catalog::sku_attribute_value_list),
                rbac,
                catalog::sku_attribute_value_list_permission_key(),
            ),
        )
        .route(
            "/sku-attribute-values",
            with_permission(
                post(catalog::sku_attribute_value_create),
                rbac,
                catalog::sku_attribute_value_create_permission_key(),
            ),
        )
        .route(
            "/sku-attribute-values/{id}",
            with_permission(
                put(catalog::sku_attribute_value_update),
                rbac,
                catalog::sku_attribute_value_update_permission_key(),
            ),
        )
        .route(
            "/sku-attribute-values/{id}",
            with_permission(
                delete(catalog::sku_attribute_value_delete),
                rbac,
                catalog::sku_attribute_value_delete_permission_key(),
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
fn product_collection_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/products",
            with_permission(
                get(catalog::product::product_list),
                rbac,
                catalog::product::product_list_permission_key(),
            ),
        )
        .route(
            "/products",
            with_permission(
                post(catalog::product::product_create),
                rbac,
                catalog::product::product_create_permission_key(),
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
                catalog::product::product_create_with_assets_permission_key(),
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
fn product_import_upload_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/products/import",
            with_permission(
                upload::multipart_route(
                    post(catalog::import::product_import_submit),
                    upload::MAX_PRODUCT_IMPORT_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                catalog::import::product_import_submit_permission_key(),
            ),
        )
        .route(
            "/products/import-uploads",
            with_permission(
                post(catalog::import::product_import_direct_upload_init),
                rbac,
                catalog::import::product_import_direct_upload_init_permission_key(),
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}/parts/{part_number}",
            with_permission(
                get(catalog::import::product_import_direct_upload_part_url),
                rbac,
                catalog::import::product_import_direct_upload_part_url_permission_key(),
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}/complete",
            with_permission(
                post(catalog::import::product_import_direct_upload_complete),
                rbac,
                catalog::import::product_import_direct_upload_complete_permission_key(),
            ),
        )
        .route(
            "/products/import-uploads/{upload_id}",
            with_permission(
                delete(catalog::import::product_import_direct_upload_abort),
                rbac,
                catalog::import::product_import_direct_upload_abort_permission_key(),
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
fn product_import_job_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/products/import-jobs",
            with_permission(
                get(catalog::import::product_import_job_list),
                rbac,
                catalog::import::product_import_job_list_permission_key(),
            ),
        )
        .route(
            "/products/import-jobs/{id}",
            with_permission(
                get(catalog::import::product_import_job_detail),
                rbac,
                catalog::import::product_import_job_detail_permission_key(),
            ),
        )
        .route(
            "/products/import-jobs/{id}/items",
            with_permission(
                get(catalog::import::product_import_job_items),
                rbac,
                catalog::import::product_import_job_items_permission_key(),
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
fn product_item_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/products/{id}",
            with_permission(
                put(catalog::product::product_update),
                rbac,
                catalog::product::product_update_permission_key(),
            ),
        )
        .route(
            "/products/{id}",
            with_permission(
                get(catalog::product::product_detail),
                rbac,
                catalog::product::product_detail_permission_key(),
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
fn product_handover_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/products/{id}/handover",
            with_permission(
                post(catalog::product::product_handover),
                rbac,
                catalog::product::product_handover_permission_key(),
            ),
        )
        .route(
            "/products/{id}/handover-candidates",
            with_permission(
                get(catalog::product::product_handover_candidates),
                rbac,
                catalog::product::product_handover_candidates_permission_key(),
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
fn product_revision_query_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/products/{id}/revisions",
            with_permission(
                get(catalog::product::product_detail_revisions),
                rbac,
                catalog::product::product_detail_permission_key(),
            ),
        )
        .route(
            "/products/{id}/skus",
            with_permission(
                get(catalog::product::product_detail_skus),
                rbac,
                catalog::product::product_detail_permission_key(),
            ),
        )
        .route(
            "/products/{id}/sku-revisions",
            with_permission(
                get(catalog::product::product_detail_sku_revisions),
                rbac,
                catalog::product::product_detail_permission_key(),
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
fn product_asset_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/products/{id}/with-assets",
        with_permission(
            upload::multipart_route(
                put(catalog::product::product_update_with_assets),
                upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
            ),
            rbac,
            catalog::product::product_update_with_assets_permission_key(),
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
fn product_status_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/products/{id}/disable",
            with_permission(
                put(catalog::product::product_disable),
                rbac,
                catalog::product::product_disable_permission_key(),
            ),
        )
        .route(
            "/products/{id}/listing-status",
            with_permission(
                put(catalog::product::product_listing_update),
                rbac,
                catalog::product::product_listing_update_permission_key(),
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
fn product_revision_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/product-revisions",
        with_permission(
            get(catalog::product::product_revision_list),
            rbac,
            catalog::product::product_revision_list_permission_key(),
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
fn sku_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/skus",
            with_permission(
                get(catalog::product::sku_list),
                rbac,
                catalog::product::sku_list_permission_key(),
            ),
        )
        .route(
            "/skus/{id}/listing-status",
            with_permission(
                put(catalog::product::sku_listing_update),
                rbac,
                catalog::product::sku_listing_update_permission_key(),
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
fn sellable_sku_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/sellable-skus",
        with_permission(
            get(catalog::product::sellable_sku_list),
            rbac,
            catalog::product::sellable_sku_list_permission_key(),
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
fn sku_revision_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/sku-revisions",
        with_permission(
            get(catalog::product::sku_revision_list),
            rbac,
            catalog::product::sku_revision_list_permission_key(),
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
fn voucher_category_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/voucher-category-profiles",
            with_permission(
                get(catalog::product::voucher_category_profile_list),
                rbac,
                catalog::product::voucher_category_profile_list_permission_key(),
            ),
        )
        .route(
            "/voucher-categories",
            with_permission(
                post(catalog::product::voucher_category_create),
                rbac,
                catalog::product::voucher_category_create_permission_key(),
            ),
        )
        .route(
            "/voucher-categories/{sku_id}",
            with_permission(
                put(catalog::product::voucher_category_update),
                rbac,
                catalog::product::voucher_category_update_permission_key(),
            ),
        )
}
