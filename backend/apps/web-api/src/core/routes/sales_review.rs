//! 域 D14 `sales_review` 管理端路由。
//!
//! 仅保留销售变更单；卡券专用审批、采购确认与低毛利路由已删除。

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
            "/sales-change-orders/{id}/void",
            with_permission(
                post(sales_review::sales_change_order_void),
                rbac,
                sales_review::sales_change_order_void_permission_key(),
            ),
        )
        .route(
            "/sales-change-orders/{id}/cancel-approval",
            with_permission(
                post(sales_review::sales_change_order_cancel_approval),
                rbac,
                sales_review::sales_change_order_cancel_approval_permission_key(),
            ),
        )
}
