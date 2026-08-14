//! 域 D25 `supplier_api` 管理端路由。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier_api, middleware::with_permission},
};

/// 返回本域管理端路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections",
            with_permission(
                get(supplier_api::supplier_api_connection_list),
                rbac,
                supplier_api::supplier_api_connection_list_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections",
            with_permission(
                post(supplier_api::supplier_api_connection_create),
                rbac,
                supplier_api::supplier_api_connection_create_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}",
            with_permission(
                get(supplier_api::supplier_api_connection_detail),
                rbac,
                supplier_api::supplier_api_connection_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/commands",
            with_permission(
                post(supplier_api::supplier_api_connection_command),
                rbac,
                supplier_api::supplier_api_connection_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/business-capability-confirmations",
            with_permission(
                post(supplier_api::supplier_api_business_capability_confirm),
                rbac,
                supplier_api::supplier_api_capability_list_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/capabilities",
            with_permission(
                put(supplier_api::supplier_api_capabilities_update),
                rbac,
                supplier_api::supplier_api_connection_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/jobs/{job_id}",
            with_permission(
                get(supplier_api::supplier_api_connection_job_detail),
                rbac,
                supplier_api::supplier_api_connection_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-api-capabilities",
            with_permission(
                get(supplier_api::supplier_api_capability_list),
                rbac,
                supplier_api::supplier_api_capability_list_permission_key(),
            ),
        )
}
