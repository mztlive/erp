//! 销售选品管理端与公开路由。
//!
//! 管理端主路径与前端 `/admin/sales-selection-books` 对齐；
//! `/sales-selection-booklets` 为同义路径。公开页走 `/public/selection/{token}`。
//! 路径必须是字面量，供权限生成扫描。

use axum::http::HeaderValue;
use axum::http::header::{CACHE_CONTROL, REFERRER_POLICY};
use axum::response::Response;
use axum::routing::{delete, get, post};
use axum::{Router, middleware};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::sales_selection;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;

/// 返回选品管理端路由。
///
/// # 参数
/// * `rbac` - RBAC
///
/// # 返回
/// 返回路由。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .merge(book_collection_routes(rbac))
        .merge(book_copy_routes(rbac))
        .merge(book_pricing_routes(rbac))
        .route(
            "/sales-selection-books/{id}/images",
            with_permission(
                get(sales_selection::admin_image),
                rbac,
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
        .merge(book_item_routes(rbac, true))
        .merge(book_item_routes(rbac, false))
        .merge(book_protection_routes_books(rbac))
        .merge(book_protection_routes_booklets(rbac))
        .route(
            "/sales-selection-proposals",
            with_permission(
                get(sales_selection::proposal_list),
                rbac,
                &permissions::sales_selection::PROPOSAL_LIST,
            ),
        )
        .route(
            "/sales-selection-proposals/{id}",
            with_permission(
                get(sales_selection::proposal_detail),
                rbac,
                &permissions::sales_selection::PROPOSAL_DETAIL,
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
fn book_collection_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books",
            with_permission(
                get(sales_selection::booklet_list),
                rbac,
                &permissions::sales_selection::BOOKLET_LIST,
            ),
        )
        .route(
            "/sales-selection-books",
            with_permission(
                post(sales_selection::booklet_create),
                rbac,
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
        .route(
            "/sales-selection-booklets",
            with_permission(
                get(sales_selection::booklet_list),
                rbac,
                &permissions::sales_selection::BOOKLET_LIST,
            ),
        )
        .route(
            "/sales-selection-booklets",
            with_permission(
                post(sales_selection::booklet_create),
                rbac,
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
}

/// 选品册复制路由，复用创建权限；原册读取范围由用例再次校验。
///
/// # 参数
/// * `rbac` - RBAC
///
/// # 返回
/// 返回两条同义复制路径。
///
/// # 错误
/// 无。
fn book_copy_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/copy",
            with_permission(
                post(sales_selection::booklet_copy),
                rbac,
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/copy",
            with_permission(
                post(sales_selection::booklet_copy),
                rbac,
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
}

/// 客户售价维护路由，使用现有选品册维护权限。
///
/// # 参数
/// `rbac` 为权限服务。
///
/// # 返回
/// 返回两条同义改价路由。
///
/// # 错误
/// 无。
fn book_pricing_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/prices",
            with_permission(
                post(sales_selection::booklet_update_prices),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/prices",
            with_permission(
                post(sales_selection::booklet_update_prices),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
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
fn book_item_routes(rbac: &AuthorizationService, books: bool) -> Router<HttpState> {
    if books { book_item_routes_books(rbac) } else { book_item_routes_booklets(rbac) }
}

/// `sales-selection-books` 实例路由。
///
/// # 参数
/// * `rbac` - RBAC
///
/// # 返回
/// 返回实例路由。
fn book_item_routes_books(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}",
            with_permission(
                get(sales_selection::booklet_detail),
                rbac,
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/prepare",
            with_permission(
                post(sales_selection::booklet_prepare),
                rbac,
                &permissions::sales_selection::BOOKLET_PREPARE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/display-items/{item_id}",
            with_permission(
                delete(sales_selection::booklet_delete_item),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-books/{id}/display-items/{item_id}/delete",
            with_permission(
                post(sales_selection::booklet_delete_item_post),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-books/{id}/publish",
            with_permission(
                post(sales_selection::booklet_publish),
                rbac,
                &permissions::sales_selection::BOOKLET_PUBLISH,
            ),
        )
        .route(
            "/sales-selection-books/{id}/link",
            with_permission(
                get(sales_selection::booklet_copy_link_url),
                rbac,
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/copy-link",
            with_permission(
                post(sales_selection::booklet_copy_link),
                rbac,
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/replace-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/rotate-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/close",
            with_permission(
                post(sales_selection::booklet_close),
                rbac,
                &permissions::sales_selection::BOOKLET_CLOSE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/revoke-link",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/revoke",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/void",
            with_permission(
                post(sales_selection::booklet_void),
                rbac,
                &permissions::sales_selection::BOOKLET_VOID,
            ),
        )
        .route(
            "/sales-selection-books/{id}/session",
            with_permission(
                get(sales_selection::booklet_session),
                rbac,
                &permissions::sales_selection::ADMIN_IMAGE,
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
fn book_item_routes_booklets(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-booklets/{id}",
            with_permission(
                get(sales_selection::booklet_detail),
                rbac,
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/prepare",
            with_permission(
                post(sales_selection::booklet_prepare),
                rbac,
                &permissions::sales_selection::BOOKLET_PREPARE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/display-items/{item_id}",
            with_permission(
                delete(sales_selection::booklet_delete_item),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/display-items/{item_id}/delete",
            with_permission(
                post(sales_selection::booklet_delete_item_post),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/publish",
            with_permission(
                post(sales_selection::booklet_publish),
                rbac,
                &permissions::sales_selection::BOOKLET_PUBLISH,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/link",
            with_permission(
                get(sales_selection::booklet_copy_link_url),
                rbac,
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/copy-link",
            with_permission(
                post(sales_selection::booklet_copy_link),
                rbac,
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/replace-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/rotate-link",
            with_permission(
                post(sales_selection::booklet_rotate_link),
                rbac,
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/close",
            with_permission(
                post(sales_selection::booklet_close),
                rbac,
                &permissions::sales_selection::BOOKLET_CLOSE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/revoke-link",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/revoke",
            with_permission(
                post(sales_selection::booklet_revoke),
                rbac,
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/void",
            with_permission(
                post(sales_selection::booklet_void),
                rbac,
                &permissions::sales_selection::BOOKLET_VOID,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/session",
            with_permission(
                get(sales_selection::booklet_session),
                rbac,
                &permissions::sales_selection::ADMIN_IMAGE,
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
pub fn public_routes() -> Router<HttpState> {
    Router::new()
        .route("/public/selection/{token}", get(sales_selection::public_page))
        .route("/public/selection/{token}/unlock", post(sales_selection::protection::public_unlock))
        .route(
            "/public/selection/{token}/session",
            post(sales_selection::public_save).put(sales_selection::public_save),
        )
        .route("/public/selection/{token}/submit", post(sales_selection::public_submit))
        .route("/public/selection/{token}/receipt", get(sales_selection::public_page))
        .route("/public/selection/{token}/images", get(sales_selection::public_image))
        .route("/public/selection/{token}/images/{asset_id}", get(sales_selection::public_image_path))
        .route("/public/selection/{token}/customize", post(sales_selection::public_customize))
        .route("/public/sales-selections/{token}", get(sales_selection::public_page))
        .route("/public/sales-selections/{token}/unlock", post(sales_selection::protection::public_unlock))
        .route(
            "/public/sales-selections/{token}/session",
            post(sales_selection::public_save).put(sales_selection::public_save),
        )
        .route("/public/sales-selections/{token}/submit", post(sales_selection::public_submit))
        .route("/public/sales-selections/{token}/receipt", get(sales_selection::public_page))
        .route("/public/sales-selections/{token}/images", get(sales_selection::public_image))
        .route("/public/sales-selections/{token}/images/{asset_id}", get(sales_selection::public_image_path))
        .route("/public/sales-selections/{token}/customize", post(sales_selection::public_customize))
        .layer(middleware::map_response(private_public_response))
}

fn book_protection_routes_books(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/access-password",
            with_permission(
                post(sales_selection::protection::booklet_password),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-books/{id}/vouchers",
            with_permission(
                get(sales_selection::protection::booklet_vouchers),
                rbac,
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/selection-details",
            with_permission(
                get(sales_selection::protection::booklet_selection_details),
                rbac,
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
}

fn book_protection_routes_booklets(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-booklets/{id}/access-password",
            with_permission(
                post(sales_selection::protection::booklet_password),
                rbac,
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/vouchers",
            with_permission(
                get(sales_selection::protection::booklet_vouchers),
                rbac,
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/selection-details",
            with_permission(
                get(sales_selection::protection::booklet_selection_details),
                rbac,
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
}

async fn private_public_response(mut response: Response) -> Response {
    response.headers_mut().insert(CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response.headers_mut().insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    response
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::header::{CACHE_CONTROL, REFERRER_POLICY};
    use axum::response::Response;

    #[tokio::test]
    async fn protected_public_responses_disallow_caching_and_referrer_forwarding() {
        let response = super::private_public_response(Response::new(Body::empty())).await;
        assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "private, no-store");
        assert_eq!(response.headers().get(REFERRER_POLICY).unwrap(), "no-referrer");
    }
}
