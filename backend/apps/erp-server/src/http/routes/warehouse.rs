//! 域 D11 `warehouse` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/warehouses`、
//! `/admin/warehouse-revisions`、`/admin/warehouse-sku-policies`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::SharedRbacService;

use crate::http::handler::warehouse;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/warehouse-directory",
            with_permission(get(warehouse::directory::list), rbac, permissions::warehouse::DIRECTORY_LIST),
        )
        .route(
            "/warehouse-directory/selected",
            with_permission(
                get(warehouse::directory::selected),
                rbac,
                permissions::warehouse::DIRECTORY_SELECTED,
            ),
        )
        .route(
            "/warehouses",
            with_permission(get(warehouse::warehouse_list), rbac, permissions::warehouse::LIST),
        )
        .route(
            "/warehouses",
            with_permission(post(warehouse::warehouse_create), rbac, permissions::warehouse::CREATE),
        )
        .route(
            "/warehouses/{id}",
            with_permission(put(warehouse::warehouse_update), rbac, permissions::warehouse::UPDATE),
        )
        .route(
            "/warehouses/{id}/fulfillment-handlers",
            with_permission(
                put(warehouse::warehouse_fulfillment_handlers_update),
                rbac,
                permissions::warehouse::FULFILLMENT_HANDLERS_UPDATE,
            ),
        )
        .route(
            "/warehouse-fulfillment-handler-options",
            with_permission(
                get(warehouse::warehouse_fulfillment_handler_options),
                rbac,
                permissions::warehouse::FULFILLMENT_HANDLER_OPTIONS,
            ),
        )
        .route(
            "/warehouse-revisions",
            with_permission(
                get(warehouse::warehouse_revision_list),
                rbac,
                permissions::warehouse::REVISION_LIST,
            ),
        )
        .route(
            "/warehouse-sku-policies",
            with_permission(
                get(warehouse::warehouse_sku_policy_list),
                rbac,
                permissions::warehouse::SKU_POLICY_LIST,
            ),
        )
        .route(
            "/warehouse-sku-policies",
            with_permission(
                post(warehouse::warehouse_sku_policy_create),
                rbac,
                permissions::warehouse::SKU_POLICY_CREATE,
            ),
        )
        .route(
            "/warehouse-sku-policies/{id}",
            with_permission(
                put(warehouse::warehouse_sku_policy_update),
                rbac,
                permissions::warehouse::SKU_POLICY_UPDATE,
            ),
        )
        .route(
            "/warehouse-sku-policies/{id}",
            with_permission(
                delete(warehouse::warehouse_sku_policy_delete),
                rbac,
                permissions::warehouse::SKU_POLICY_DELETE,
            ),
        )
}
