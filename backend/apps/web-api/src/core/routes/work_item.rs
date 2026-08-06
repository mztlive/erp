//! 域 D03 `work_item` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/work-items`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::work_item, middleware::with_permission},
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
            "/work-items",
            with_permission(
                get(work_item::work_item_list),
                rbac,
                work_item::work_item_list_permission_key(),
            ),
        )
        .route(
            "/work-items",
            with_permission(
                post(work_item::work_item_create),
                rbac,
                work_item::work_item_create_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}",
            with_permission(
                get(work_item::work_item_detail),
                rbac,
                work_item::work_item_detail_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/claim",
            with_permission(
                post(work_item::work_item_claim),
                rbac,
                work_item::work_item_claim_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/defer",
            with_permission(
                post(work_item::work_item_defer),
                rbac,
                work_item::work_item_defer_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/transfer",
            with_permission(
                post(work_item::work_item_transfer),
                rbac,
                work_item::work_item_transfer_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/complete",
            with_permission(
                post(work_item::work_item_complete),
                rbac,
                work_item::work_item_complete_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/close",
            with_permission(
                post(work_item::work_item_close),
                rbac,
                work_item::work_item_close_permission_key(),
            ),
        )
}
