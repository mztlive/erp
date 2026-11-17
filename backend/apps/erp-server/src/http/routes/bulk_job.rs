//! 域 D04 `bulk_job` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/bulk-selection-snapshots`、
//! `/admin/background-jobs`；每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，
//! 权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::bulk_job;
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
    Router::new().merge(selection_routes()).merge(job_routes())
}

/// 批量选择快照路由。
fn selection_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/bulk-selection-snapshots",
            permission_route(
                get(bulk_job::bulk_selection_snapshot_list),
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_LIST,
            ),
        )
        .route(
            "/bulk-selection-snapshots",
            permission_route(
                post(bulk_job::bulk_selection_snapshot_create),
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_CREATE,
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/confirm",
            permission_route(
                post(bulk_job::bulk_selection_snapshot_confirm),
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_CONFIRM,
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/expire",
            permission_route(
                post(bulk_job::bulk_selection_snapshot_expire),
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_EXPIRE,
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/items",
            permission_route(
                get(bulk_job::bulk_selection_item_list),
                &permissions::bulk_job::BULK_SELECTION_ITEM_LIST,
            ),
        )
}

/// 后台任务查询与控制路由。
fn job_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/background-jobs",
            permission_route(get(bulk_job::background_job_list), &permissions::bulk_job::BACKGROUND_JOB_LIST),
        )
        .route(
            "/background-jobs",
            permission_route(
                post(bulk_job::background_job_create),
                &permissions::bulk_job::BACKGROUND_JOB_CREATE,
            ),
        )
        .route(
            "/background-jobs/cancel-all",
            permission_route(
                post(bulk_job::background_job_cancel_all),
                &permissions::bulk_job::BACKGROUND_JOB_CANCEL_ALL,
            ),
        )
        .route(
            "/background-jobs/{id}",
            permission_route(
                get(bulk_job::background_job_detail),
                &permissions::bulk_job::BACKGROUND_JOB_DETAIL,
            ),
        )
        .route(
            "/background-jobs/{id}/cancel",
            permission_route(
                post(bulk_job::background_job_cancel),
                &permissions::bulk_job::BACKGROUND_JOB_CANCEL,
            ),
        )
        .route(
            "/background-jobs/{id}/items",
            permission_route(
                get(bulk_job::background_job_item_list),
                &permissions::bulk_job::BACKGROUND_JOB_ITEM_LIST,
            ),
        )
}
