//! 域 D15 `purchase_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-orders`、
//! `/admin/purchase-change-orders`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::purchase_order, middleware::with_permission},
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
            "/purchase-orders",
            with_permission(
                get(purchase_order::purchase_order_list),
                rbac,
                purchase_order::purchase_order_list_permission_key(),
            ),
        )
        .route(
            "/purchase-orders",
            with_permission(
                post(purchase_order::purchase_order_create),
                rbac,
                purchase_order::purchase_order_create_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}",
            with_permission(
                get(purchase_order::purchase_order_detail),
                rbac,
                purchase_order::purchase_order_detail_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/draft",
            with_permission(
                post(purchase_order::purchase_order_save_draft),
                rbac,
                purchase_order::purchase_order_save_draft_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/submit",
            with_permission(
                post(purchase_order::purchase_order_submit),
                rbac,
                purchase_order::purchase_order_submit_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/review/approve",
            with_permission(
                post(purchase_order::purchase_order_approve),
                rbac,
                purchase_order::purchase_order_approve_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/review/reject",
            with_permission(
                post(purchase_order::purchase_order_reject),
                rbac,
                purchase_order::purchase_order_reject_permission_key(),
            ),
        )
        .route(
            "/purchase-creation-bases",
            with_permission(
                get(purchase_order::purchase_creation_basis_list),
                rbac,
                purchase_order::purchase_creation_basis_list_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/changes",
            with_permission(
                post(purchase_order::purchase_change_create),
                rbac,
                purchase_order::purchase_change_create_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders",
            with_permission(
                get(purchase_order::purchase_change_list),
                rbac,
                purchase_order::purchase_change_list_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}",
            with_permission(
                get(purchase_order::purchase_change_detail),
                rbac,
                purchase_order::purchase_change_detail_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}/submit",
            with_permission(
                post(purchase_order::purchase_change_submit),
                rbac,
                purchase_order::purchase_change_submit_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}/effect",
            with_permission(
                post(purchase_order::purchase_change_effect),
                rbac,
                purchase_order::purchase_change_effect_permission_key(),
            ),
        )
}
