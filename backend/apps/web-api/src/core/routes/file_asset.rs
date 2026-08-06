//! 域 D05 `file_asset` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/file-assets`、
//! `/admin/documents/{id}/attachments`、`/admin/document-attachments`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。
//!
//! 上传路由（multipart）需要 `upload::WriteLock` 扩展（与 `core/upload` 同款
//! 进程内串行锁），在本路由集合内单独注入，不触碰冻结的 `routes/mod.rs`。

use axum::{
    routing::{get, post, put},
    Extension, Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::file_asset, middleware::with_permission, upload},
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
        // 上传 handler 依赖进程内串行写锁（与 `core/upload` 同款），仅对本域
        // 路由注入；全局 /upload 已由冻结的 routes/mod.rs 注入同一形态扩展。
        .layer(Extension(upload::write_lock()))
}
