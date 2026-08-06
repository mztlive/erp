//! 域 D24 `supplier_catalog` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-catalog/*`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），
//! handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier_catalog, middleware::with_permission},
};

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-catalog/products",
            with_permission(
                get(supplier_catalog::supplier_catalog_product_list),
                rbac,
                supplier_catalog::supplier_catalog_product_list_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/products",
            with_permission(
                post(supplier_catalog::supplier_catalog_product_create),
                rbac,
                supplier_catalog::supplier_catalog_product_create_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/products/{id}",
            with_permission(
                get(supplier_catalog::supplier_catalog_product_detail),
                rbac,
                supplier_catalog::supplier_catalog_product_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/products/{id}/revisions",
            with_permission(
                post(supplier_catalog::supplier_catalog_product_revise),
                rbac,
                supplier_catalog::supplier_catalog_product_revise_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/skus",
            with_permission(
                get(supplier_catalog::supplier_catalog_sku_list),
                rbac,
                supplier_catalog::supplier_catalog_sku_list_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/mappings",
            with_permission(
                get(supplier_catalog::supplier_product_mapping_list),
                rbac,
                supplier_catalog::supplier_product_mapping_list_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/mappings",
            with_permission(
                post(supplier_catalog::supplier_product_mapping_create),
                rbac,
                supplier_catalog::supplier_product_mapping_create_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/mappings/{id}/approve",
            with_permission(
                post(supplier_catalog::supplier_product_mapping_approve),
                rbac,
                supplier_catalog::supplier_product_mapping_approve_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/offerings",
            with_permission(
                get(supplier_catalog::supplier_offering_list),
                rbac,
                supplier_catalog::supplier_offering_list_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/offerings/{id}/revisions",
            with_permission(
                post(supplier_catalog::supplier_offering_revise),
                rbac,
                supplier_catalog::supplier_offering_revise_permission_key(),
            ),
        )
        .route(
            "/supplier-catalog/intake-batches",
            with_permission(
                get(supplier_catalog::supplier_catalog_intake_batch_list),
                rbac,
                supplier_catalog::supplier_catalog_intake_batch_list_permission_key(),
            ),
        )
}
