//! 域 D34 `integration_ops` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/integration/inbox-messages`、
//! `/admin/integration/error-tasks`、`/admin/integration/differences`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::integration_ops, middleware::with_permission},
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
        // 入站消息
        .route(
            "/integration/inbox-messages",
            with_permission(
                get(integration_ops::inbox_message_list),
                rbac,
                integration_ops::inbox_message_list_permission_key(),
            ),
        )
        .route(
            "/integration/inbox-messages",
            with_permission(
                post(integration_ops::inbox_message_register),
                rbac,
                integration_ops::inbox_message_register_permission_key(),
            ),
        )
        .route(
            "/integration/inbox-messages/{id}",
            with_permission(
                get(integration_ops::inbox_message_detail),
                rbac,
                integration_ops::inbox_message_detail_permission_key(),
            ),
        )
        .route(
            "/integration/inbox-messages/{id}/result",
            with_permission(
                post(integration_ops::inbox_message_write_back),
                rbac,
                integration_ops::inbox_message_write_back_permission_key(),
            ),
        )
        // 集成错误任务
        .route(
            "/integration/error-tasks",
            with_permission(
                get(integration_ops::error_task_list),
                rbac,
                integration_ops::error_task_list_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks",
            with_permission(
                post(integration_ops::error_task_create),
                rbac,
                integration_ops::error_task_create_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks/{id}",
            with_permission(
                get(integration_ops::error_task_detail),
                rbac,
                integration_ops::error_task_detail_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks/{id}/query",
            with_permission(
                post(integration_ops::error_task_query),
                rbac,
                integration_ops::error_task_query_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks/{id}/replay",
            with_permission(
                post(integration_ops::error_task_replay),
                rbac,
                integration_ops::error_task_replay_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks/{id}/hold",
            with_permission(
                post(integration_ops::error_task_hold),
                rbac,
                integration_ops::error_task_hold_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks/{id}/transfer",
            with_permission(
                post(integration_ops::error_task_transfer),
                rbac,
                integration_ops::error_task_transfer_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks/{id}/resolve",
            with_permission(
                post(integration_ops::error_task_resolve),
                rbac,
                integration_ops::error_task_resolve_permission_key(),
            ),
        )
        .route(
            "/integration/error-tasks/{id}/close",
            with_permission(
                post(integration_ops::error_task_close),
                rbac,
                integration_ops::error_task_close_permission_key(),
            ),
        )
        // 对账差异
        .route(
            "/integration/differences",
            with_permission(
                get(integration_ops::difference_list),
                rbac,
                integration_ops::difference_list_permission_key(),
            ),
        )
        .route(
            "/integration/differences",
            with_permission(
                post(integration_ops::difference_create),
                rbac,
                integration_ops::difference_create_permission_key(),
            ),
        )
        .route(
            "/integration/differences/{id}",
            with_permission(
                get(integration_ops::difference_detail),
                rbac,
                integration_ops::difference_detail_permission_key(),
            ),
        )
        .route(
            "/integration/differences/{id}/process",
            with_permission(
                post(integration_ops::difference_process),
                rbac,
                integration_ops::difference_process_permission_key(),
            ),
        )
        .route(
            "/integration/differences/{id}/resolve",
            with_permission(
                post(integration_ops::difference_resolve),
                rbac,
                integration_ops::difference_resolve_permission_key(),
            ),
        )
}
