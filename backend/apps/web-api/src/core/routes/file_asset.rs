//! 域 D05 `file_asset` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/file-assets`、
//! `/admin/documents/{id}/attachments`、`/admin/document-attachments`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。
//!
use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::file_asset, middleware::with_permission},
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
            "/file-assets",
            with_permission(
                get(file_asset::file_asset_list),
                rbac,
                file_asset::file_asset_list_permission_key(),
            ),
        )
        .route(
            "/file-assets/upload",
            with_permission(
                post(file_asset::file_asset_upload),
                rbac,
                file_asset::file_asset_upload_permission_key(),
            ),
        )
        .route(
            "/file-assets/register",
            with_permission(
                post(file_asset::file_asset_register),
                rbac,
                file_asset::file_asset_register_permission_key(),
            ),
        )
        .route(
            "/file-assets/{id}",
            with_permission(
                get(file_asset::file_asset_detail),
                rbac,
                file_asset::file_asset_detail_permission_key(),
            ),
        )
        .route(
            "/file-assets/{id}/scan-result",
            with_permission(
                put(file_asset::file_asset_scan_result),
                rbac,
                file_asset::file_asset_scan_result_permission_key(),
            ),
        )
        .route(
            "/file-assets/{id}/destroy",
            with_permission(
                post(file_asset::file_asset_destroy),
                rbac,
                file_asset::file_asset_destroy_permission_key(),
            ),
        )
        .route(
            "/document-attachments",
            with_permission(
                post(file_asset::document_attachment_create),
                rbac,
                file_asset::document_attachment_create_permission_key(),
            ),
        )
        .route(
            "/documents/{id}/attachments",
            with_permission(
                get(file_asset::document_attachment_list),
                rbac,
                file_asset::document_attachment_list_permission_key(),
            ),
        )
}
