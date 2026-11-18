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
use erp_permissions as permissions;

use crate::http::handler::sales_selection;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载选品册与销售方案接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(book_collection_routes())
        .merge(book_copy_routes())
        .merge(book_pricing_routes())
        .route(
            "/sales-selection-books/{id}/images",
            permission_route(get(sales_selection::admin_image), &permissions::sales_selection::ADMIN_IMAGE),
        )
        .merge(book_item_routes(true))
        .merge(book_item_routes(false))
        .merge(book_protection_routes_books())
        .merge(book_protection_routes_booklets())
        .route(
            "/sales-selection-proposals",
            permission_route(
                get(sales_selection::proposal_list),
                &permissions::sales_selection::PROPOSAL_LIST,
            ),
        )
        .route(
            "/sales-selection-proposals/{id}",
            permission_route(
                get(sales_selection::proposal_detail),
                &permissions::sales_selection::PROPOSAL_DETAIL,
            ),
        )
}

// 为父路由挂载选品册集合路由。
// 返回列表与创建路由。
fn book_collection_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books",
            permission_route(get(sales_selection::booklet_list), &permissions::sales_selection::BOOKLET_LIST),
        )
        .route(
            "/sales-selection-books",
            permission_route(
                post(sales_selection::booklet_create),
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
        .route(
            "/sales-selection-booklets",
            permission_route(get(sales_selection::booklet_list), &permissions::sales_selection::BOOKLET_LIST),
        )
        .route(
            "/sales-selection-booklets",
            permission_route(
                post(sales_selection::booklet_create),
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
}

// 选品册复制路由，复用创建权限；原册读取范围由用例再次校验。
// 返回两条同义复制路径。
fn book_copy_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/copy",
            permission_route(
                post(sales_selection::booklet_copy),
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/copy",
            permission_route(
                post(sales_selection::booklet_copy),
                &permissions::sales_selection::BOOKLET_CREATE,
            ),
        )
}

// 客户售价维护路由，使用现有选品册维护权限。
// 返回两条同义改价路由。
fn book_pricing_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/prices",
            permission_route(
                post(sales_selection::booklet_update_prices),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/prices",
            permission_route(
                post(sales_selection::booklet_update_prices),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
}

// 为父路由挂载选品册实例路由。
// `books` 为 `true` 使用 `sales-selection-books`，否则使用 `sales-selection-booklets`。
// 返回实例路由。
fn book_item_routes(books: bool) -> Router<HttpState> {
    if books { book_item_routes_books() } else { book_item_routes_booklets() }
}

// 为父路由挂载`sales-selection-books` 实例路由。
// 返回实例路由。
fn book_item_routes_books() -> Router<HttpState> {
    Router::new().merge(book_content_routes()).merge(book_link_routes()).merge(book_lifecycle_routes())
}

// 为父路由挂载选品册资料、准备与展示项路由。
fn book_content_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}",
            permission_route(
                get(sales_selection::booklet_detail),
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/prepare",
            permission_route(
                post(sales_selection::booklet_prepare),
                &permissions::sales_selection::BOOKLET_PREPARE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/display-items/{item_id}",
            permission_route(
                delete(sales_selection::booklet_delete_item),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-books/{id}/display-items/{item_id}/delete",
            permission_route(
                post(sales_selection::booklet_delete_item_post),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
}

// 为父路由挂载选品册发布与链接管理路由。
fn book_link_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/publish",
            permission_route(
                post(sales_selection::booklet_publish),
                &permissions::sales_selection::BOOKLET_PUBLISH,
            ),
        )
        .route(
            "/sales-selection-books/{id}/link",
            permission_route(
                get(sales_selection::booklet_copy_link_url),
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/copy-link",
            permission_route(
                post(sales_selection::booklet_copy_link),
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/replace-link",
            permission_route(
                post(sales_selection::booklet_rotate_link),
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/rotate-link",
            permission_route(
                post(sales_selection::booklet_rotate_link),
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
}

// 为父路由挂载选品册关闭、作废与访问路由。
fn book_lifecycle_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/close",
            permission_route(
                post(sales_selection::booklet_close),
                &permissions::sales_selection::BOOKLET_CLOSE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/revoke-link",
            permission_route(
                post(sales_selection::booklet_revoke),
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/revoke",
            permission_route(
                post(sales_selection::booklet_revoke),
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-books/{id}/void",
            permission_route(
                post(sales_selection::booklet_void),
                &permissions::sales_selection::BOOKLET_VOID,
            ),
        )
        .route(
            "/sales-selection-books/{id}/session",
            permission_route(
                get(sales_selection::booklet_session),
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
}

// 为父路由挂载`sales-selection-booklets` 同义实例路由。
// 返回实例路由。
fn book_item_routes_booklets() -> Router<HttpState> {
    Router::new()
        .merge(booklet_content_routes())
        .merge(booklet_link_routes())
        .merge(booklet_lifecycle_routes())
}

// 为兼容路径组装选品册资料、准备与展示项路由。
fn booklet_content_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-booklets/{id}",
            permission_route(
                get(sales_selection::booklet_detail),
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/prepare",
            permission_route(
                post(sales_selection::booklet_prepare),
                &permissions::sales_selection::BOOKLET_PREPARE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/display-items/{item_id}",
            permission_route(
                delete(sales_selection::booklet_delete_item),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/display-items/{item_id}/delete",
            permission_route(
                post(sales_selection::booklet_delete_item_post),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
}

// 为兼容路径组装选品册发布与链接管理路由。
fn booklet_link_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-booklets/{id}/publish",
            permission_route(
                post(sales_selection::booklet_publish),
                &permissions::sales_selection::BOOKLET_PUBLISH,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/link",
            permission_route(
                get(sales_selection::booklet_copy_link_url),
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/copy-link",
            permission_route(
                post(sales_selection::booklet_copy_link),
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/replace-link",
            permission_route(
                post(sales_selection::booklet_rotate_link),
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/rotate-link",
            permission_route(
                post(sales_selection::booklet_rotate_link),
                &permissions::sales_selection::BOOKLET_ROTATE_LINK,
            ),
        )
}

// 为兼容路径组装选品册关闭、作废与访问路由。
fn booklet_lifecycle_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-booklets/{id}/close",
            permission_route(
                post(sales_selection::booklet_close),
                &permissions::sales_selection::BOOKLET_CLOSE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/revoke-link",
            permission_route(
                post(sales_selection::booklet_revoke),
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/revoke",
            permission_route(
                post(sales_selection::booklet_revoke),
                &permissions::sales_selection::BOOKLET_REVOKE,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/void",
            permission_route(
                post(sales_selection::booklet_void),
                &permissions::sales_selection::BOOKLET_VOID,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/session",
            permission_route(
                get(sales_selection::booklet_session),
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
}

/// 公开选品路由，与 `/admin` 隔离。
/// 返回公开路由。
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

fn book_protection_routes_books() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-books/{id}/access-password",
            permission_route(
                post(sales_selection::protection::booklet_password),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-books/{id}/vouchers",
            permission_route(
                get(sales_selection::protection::booklet_vouchers),
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-books/{id}/selection-details",
            permission_route(
                get(sales_selection::protection::booklet_selection_details),
                &permissions::sales_selection::ADMIN_IMAGE,
            ),
        )
}

fn book_protection_routes_booklets() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-selection-booklets/{id}/access-password",
            permission_route(
                post(sales_selection::protection::booklet_password),
                &permissions::sales_selection::BOOKLET_DELETE_ITEM,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/vouchers",
            permission_route(
                get(sales_selection::protection::booklet_vouchers),
                &permissions::sales_selection::BOOKLET_COPY_LINK,
            ),
        )
        .route(
            "/sales-selection-booklets/{id}/selection-details",
            permission_route(
                get(sales_selection::protection::booklet_selection_details),
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
