//! 域 D32 `supplier_fulfillment` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-fulfillment-orders`、
//! `/admin/supplier-refund-facts`；每条路由统一走 JWT + RBAC（`with_permission`），
//! handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier_fulfillment, middleware::with_permission},
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
            "/supplier-fulfillment-orders",
            with_permission(
                get(supplier_fulfillment::supplier_fulfillment_order_list),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_list_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_submit),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_submit_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders/investigations",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_investigation),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_investigation_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders/task-investigations",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_task_investigation),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_task_investigation_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders/task-completions",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_task_completion),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_task_completion_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}",
            with_permission(
                get(supplier_fulfillment::supplier_fulfillment_order_detail),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/cancel",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_cancel),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_cancel_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/refund",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_refund),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_refund_permission_key(),
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/reject",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_reject),
                rbac,
                supplier_fulfillment::supplier_fulfillment_order_reject_permission_key(),
            ),
        )
        .route(
            "/supplier-refund-facts",
            with_permission(
                post(supplier_fulfillment::supplier_refund_fact_post),
                rbac,
                supplier_fulfillment::supplier_refund_fact_post_permission_key(),
            ),
        )
}
