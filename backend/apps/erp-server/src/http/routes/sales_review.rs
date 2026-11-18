//! 域 D14 `sales_review` 管理端路由。
//!
//! 仅保留销售变更单；卡券专用审批、采购确认与低毛利路由已删除。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::sales_review;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载销售变更接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(draft_routes())
        .route(
            "/sales-change-orders",
            permission_route(
                get(sales_review::sales_change_order_list),
                &permissions::sales_review::SALES_CHANGE_ORDER_LIST,
            ),
        )
        .route(
            "/sales-change-orders",
            permission_route(
                post(sales_review::sales_change_order_create),
                &permissions::sales_review::SALES_CHANGE_ORDER_CREATE,
            ),
        )
        .route(
            "/sales-change-orders/{id}",
            permission_route(
                get(sales_review::sales_change_order_detail),
                &permissions::sales_review::SALES_CHANGE_ORDER_DETAIL,
            ),
        )
        .route(
            "/sales-change-orders/{id}/submit-impact",
            permission_route(
                post(sales_review::sales_change_order_submit_impact),
                &permissions::sales_review::SALES_CHANGE_DRAFT,
            ),
        )
        .route(
            "/sales-change-orders/{id}/void",
            permission_route(
                post(sales_review::sales_change_order_void),
                &permissions::sales_review::SALES_CHANGE_ORDER_VOID,
            ),
        )
        .route(
            "/sales-change-orders/{id}/cancel-approval",
            permission_route(
                post(sales_review::sales_change_order_cancel_approval),
                &permissions::sales_review::SALES_CHANGE_DRAFT,
            ),
        )
}

// 为父路由挂载销售变更草稿读取与保存路由。
fn draft_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-change-orders/{id}/draft",
            permission_route(
                get(sales_review::sales_change_draft),
                &permissions::sales_review::SALES_CHANGE_DRAFT,
            ),
        )
        .route(
            "/sales-change-orders/{id}/draft",
            permission_route(
                post(sales_review::sales_change_save_draft),
                &permissions::sales_review::SALES_CHANGE_DRAFT,
            ),
        )
}
