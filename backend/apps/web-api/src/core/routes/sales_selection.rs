//! 销售选品管理端与公开路由。
//!
//! 管理端主路径与前端 `/admin/sales-selection-books` 对齐；
//! `/sales-selection-booklets` 为同义路径。公开页走 `/public/selection/{token}`。
//! 路径必须是字面量，供权限生成扫描。

use axum::{
    routing::{delete, get, post},
    Router,
};
use erp_identity::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::sales_selection, middleware::with_permission},
};

/// 返回选品管理端路由。
///
/// # 参数
/// * `rbac` - RBAC
///
/// # 返回
/// 返回路由。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .merge(book_collection_routes(rbac))
        .route(
            "/sales-selection-books/{id}/images",
            with_permission(
                get(sales_selection::admin_image),
                rbac,
                sales_selection::admin_image_permission_key(),
            ),
        )
        .merge(book_item_routes(rbac, true))
        .merge(book_item_routes(rbac, false))
        .route(
            "/sales-selection-proposals",
            with_permission(
                get(sales_selection::proposal_list),
                rbac,
                sales_selection::proposal_list_permission_key(),
            ),
        )
        .route(
            "/sales-selection-proposals/{id}",
            with_permission(
                get(sales_selection::proposal_detail),
                rbac,
                sales_selection::proposal_detail_permission_key(),
            ),
        )
}

/// 选品册集合路由。
///
/// # 参数
/// * `rbac` - RBAC
///
/// # 返回
/// 返回列表与创建路由。
fn book_collection_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-selection-books",
            with_permission(
                get(sales_selection::booklet_list),
                rbac,
                sales_selection::booklet_list_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books",
            with_permission(
                post(sales_selection::booklet_create),
                rbac,
                sales_selection::booklet_create_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets",
            with_permission(
                get(sales_selection::booklet_list),
                rbac,
                sales_selection::booklet_list_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets",
            with_permission(
                post(sales_selection::booklet_create),
                rbac,
                sales_selection::booklet_create_permission_key(),
            ),
        )
}

/// 选品册实例路由。
///
/// # 参数
/// * `rbac` - RBAC
/// * `books` - `true` 使用 `sales-selection-books`，否则使用 `sales-selection-booklets`
///
/// # 返回
/// 返回实例路由。
fn book_item_routes(rbac: &SharedRbacService, books: bool) -> Router<AppState> {
    if books {
        book_item_routes_books(rbac)
    } else {
        book_item_routes_booklets(rbac)
    }
}

/// `sales-selection-books` 实例路由。
///
/// # 参数
/// * `rbac` - RBAC
///
/// # 返回
/// 返回实例路由。
fn book_item_routes_books(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}",
            with_permission(
                get(sales_selection::booklet_detail),
                rbac,
                sales_selection::booklet_detail_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/prepare",
            with_permission(
                post(sales_selection::booklet_prepare),
                rbac,
                sales_selection::booklet_prepare_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/display-items/{item_id}",
            with_permission(
                delete(sales_selection::booklet_delete_item),
                rbac,
                sales_selection::booklet_delete_item_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/display-items/{item_id}/delete",
            with_permission(
                post(sales_selection::booklet_delete_item_post),
                rbac,
                sales_selection::booklet_delete_item_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/publish",
            with_permission(
                post(sales_selection::booklet_publish),
                rbac,
                sales_selection::booklet_publish_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/link",
            with_permission(
                get(sales_selection::booklet_copy_link_url),
                rbac,
                sales_selection::booklet_copy_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/copy-link",
            with_permission(
                post(sales_selection::booklet_copy_link),
                rbac,
                sales_selection::booklet_copy_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/replace-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                sales_selection::booklet_rotate_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/rotate-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                sales_selection::booklet_rotate_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/close",
            with_permission(
                post(sales_selection::booklet_close),
                rbac,
                sales_selection::booklet_close_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/revoke-link",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                sales_selection::booklet_revoke_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/revoke",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                sales_selection::booklet_revoke_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/void",
            with_permission(
                post(sales_selection::booklet_void),
                rbac,
                sales_selection::booklet_void_permission_key(),
            ),
        )
        .route(
            "/sales-selection-books/{id}/session",
            with_permission(
                get(sales_selection::booklet_session),
                rbac,
                sales_selection::booklet_detail_permission_key(),
            ),
        )
}

/// `sales-selection-booklets` 同义实例路由。
///
/// # 参数
/// * `rbac` - RBAC
///
/// # 返回
/// 返回实例路由。
fn book_item_routes_booklets(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-selection-booklets/{id}",
            with_permission(
                get(sales_selection::booklet_detail),
                rbac,
                sales_selection::booklet_detail_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/prepare",
            with_permission(
                post(sales_selection::booklet_prepare),
                rbac,
                sales_selection::booklet_prepare_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/display-items/{item_id}",
            with_permission(
                delete(sales_selection::booklet_delete_item),
                rbac,
                sales_selection::booklet_delete_item_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/display-items/{item_id}/delete",
            with_permission(
                post(sales_selection::booklet_delete_item_post),
                rbac,
                sales_selection::booklet_delete_item_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/publish",
            with_permission(
                post(sales_selection::booklet_publish),
                rbac,
                sales_selection::booklet_publish_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/link",
            with_permission(
                get(sales_selection::booklet_copy_link_url),
                rbac,
                sales_selection::booklet_copy_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/copy-link",
            with_permission(
                post(sales_selection::booklet_copy_link),
                rbac,
                sales_selection::booklet_copy_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/replace-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                sales_selection::booklet_rotate_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/rotate-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                sales_selection::booklet_rotate_link_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/close",
            with_permission(
                post(sales_selection::booklet_close),
                rbac,
                sales_selection::booklet_close_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/revoke-link",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                sales_selection::booklet_revoke_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/revoke",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                sales_selection::booklet_revoke_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/void",
            with_permission(
                post(sales_selection::booklet_void),
                rbac,
                sales_selection::booklet_void_permission_key(),
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/session",
            with_permission(
                get(sales_selection::booklet_session),
                rbac,
                sales_selection::booklet_detail_permission_key(),
            ),
        )
}

/// 公开选品路由，与 `/admin` 隔离。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回公开路由。
///
/// # 错误
/// 无。
pub fn public_routes() -> Router<AppState> {
    Router::new()
        .route("/public/selection/{token}", get(sales_selection::public_page))
        .route(
            "/public/selection/{token}/session",
            post(sales_selection::public_save).put(sales_selection::public_save),
        )
        .route(
            "/public/selection/{token}/submit",
            post(sales_selection::public_submit),
        )
        .route(
            "/public/selection/{token}/receipt",
            get(sales_selection::public_page),
        )
        .route(
            "/public/selection/{token}/images",
            get(sales_selection::public_image),
        )
        .route(
            "/public/selection/{token}/images/{asset_id}",
            get(sales_selection::public_image_path),
        )
        .route(
            "/public/selection/{token}/customize",
            post(sales_selection::public_customize),
        )
        .route(
            "/public/sales-selections/{token}",
            get(sales_selection::public_page),
        )
        .route(
            "/public/sales-selections/{token}/session",
            post(sales_selection::public_save).put(sales_selection::public_save),
        )
        .route(
            "/public/sales-selections/{token}/submit",
            post(sales_selection::public_submit),
        )
        .route(
            "/public/sales-selections/{token}/receipt",
            get(sales_selection::public_page),
        )
        .route(
            "/public/sales-selections/{token}/images",
            get(sales_selection::public_image),
        )
        .route(
            "/public/sales-selections/{token}/images/{asset_id}",
            get(sales_selection::public_image_path),
        )
        .route(
            "/public/sales-selections/{token}/customize",
            post(sales_selection::public_customize),
        )
}
