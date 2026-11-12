//! 域 D04 `bulk_job` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/bulk-selection-snapshots`、
//! `/admin/background-jobs`；每条路由统一走 JWT + RBAC（`with_permission`），
//! 权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::bulk_job;
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
        .route(
            "/bulk-selection-snapshots",
            with_permission(
                get(bulk_job::bulk_selection_snapshot_list),
                rbac,
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_LIST,
            ),
        )
        .route(
            "/bulk-selection-snapshots",
            with_permission(
                post(bulk_job::bulk_selection_snapshot_create),
                rbac,
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_CREATE,
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/confirm",
            with_permission(
                post(bulk_job::bulk_selection_snapshot_confirm),
                rbac,
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_CONFIRM,
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/expire",
            with_permission(
                post(bulk_job::bulk_selection_snapshot_expire),
                rbac,
                &permissions::bulk_job::BULK_SELECTION_SNAPSHOT_EXPIRE,
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/items",
            with_permission(
                get(bulk_job::bulk_selection_item_list),
                rbac,
                &permissions::bulk_job::BULK_SELECTION_ITEM_LIST,
            ),
        )
        .route(
            "/background-jobs",
            with_permission(
                get(bulk_job::background_job_list),
                rbac,
                &permissions::bulk_job::BACKGROUND_JOB_LIST,
            ),
        )
        .route(
            "/background-jobs",
            with_permission(
                post(bulk_job::background_job_create),
                rbac,
                &permissions::bulk_job::BACKGROUND_JOB_CREATE,
            ),
        )
        .route(
            "/background-jobs/cancel-all",
            with_permission(
                post(bulk_job::background_job_cancel_all),
                rbac,
                &permissions::bulk_job::BACKGROUND_JOB_CANCEL_ALL,
            ),
        )
        .route(
            "/background-jobs/{id}",
            with_permission(
                get(bulk_job::background_job_detail),
                rbac,
                &permissions::bulk_job::BACKGROUND_JOB_DETAIL,
            ),
        )
        .route(
            "/background-jobs/{id}/cancel",
            with_permission(
                post(bulk_job::background_job_cancel),
                rbac,
                &permissions::bulk_job::BACKGROUND_JOB_CANCEL,
            ),
        )
        .route(
            "/background-jobs/{id}/items",
            with_permission(
                get(bulk_job::background_job_item_list),
                rbac,
                &permissions::bulk_job::BACKGROUND_JOB_ITEM_LIST,
            ),
        )
}
