//! 域 D02 `document_registry` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/business-documents`、
//! `/admin/workflow-actions`、`/admin/documents/{id}/relations`、
//! `/admin/document-relations`、`/admin/document-participants`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::document_registry;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/business-documents",
            with_permission(
                get(document_registry::business_document_list),
                rbac,
                &permissions::document_registry::BUSINESS_DOCUMENT_LIST,
            ),
        )
        .route(
            "/business-documents",
            with_permission(
                post(document_registry::business_document_register),
                rbac,
                &permissions::document_registry::BUSINESS_DOCUMENT_REGISTER,
            ),
        )
        .route(
            "/business-documents/{id}",
            with_permission(
                get(document_registry::business_document_detail),
                rbac,
                &permissions::document_registry::BUSINESS_DOCUMENT_DETAIL,
            ),
        )
        .route(
            "/workflow-actions",
            with_permission(
                get(document_registry::workflow_action_list),
                rbac,
                &permissions::document_registry::WORKFLOW_ACTION_LIST,
            ),
        )
        .route(
            "/workflow-actions",
            with_permission(
                post(document_registry::workflow_action_create),
                rbac,
                &permissions::document_registry::WORKFLOW_ACTION_CREATE,
            ),
        )
        .route(
            "/documents/{id}/relations",
            with_permission(
                get(document_registry::document_relation_list),
                rbac,
                &permissions::document_registry::DOCUMENT_RELATION_LIST,
            ),
        )
        .route(
            "/document-relations",
            with_permission(
                post(document_registry::document_relation_create),
                rbac,
                &permissions::document_registry::DOCUMENT_RELATION_CREATE,
            ),
        )
        .route(
            "/document-participants",
            with_permission(
                get(document_registry::document_participant_list),
                rbac,
                &permissions::document_registry::DOCUMENT_PARTICIPANT_LIST,
            ),
        )
        .route(
            "/document-participants",
            with_permission(
                post(document_registry::document_participant_create),
                rbac,
                &permissions::document_registry::DOCUMENT_PARTICIPANT_CREATE,
            ),
        )
}
