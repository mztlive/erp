//! 域 D25 `supplier_api` 管理端路由。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::supplier_api;
use crate::core::middleware::with_permission;
use crate::permissions;

/// 返回本域管理端路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    connection_routes(rbac)
        .merge(governance_routes(rbac))
        .merge(dangaoshushu_routes(rbac))
        .merge(jd_vop_routes(rbac))
        .merge(catalog_routes(rbac))
        .merge(adoption_routes(rbac))
        .merge(adoption_batch_routes(rbac))
}

fn jd_vop_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/jd-vop-tokens",
            supplier_api::jd_vop::authorize_route(with_permission(
                post(supplier_api::jd_vop::tokens::configure),
                rbac,
                permissions::supplier_api::JD_VOP_TOKENS_CONFIGURE,
            )),
        )
        .route(
            "/supplier-api-connections/{id}/jd-vop-authorization",
            supplier_api::jd_vop::status_route(with_permission(
                get(supplier_api::jd_vop::status),
                rbac,
                permissions::supplier_api::JD_VOP_STATUS,
            ))
            .merge(supplier_api::jd_vop::authorize_route(with_permission(
                post(supplier_api::jd_vop::authorize),
                rbac,
                permissions::supplier_api::JD_VOP_AUTHORIZE,
            ))),
        )
}

fn adoption_batch_routes(rbac: &SharedRbacService) -> Router<AppState> {
    adoption_batch_query_routes(rbac)
        .merge(adoption_batch_command_routes(rbac))
        .merge(adoption_preparation_routes(rbac))
}

/// 注册准备摘要、轻量选择及任务控制入口。
fn adoption_preparation_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/selection",
            with_permission(
                get(supplier_api::adoption_batches::selection),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_SELECTION,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/preparation",
            with_permission(
                get(supplier_api::adoption_batches::preparation),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_PREPARATION,
            )
            .merge(with_permission(
                post(supplier_api::adoption_batches::control_preparation),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_CONTROL_PREPARATION,
            )),
        )
}

fn adoption_batch_query_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/submission-result",
            with_permission(
                post(supplier_api::adoption_batches::submission_result),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_SUBMISSION_RESULT,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches",
            with_permission(
                post(supplier_api::adoption_batches::create),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_CREATE,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches",
            with_permission(
                get(supplier_api::adoption_batches::list),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_LIST,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}",
            with_permission(
                get(supplier_api::adoption_batches::detail),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_DETAIL,
            ),
        )
}

fn adoption_batch_command_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/work",
            with_permission(
                post(supplier_api::adoption_batches::control_work),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_CONTROL_WORK,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}",
            with_permission(
                put(supplier_api::adoption_batches::save),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_SAVE,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/preview",
            with_permission(
                post(supplier_api::adoption_batches::preview),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/submit",
            with_permission(
                post(supplier_api::adoption_batches::submit),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_SUBMIT,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/reconcile",
            with_permission(
                post(supplier_api::adoption_batches::reconcile),
                rbac,
                permissions::supplier_api::ADOPTION_BATCHES_RECONCILE,
            ),
        )
}

fn adoption_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/preview",
            with_permission(
                post(supplier_api::adoptions::preview),
                rbac,
                permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/{preview}/confirm",
            with_permission(
                post(supplier_api::adoptions::confirm),
                rbac,
                permissions::supplier_api::ADOPTIONS_CONFIRM,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/{preview}/resolve",
            with_permission(
                post(supplier_api::adoptions::resolve),
                rbac,
                permissions::supplier_api::ADOPTIONS_RESOLVE,
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
                permissions::supplier_api::REFERENCES_OPTIONS,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/dangaoshushu/catalog",
            with_permission(
                get(supplier_api::dangaoshushu::read),
                rbac,
                permissions::supplier_api::DANGAOSHUSHU_READ,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/dangaoshushu/reference-tickets",
            with_permission(
                post(supplier_api::dangaoshushu::reference_tickets),
                rbac,
                permissions::supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS,
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
                permissions::supplier_api::CONNECTOR_TYPES_OPTIONS,
            ),
        )
        .route(
            "/supplier-api-connections",
            with_permission(
                get(supplier_api::supplier_api_connection_list),
                rbac,
                permissions::supplier_api::CONNECTION_LIST,
            ),
        )
        .route(
            "/supplier-api-connections",
            with_permission(
                post(supplier_api::supplier_api_connection_create),
                rbac,
                permissions::supplier_api::CONNECTION_CREATE,
            ),
        )
        .route(
            "/supplier-api-connections/{id}",
            with_permission(
                get(supplier_api::supplier_api_connection_detail),
                rbac,
                permissions::supplier_api::CONNECTION_DETAIL,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/commands",
            with_permission(
                post(supplier_api::supplier_api_connection_command),
                rbac,
                permissions::supplier_api::CONNECTION_DETAIL
                    .documented_as(permissions::supplier_api::CONNECTION_COMMAND),
            ),
        )
}

fn governance_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/supplier-api-connections/{id}/jobs/{job_id}",
        with_permission(
            get(supplier_api::supplier_api_connection_job_detail),
            rbac,
            permissions::supplier_api::CONNECTION_JOB_DETAIL,
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
                permissions::supplier_api::CATALOG_IDENTITY_REVIEW,
            )
            .merge(with_permission(
                post(supplier_api::catalog::confirm_identity_review),
                rbac,
                permissions::supplier_api::CATALOG_CONFIRM_IDENTITY_REVIEW,
            )),
        )
        .route(
            "/supplier-api-connections/{id}/catalog",
            with_permission(get(supplier_api::catalog::list), rbac, permissions::supplier_api::CATALOG_LIST),
        )
}
