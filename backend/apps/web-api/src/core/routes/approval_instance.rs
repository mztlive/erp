//! D03 阻塞审批管理路由。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::approval_instance, middleware::with_permission},
};

/// 返回阻塞审批诊断与固定恢复动作路由。
///
/// # 返回
/// 返回 `GET /approval-instances` 与
/// `POST /approval-instances/{id}/recover` 路由。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/approval-instances",
            with_permission(
                get(approval_instance::blocked_approval_list),
                rbac,
                approval_instance::blocked_approval_list_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/recover",
            with_permission(
                post(approval_instance::recover_blocked_approval),
                rbac,
                approval_instance::recover_blocked_approval_permission_key(),
            ),
        )
}
