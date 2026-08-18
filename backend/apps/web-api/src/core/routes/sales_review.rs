//! 域 D14 `sales_review` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-order-reviews`、
//! `/admin/sales-change-orders`；采购二次确认与低毛利路由已移除。每条路由统一
//! 走 JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::sales_review, middleware::with_permission},
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
        // 卡券销售审批轨（W05）
        .route(
            "/sales-order-reviews",
            with_permission(
                get(sales_review::sales_order_review_list),
                rbac,
                sales_review::sales_order_review_list_permission_key(),
            ),
        )
        .route(
            "/sales-order-reviews/decisions",
            with_permission(
                post(sales_review::sales_order_review_decide),
                rbac,
                sales_review::sales_order_review_decide_permission_key(),
            ),
        )
        .route(
            "/sales-order-reviews/cancellations",
            with_permission(
                post(sales_review::sales_order_review_cancel),
                rbac,
                sales_review::sales_order_review_cancel_permission_key(),
            ),
        )
        // 销售变更单（W05 变更轨）
        .route(
            "/sales-change-orders",
            with_permission(
                get(sales_review::sales_change_order_list),
                rbac,
                sales_review::sales_change_order_list_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders",
            with_permission(
                post(sales_review::sales_change_order_create),
                rbac,
                sales_review::sales_change_order_create_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}",
            with_permission(
                get(sales_review::sales_change_order_detail),
                rbac,
                sales_review::sales_change_order_detail_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}/submit-impact",
            with_permission(
                post(sales_review::sales_change_order_submit_impact),
                rbac,
                sales_review::sales_change_order_submit_impact_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}/impact-confirm",
            with_permission(
                post(sales_review::sales_change_order_confirm_impact),
                rbac,
                sales_review::sales_change_order_confirm_impact_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}/impact-reject",
            with_permission(
                post(sales_review::sales_change_order_reject_impact),
                rbac,
                sales_review::sales_change_order_reject_impact_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}/finance-confirm",
            with_permission(
                post(sales_review::sales_change_order_confirm_finance),
                rbac,
                sales_review::sales_change_order_confirm_finance_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}/finance-reject",
            with_permission(
                post(sales_review::sales_change_order_reject_finance),
                rbac,
                sales_review::sales_change_order_reject_finance_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}/void",
            with_permission(
                post(sales_review::sales_change_order_void),
                rbac,
                sales_review::sales_change_order_void_permission_key(),
            ),
        )
}
