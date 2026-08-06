//! 域 D02 `document_registry` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/business-documents`、
//! `/admin/workflow-actions`、`/admin/documents/{id}/relations`、
//! `/admin/document-relations`、`/admin/document-participants`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::document_registry, middleware::with_permission},
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
            "/business-documents",
            with_permission(
                get(document_registry::business_document_list),
                rbac,
                document_registry::business_document_list_permission_key(),
            ),
        )
        .route(
            "/business-documents",
            with_permission(
                post(document_registry::business_document_register),
                rbac,
                document_registry::business_document_register_permission_key(),
            ),
        )
        .route(
            "/business-documents/{id}",
            with_permission(
                get(document_registry::business_document_detail),
                rbac,
                document_registry::business_document_detail_permission_key(),
            ),
        )
        .route(
            "/workflow-actions",
            with_permission(
                get(document_registry::workflow_action_list),
                rbac,
                document_registry::workflow_action_list_permission_key(),
            ),
        )
        .route(
            "/workflow-actions",
            with_permission(
                post(document_registry::workflow_action_create),
                rbac,
                document_registry::workflow_action_create_permission_key(),
            ),
        )
        .route(
            "/documents/{id}/relations",
            with_permission(
                get(document_registry::document_relation_list),
                rbac,
                document_registry::document_relation_list_permission_key(),
            ),
        )
        .route(
            "/document-relations",
            with_permission(
                post(document_registry::document_relation_create),
                rbac,
                document_registry::document_relation_create_permission_key(),
            ),
        )
        .route(
            "/document-participants",
            with_permission(
                get(document_registry::document_participant_list),
                rbac,
                document_registry::document_participant_list_permission_key(),
            ),
        )
        .route(
            "/document-participants",
            with_permission(
                post(document_registry::document_participant_create),
                rbac,
                document_registry::document_participant_create_permission_key(),
            ),
        )
}
