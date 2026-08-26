//! 域 D11 `warehouse` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/warehouses`、
//! `/admin/warehouse-revisions`、`/admin/warehouse-sku-policies`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::warehouse, middleware::with_permission},
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
            "/warehouses",
            with_permission(
                get(warehouse::warehouse_list),
                rbac,
                warehouse::warehouse_list_permission_key(),
            ),
        )
        .route(
            "/warehouses",
            with_permission(
                post(warehouse::warehouse_create),
                rbac,
                warehouse::warehouse_create_permission_key(),
            ),
        )
        .route(
            "/warehouses/{id}",
            with_permission(
                put(warehouse::warehouse_update),
                rbac,
                warehouse::warehouse_update_permission_key(),
            ),
        )
        .route(
            "/warehouses/{id}/fulfillment-handlers",
            with_permission(
                put(warehouse::warehouse_fulfillment_handlers_update),
                rbac,
                warehouse::warehouse_fulfillment_handlers_update_permission_key(),
            ),
        )
        .route(
            "/warehouse-fulfillment-handler-options",
            with_permission(
                get(warehouse::warehouse_fulfillment_handler_options),
                rbac,
                warehouse::warehouse_fulfillment_handler_options_permission_key(),
            ),
        )
        .route(
            "/warehouse-revisions",
            with_permission(
                get(warehouse::warehouse_revision_list),
                rbac,
                warehouse::warehouse_revision_list_permission_key(),
            ),
        )
        .route(
            "/warehouse-sku-policies",
            with_permission(
                get(warehouse::warehouse_sku_policy_list),
                rbac,
                warehouse::warehouse_sku_policy_list_permission_key(),
            ),
        )
        .route(
            "/warehouse-sku-policies",
            with_permission(
                post(warehouse::warehouse_sku_policy_create),
                rbac,
                warehouse::warehouse_sku_policy_create_permission_key(),
            ),
        )
        .route(
            "/warehouse-sku-policies/{id}",
            with_permission(
                put(warehouse::warehouse_sku_policy_update),
                rbac,
                warehouse::warehouse_sku_policy_update_permission_key(),
            ),
        )
        .route(
            "/warehouse-sku-policies/{id}",
            with_permission(
                delete(warehouse::warehouse_sku_policy_delete),
                rbac,
                warehouse::warehouse_sku_policy_delete_permission_key(),
            ),
        )
}
