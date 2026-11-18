//! 域 D11 `warehouse` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/warehouses`、
//! `/admin/warehouse-revisions`、`/admin/warehouse-sku-policies`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_permissions as permissions;

use crate::http::handler::warehouse;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载仓库档案与 SKU 策略接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new().merge(warehouse_routes()).merge(fulfillment_routes()).merge(sku_policy_routes())
}

// 为父路由挂载仓库目录、档案查询与维护路由。
fn warehouse_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/warehouse-directory",
            permission_route(get(warehouse::directory::list), &permissions::warehouse::LIST),
        )
        .route(
            "/warehouse-directory/selected",
            permission_route(get(warehouse::directory::selected), &permissions::warehouse::LIST),
        )
        .route("/warehouses", permission_route(get(warehouse::warehouse_list), &permissions::warehouse::LIST))
        .route(
            "/warehouses",
            permission_route(post(warehouse::warehouse_create), &permissions::warehouse::CREATE),
        )
        .route(
            "/warehouses/{id}",
            permission_route(put(warehouse::warehouse_update), &permissions::warehouse::UPDATE),
        )
}

// 为父路由挂载仓库履约人选与修订读取路由。
fn fulfillment_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/warehouses/{id}/fulfillment-handlers",
            permission_route(
                put(warehouse::warehouse_fulfillment_handlers_update),
                &permissions::warehouse::UPDATE,
            ),
        )
        .route(
            "/warehouse-fulfillment-handler-options",
            permission_route(
                get(warehouse::warehouse_fulfillment_handler_options),
                &permissions::warehouse::UPDATE,
            ),
        )
        .route(
            "/warehouse-revisions",
            permission_route(get(warehouse::warehouse_revision_list), &permissions::warehouse::REVISION_LIST),
        )
}

// 为父路由挂载仓库规格政策路由。
fn sku_policy_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/warehouse-sku-policies",
            permission_route(
                get(warehouse::warehouse_sku_policy_list),
                &permissions::warehouse::SKU_POLICY_LIST,
            ),
        )
        .route(
            "/warehouse-sku-policies",
            permission_route(
                post(warehouse::warehouse_sku_policy_create),
                &permissions::warehouse::SKU_POLICY_CREATE,
            ),
        )
        .route(
            "/warehouse-sku-policies/{id}",
            permission_route(
                put(warehouse::warehouse_sku_policy_update),
                &permissions::warehouse::SKU_POLICY_UPDATE,
            ),
        )
        .route(
            "/warehouse-sku-policies/{id}",
            permission_route(
                delete(warehouse::warehouse_sku_policy_delete),
                &permissions::warehouse::SKU_POLICY_DELETE,
            ),
        )
}
