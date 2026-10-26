//! 域 D25 `supplier_api` 管理端路由。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::supplier_api;
use crate::core::middleware::with_permission;

/// 返回本域管理端路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    connection_routes(rbac)
        .merge(governance_routes(rbac))
        .merge(dangaoshushu_routes(rbac))
        .merge(catalog_routes(rbac))
        .merge(adoption_routes(rbac))
}

fn adoption_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/preview",
            with_permission(
                post(supplier_api::adoptions::preview),
                rbac,
                supplier_api::adoptions::preview_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/{preview}/confirm",
            with_permission(
                post(supplier_api::adoptions::confirm),
                rbac,
                supplier_api::adoptions::confirm_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/catalog/adoption-targets/{sku}",
            with_permission(
                get(supplier_api::adoptions::target),
                rbac,
                supplier_api::adoptions::target_permission_key(),
            ),
        )
}

fn dangaoshushu_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/reference-options",
            with_permission(
                get(supplier_api::references::options),
                rbac,
                supplier_api::references::options_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/dangaoshushu/catalog",
            with_permission(
                get(supplier_api::dangaoshushu::read),
                rbac,
                supplier_api::dangaoshushu::read_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/dangaoshushu/reference-tickets",
            with_permission(
                post(supplier_api::dangaoshushu::reference_tickets),
                rbac,
                supplier_api::dangaoshushu::reference_tickets_permission_key(),
            ),
        )
}

fn connection_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connector-types",
            with_permission(
                get(supplier_api::connector_types::options),
                rbac,
                supplier_api::connector_types::options_permission_key(),
            ),
        )
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
}

fn governance_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
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

fn catalog_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/catalog/{item}/identity-review",
            with_permission(
                get(supplier_api::catalog::identity_review),
                rbac,
                supplier_api::catalog::identity_review_permission_key(),
            )
            .merge(with_permission(
                post(supplier_api::catalog::confirm_identity_review),
                rbac,
                supplier_api::catalog::confirm_identity_review_permission_key(),
            )),
        )
        .route(
            "/supplier-api-connections/{id}/catalog/{item}/match",
            with_permission(
                post(supplier_api::catalog::match_item),
                rbac,
                supplier_api::catalog::match_item_permission_key(),
            ),
        )
        .route(
            "/supplier-api-connections/{id}/catalog",
            with_permission(
                get(supplier_api::catalog::list),
                rbac,
                supplier_api::catalog::list_permission_key(),
            ),
        )
}
