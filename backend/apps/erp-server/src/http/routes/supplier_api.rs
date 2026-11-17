//! 域 D25 `supplier_api` 管理端路由。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::supplier_api;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    connection_routes()
        .merge(governance_routes())
        .merge(dangaoshushu_routes())
        .merge(jd_vop_routes())
        .merge(catalog_routes())
        .merge(adoption_routes())
        .merge(adoption_batch_routes())
}

fn jd_vop_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/jd-vop-tokens",
            supplier_api::jd_vop::authorize_route(permission_route(
                post(supplier_api::jd_vop::tokens::configure),
                &permissions::supplier_api::JD_VOP_AUTHORIZE,
            )),
        )
        .route(
            "/supplier-api-connections/{id}/jd-vop-authorization",
            supplier_api::jd_vop::status_route(permission_route(
                get(supplier_api::jd_vop::status),
                &permissions::supplier_api::CONNECTION_DETAIL,
            ))
            .merge(supplier_api::jd_vop::authorize_route(permission_route(
                post(supplier_api::jd_vop::authorize),
                &permissions::supplier_api::JD_VOP_AUTHORIZE,
            ))),
        )
}

fn adoption_batch_routes() -> Router<HttpState> {
    adoption_batch_query_routes().merge(adoption_batch_command_routes()).merge(adoption_preparation_routes())
}

/// 注册准备摘要、轻量选择及任务控制入口。
fn adoption_preparation_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/selection",
            permission_route(
                get(supplier_api::adoption_batches::selection),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/preparation",
            permission_route(
                get(supplier_api::adoption_batches::preparation),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            )
            .merge(permission_route(
                post(supplier_api::adoption_batches::control_preparation),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            )),
        )
}

fn adoption_batch_query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/submission-result",
            permission_route(
                post(supplier_api::adoption_batches::submission_result),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches",
            permission_route(
                post(supplier_api::adoption_batches::create),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches",
            permission_route(
                get(supplier_api::adoption_batches::list),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}",
            permission_route(
                get(supplier_api::adoption_batches::detail),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
}

fn adoption_batch_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/work",
            permission_route(
                post(supplier_api::adoption_batches::control_work),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}",
            permission_route(
                put(supplier_api::adoption_batches::save),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/preview",
            permission_route(
                post(supplier_api::adoption_batches::preview),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/submit",
            permission_route(
                post(supplier_api::adoption_batches::submit),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/adoption-batches/{batch}/reconcile",
            permission_route(
                post(supplier_api::adoption_batches::reconcile),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
}

fn adoption_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/preview",
            permission_route(
                post(supplier_api::adoptions::preview),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/{preview}/confirm",
            permission_route(
                post(supplier_api::adoptions::confirm),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/catalog/adoptions/{preview}/resolve",
            permission_route(
                post(supplier_api::adoptions::resolve),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            ),
        )
}

fn dangaoshushu_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/reference-options",
            permission_route(
                get(supplier_api::references::options),
                &permissions::supplier_api::REFERENCES_OPTIONS,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/dangaoshushu/catalog",
            permission_route(
                get(supplier_api::dangaoshushu::read),
                &permissions::supplier_api::DANGAOSHUSHU_READ,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/dangaoshushu/reference-tickets",
            permission_route(
                post(supplier_api::dangaoshushu::reference_tickets),
                &permissions::supplier_api::DANGAOSHUSHU_REFERENCE_TICKETS,
            ),
        )
}

fn connection_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connector-types",
            permission_route(
                get(supplier_api::connector_types::options),
                &permissions::supplier_api::CONNECTION_CREATE,
            ),
        )
        .route(
            "/supplier-api-connections",
            permission_route(
                get(supplier_api::supplier_api_connection_list),
                &permissions::supplier_api::CONNECTION_LIST,
            ),
        )
        .route(
            "/supplier-api-connections",
            permission_route(
                post(supplier_api::supplier_api_connection_create),
                &permissions::supplier_api::CONNECTION_CREATE,
            ),
        )
        .route(
            "/supplier-api-connections/{id}",
            permission_route(
                get(supplier_api::supplier_api_connection_detail),
                &permissions::supplier_api::CONNECTION_DETAIL,
            ),
        )
        .route(
            "/supplier-api-connections/{id}/commands",
            permission_route(
                post(supplier_api::supplier_api_connection_command),
                &permissions::supplier_api::CONNECTION_DETAIL,
            ),
        )
}

fn governance_routes() -> Router<HttpState> {
    Router::new().route(
        "/supplier-api-connections/{id}/jobs/{job_id}",
        permission_route(
            get(supplier_api::supplier_api_connection_job_detail),
            &permissions::supplier_api::CONNECTION_DETAIL,
        ),
    )
}

fn catalog_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-api-connections/{id}/catalog/{item}/identity-review",
            permission_route(
                get(supplier_api::catalog::identity_review),
                &permissions::supplier_api::CONNECTION_DETAIL,
            )
            .merge(permission_route(
                post(supplier_api::catalog::confirm_identity_review),
                &permissions::supplier_api::ADOPTIONS_PREVIEW,
            )),
        )
        .route(
            "/supplier-api-connections/{id}/catalog",
            permission_route(get(supplier_api::catalog::list), &permissions::supplier_api::CONNECTION_DETAIL),
        )
}
