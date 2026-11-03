//! 域 D14 `sales_review` 管理端路由。
//!
//! 仅保留销售变更单；卡券专用审批、采购确认与低毛利路由已删除。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::http::handler::sales_review;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
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
        .merge(draft_routes(rbac))
        .route(
            "/sales-change-orders",
            with_permission(
                get(sales_review::sales_change_order_list),
                rbac,
                permissions::sales_review::SALES_CHANGE_ORDER_LIST,
            ),
        )
        .route(
            "/sales-change-orders",
            with_permission(
                post(sales_review::sales_change_order_create),
                rbac,
                permissions::sales_review::SALES_CHANGE_ORDER_CREATE,
            ),
        )
        .route(
            "/sales-change-orders/{id}",
            with_permission(
                get(sales_review::sales_change_order_detail),
                rbac,
                permissions::sales_review::SALES_CHANGE_ORDER_DETAIL,
            ),
        )
        .route(
            "/sales-change-orders/{id}/submit-impact",
            with_permission(
                post(sales_review::sales_change_order_submit_impact),
                rbac,
                permissions::sales_review::SALES_CHANGE_ORDER_SUBMIT_IMPACT,
            ),
        )
        .route(
            "/sales-change-orders/{id}/void",
            with_permission(
                post(sales_review::sales_change_order_void),
                rbac,
                permissions::sales_review::SALES_CHANGE_ORDER_VOID,
            ),
        )
        .route(
            "/sales-change-orders/{id}/cancel-approval",
            with_permission(
                post(sales_review::sales_change_order_cancel_approval),
                rbac,
                permissions::sales_review::SALES_CHANGE_ORDER_CANCEL_APPROVAL,
            ),
        )
}

/// 销售变更草稿读取与保存路由。
///
/// # 参数
/// * `rbac` - 当前共享权限服务
/// # 返回
/// 返回逐操作保留权限校验和上传限制的路由集合。
/// # 错误
/// 无；业务错误由各处理器返回。
fn draft_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-change-orders/{id}/draft",
            with_permission(
                get(sales_review::sales_change_draft),
                rbac,
                permissions::sales_review::SALES_CHANGE_DRAFT,
            ),
        )
        .route(
            "/sales-change-orders/{id}/draft",
            with_permission(
                post(sales_review::sales_change_save_draft),
                rbac,
                permissions::sales_review::SALES_CHANGE_SAVE_DRAFT,
            ),
        )
}
