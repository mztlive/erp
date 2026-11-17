//! 域 D22 `legacy_import` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/legacy-import-batches`、
//! `/admin/legacy-import-batches/{id}`、`/admin/legacy-import-batches/{id}/rows`、
//! `/admin/legacy-import-batches/{id}/apply`、`/admin/legacy-import-confirmations`、
//! `/admin/legacy-import-batches/{id}/commands`、`/admin/legacy-import-confirmations/complete`；
//! 每条路由统一走
//! JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::legacy_import;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回声明公共权限目录信息的路由集合。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    Router::new().merge(batch_routes()).merge(confirmation_routes())
}

/// 历史数据导入批次路由。
fn batch_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/legacy-import-batches",
            permission_route(
                get(legacy_import::legacy_import_batch_list),
                &permissions::legacy_import::BATCH_LIST,
            ),
        )
        .route(
            "/legacy-import-batches",
            permission_route(
                post(legacy_import::legacy_import_batch_create),
                &permissions::legacy_import::BATCH_CREATE,
            ),
        )
        .route(
            "/legacy-import-batches/{id}",
            permission_route(
                get(legacy_import::legacy_import_batch_detail),
                &permissions::legacy_import::BATCH_DETAIL,
            ),
        )
        .route(
            "/legacy-import-batches/{id}/rows",
            permission_route(
                get(legacy_import::legacy_import_row_list),
                &permissions::legacy_import::ROW_LIST,
            ),
        )
        .route(
            "/legacy-import-batches/{id}/apply",
            permission_route(
                post(legacy_import::legacy_import_batch_apply),
                &permissions::legacy_import::BATCH_APPLY,
            ),
        )
        .route(
            "/legacy-import-batches/{id}/commands",
            permission_route(
                post(legacy_import::legacy_import_execution_command),
                &permissions::legacy_import::EXECUTION_COMMAND,
            ),
        )
}

/// 历史数据导入确认路由。
fn confirmation_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/legacy-import-confirmations",
            permission_route(
                get(legacy_import::legacy_import_confirmation_list),
                &permissions::legacy_import::CONFIRMATION_LIST,
            ),
        )
        .route(
            "/legacy-import-confirmations",
            permission_route(
                post(legacy_import::legacy_import_confirmation_create),
                &permissions::legacy_import::CONFIRMATION_CREATE,
            ),
        )
        .route(
            "/legacy-import-confirmations/complete",
            permission_route(
                post(legacy_import::legacy_import_confirmation_complete),
                &permissions::legacy_import::CONFIRMATION_COMPLETE,
            ),
        )
}
