//! 域 D34 `integration_ops` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/integration/inbox-messages`、
//! `/admin/integration/error-tasks`、`/admin/integration/differences`；人工动作统一使用
//! `/admin/integration/task-actions` 与 `/admin/integration/task-completions` 强命令。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::integration_ops;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载集成运维与对账接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new().merge(inbox_routes()).merge(task_routes()).merge(difference_routes())
}

// 为父路由挂载集成消息登记与读取路由。
fn inbox_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/integration/inbox-messages",
            permission_route(
                get(integration_ops::inbox_message_list),
                &permissions::integration_ops::INBOX_MESSAGE_LIST,
            ),
        )
        .route(
            "/integration/inbox-messages",
            permission_route(
                post(integration_ops::inbox_message_register),
                &permissions::integration_ops::INBOX_MESSAGE_REGISTER,
            ),
        )
        .route(
            "/integration/inbox-messages/{id}",
            permission_route(
                get(integration_ops::inbox_message_detail),
                &permissions::integration_ops::INBOX_MESSAGE_DETAIL,
            ),
        )
        .route(
            "/integration/inbox-messages/{id}/result",
            permission_route(
                post(integration_ops::inbox_message_write_back),
                &permissions::integration_ops::INBOX_MESSAGE_WRITE_BACK,
            ),
        )
}

// 为父路由挂载集成异常任务与处置路由。
fn task_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/integration/error-tasks",
            permission_route(
                get(integration_ops::error_task_list),
                &permissions::integration_ops::ERROR_TASK_LIST,
            ),
        )
        .route(
            "/integration/error-tasks",
            permission_route(
                post(integration_ops::error_task_create),
                &permissions::integration_ops::ERROR_TASK_CREATE,
            ),
        )
        .route(
            "/integration/error-tasks/{id}",
            permission_route(
                get(integration_ops::error_task_detail),
                &permissions::integration_ops::ERROR_TASK_DETAIL,
            ),
        )
        .route(
            "/integration/task-actions",
            permission_route(
                post(integration_ops::integration_task_action),
                &permissions::integration_ops::INTEGRATION_TASK_ACTION,
            ),
        )
        .route(
            "/integration/task-completions",
            permission_route(
                post(integration_ops::integration_task_completion),
                &permissions::integration_ops::INTEGRATION_TASK_COMPLETION,
            ),
        )
}

// 为父路由挂载集成差异查询与确认路由。
fn difference_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/integration/differences",
            permission_route(
                get(integration_ops::difference_list),
                &permissions::integration_ops::DIFFERENCE_LIST,
            ),
        )
        .route(
            "/integration/differences",
            permission_route(
                post(integration_ops::difference_create),
                &permissions::integration_ops::DIFFERENCE_CREATE,
            ),
        )
        .route(
            "/integration/differences/{id}",
            permission_route(
                get(integration_ops::difference_detail),
                &permissions::integration_ops::DIFFERENCE_DETAIL,
            ),
        )
        .route(
            "/integration/differences/{id}/decisions",
            permission_route(
                post(integration_ops::difference_decision),
                &permissions::integration_ops::DIFFERENCE_DECISION,
            ),
        )
}
