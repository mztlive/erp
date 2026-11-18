//! 域 D02 `document_registry` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/business-documents`、
//! `/admin/workflow-actions`、`/admin/documents/{id}/relations`、
//! `/admin/document-relations`、`/admin/document-participants`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::document_registry;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载业务单据登记与关系接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(document_routes())
        .merge(workflow_routes())
        .merge(relation_routes())
        .merge(participant_routes())
}

// 为父路由挂载业务单据登记路由。
fn document_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/business-documents",
            permission_route(
                get(document_registry::business_document_list),
                &permissions::document_registry::BUSINESS_DOCUMENT_LIST,
            ),
        )
        .route(
            "/business-documents",
            permission_route(
                post(document_registry::business_document_register),
                &permissions::document_registry::BUSINESS_DOCUMENT_REGISTER,
            ),
        )
        .route(
            "/business-documents/{id}",
            permission_route(
                get(document_registry::business_document_detail),
                &permissions::document_registry::BUSINESS_DOCUMENT_DETAIL,
            ),
        )
}

// 为父路由挂载流程动作登记路由。
fn workflow_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/workflow-actions",
            permission_route(
                get(document_registry::workflow_action_list),
                &permissions::document_registry::WORKFLOW_ACTION_LIST,
            ),
        )
        .route(
            "/workflow-actions",
            permission_route(
                post(document_registry::workflow_action_create),
                &permissions::document_registry::WORKFLOW_ACTION_CREATE,
            ),
        )
}

// 为父路由挂载业务单据关系路由。
fn relation_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/documents/{id}/relations",
            permission_route(
                get(document_registry::document_relation_list),
                &permissions::document_registry::DOCUMENT_RELATION_LIST,
            ),
        )
        .route(
            "/document-relations",
            permission_route(
                post(document_registry::document_relation_create),
                &permissions::document_registry::DOCUMENT_RELATION_CREATE,
            ),
        )
}

// 为父路由挂载业务单据参与人路由。
fn participant_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/document-participants",
            permission_route(
                get(document_registry::document_participant_list),
                &permissions::document_registry::DOCUMENT_PARTICIPANT_LIST,
            ),
        )
        .route(
            "/document-participants",
            permission_route(
                post(document_registry::document_participant_create),
                &permissions::document_registry::DOCUMENT_PARTICIPANT_CREATE,
            ),
        )
}
