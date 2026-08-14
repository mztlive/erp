//! 域 D34 `integration_ops` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/integration/inbox-messages`、
//! `/admin/integration/error-tasks`、`/admin/integration/differences`；人工动作统一使用
//! `/admin/integration/task-actions` 与 `/admin/integration/task-completions` 强命令。

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
            "/integration/task-actions",
            with_permission(
                post(integration_ops::integration_task_action),
                rbac,
                integration_ops::integration_task_action_permission_key(),
            ),
        )
        .route(
            "/integration/task-completions",
            with_permission(
                post(integration_ops::integration_task_completion),
                rbac,
                integration_ops::integration_task_completion_permission_key(),
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
            "/integration/differences/{id}/decisions",
            with_permission(
                post(integration_ops::difference_decision),
                rbac,
                integration_ops::difference_decision_permission_key(),
            ),
        )
}
