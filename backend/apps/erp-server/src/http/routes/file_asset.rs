//! 域 D05 `file_asset` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/file-assets`、
//! `/admin/documents/{id}/attachments`、`/admin/document-attachments`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。
//!
use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::http::handler::file_asset;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::http::upload;
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
            "/file-assets",
            with_permission(get(file_asset::file_asset_list), rbac, permissions::file_asset::LIST),
        )
        .route(
            "/file-assets/upload",
            with_permission(
                upload::multipart_route(
                    post(file_asset::file_asset_upload),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::file_asset::UPLOAD,
            ),
        )
        .route(
            "/file-assets/register",
            with_permission(post(file_asset::file_asset_register), rbac, permissions::file_asset::REGISTER),
        )
        .route(
            "/file-assets/{id}",
            with_permission(get(file_asset::file_asset_detail), rbac, permissions::file_asset::DETAIL),
        )
        .route(
            "/file-assets/{id}/preview",
            with_permission(get(file_asset::file_asset_preview), rbac, permissions::file_asset::PREVIEW),
        )
        .route(
            "/file-assets/{id}/scan-result",
            with_permission(
                put(file_asset::file_asset_scan_result),
                rbac,
                permissions::file_asset::SCAN_RESULT,
            ),
        )
        .route(
            "/file-assets/{id}/destroy",
            with_permission(post(file_asset::file_asset_destroy), rbac, permissions::file_asset::DESTROY),
        )
        .route(
            "/document-attachments",
            with_permission(
                post(file_asset::document_attachment_create),
                rbac,
                permissions::file_asset::DOCUMENT_ATTACHMENT_CREATE,
            ),
        )
        .route(
            "/documents/{id}/attachments",
            with_permission(
                get(file_asset::document_attachment_list),
                rbac,
                permissions::file_asset::DOCUMENT_ATTACHMENT_LIST,
            ),
        )
}
