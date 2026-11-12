//! 域 D34 `integration_ops` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/integration/inbox-messages`、
//! `/admin/integration/error-tasks`、`/admin/integration/differences`；人工动作统一使用
//! `/admin/integration/task-actions` 与 `/admin/integration/task-completions` 强命令。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::integration_ops;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        // 入站消息
        .route(
            "/integration/inbox-messages",
            with_permission(
                get(integration_ops::inbox_message_list),
                rbac,
                &permissions::integration_ops::INBOX_MESSAGE_LIST,
            ),
        )
        .route(
            "/integration/inbox-messages",
            with_permission(
                post(integration_ops::inbox_message_register),
                rbac,
                &permissions::integration_ops::INBOX_MESSAGE_REGISTER,
            ),
        )
        .route(
            "/integration/inbox-messages/{id}",
            with_permission(
                get(integration_ops::inbox_message_detail),
                rbac,
                &permissions::integration_ops::INBOX_MESSAGE_DETAIL,
            ),
        )
        .route(
            "/integration/inbox-messages/{id}/result",
            with_permission(
                post(integration_ops::inbox_message_write_back),
                rbac,
                &permissions::integration_ops::INBOX_MESSAGE_WRITE_BACK,
            ),
        )
        // 集成错误任务
        .route(
            "/integration/error-tasks",
            with_permission(
                get(integration_ops::error_task_list),
                rbac,
                &permissions::integration_ops::ERROR_TASK_LIST,
            ),
        )
        .route(
            "/integration/error-tasks",
            with_permission(
                post(integration_ops::error_task_create),
                rbac,
                &permissions::integration_ops::ERROR_TASK_CREATE,
            ),
        )
        .route(
            "/integration/error-tasks/{id}",
            with_permission(
                get(integration_ops::error_task_detail),
                rbac,
                &permissions::integration_ops::ERROR_TASK_DETAIL,
            ),
        )
        .route(
            "/integration/task-actions",
            with_permission(
                post(integration_ops::integration_task_action),
                rbac,
                &permissions::integration_ops::INTEGRATION_TASK_ACTION,
            ),
        )
        .route(
            "/integration/task-completions",
            with_permission(
                post(integration_ops::integration_task_completion),
                rbac,
                &permissions::integration_ops::INTEGRATION_TASK_COMPLETION,
            ),
        )
        // 对账差异
        .route(
            "/integration/differences",
            with_permission(
                get(integration_ops::difference_list),
                rbac,
                &permissions::integration_ops::DIFFERENCE_LIST,
            ),
        )
        .route(
            "/integration/differences",
            with_permission(
                post(integration_ops::difference_create),
                rbac,
                &permissions::integration_ops::DIFFERENCE_CREATE,
            ),
        )
        .route(
            "/integration/differences/{id}",
            with_permission(
                get(integration_ops::difference_detail),
                rbac,
                &permissions::integration_ops::DIFFERENCE_DETAIL,
            ),
        )
        .route(
            "/integration/differences/{id}/decisions",
            with_permission(
                post(integration_ops::difference_decision),
                rbac,
                &permissions::integration_ops::DIFFERENCE_DECISION,
            ),
        )
}
