//! 域 D05 `file_asset` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/file-assets`、
//! `/admin/documents/{id}/attachments`、`/admin/document-attachments`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。
//!
use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::file_asset;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
use crate::http::upload;

/// 为管理端挂载文件资产接口，按操作登记公共权限目录信息；文件入口保留对应的上传限制。
pub fn routes() -> Router<HttpState> {
    Router::new().merge(asset_routes()).merge(lifecycle_routes()).merge(attachment_routes())
}

// 为父路由挂载文件登记、上传与读取路由。
fn asset_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/file-assets",
            permission_route(get(file_asset::file_asset_list), &permissions::file_asset::LIST),
        )
        .route(
            "/file-assets/upload",
            permission_route(
                upload::multipart_route(
                    post(file_asset::file_asset_upload),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::file_asset::UPLOAD,
            ),
        )
        .route(
            "/file-assets/register",
            permission_route(post(file_asset::file_asset_register), &permissions::file_asset::UPLOAD),
        )
        .route(
            "/file-assets/{id}",
            permission_route(get(file_asset::file_asset_detail), &permissions::file_asset::DETAIL),
        )
        .route(
            "/file-assets/{id}/preview",
            permission_route(get(file_asset::file_asset_preview), &permissions::file_asset::PREVIEW),
        )
}

// 为父路由挂载文件扫描与销毁路由。
fn lifecycle_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/file-assets/{id}/scan-result",
            permission_route(put(file_asset::file_asset_scan_result), &permissions::file_asset::SCAN_RESULT),
        )
        .route(
            "/file-assets/{id}/destroy",
            permission_route(post(file_asset::file_asset_destroy), &permissions::file_asset::DESTROY),
        )
}

// 为父路由挂载单据附件关联路由。
fn attachment_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/document-attachments",
            permission_route(
                post(file_asset::document_attachment_create),
                &permissions::file_asset::DOCUMENT_ATTACHMENT_CREATE,
            ),
        )
        .route(
            "/documents/{id}/attachments",
            permission_route(
                get(file_asset::document_attachment_list),
                &permissions::file_asset::DOCUMENT_ATTACHMENT_LIST,
            ),
        )
}
