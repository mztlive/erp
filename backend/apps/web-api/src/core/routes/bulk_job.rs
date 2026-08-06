//! 域 D04 `bulk_job` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/bulk-selection-snapshots`、
//! `/admin/background-jobs`；每条路由统一走 JWT + RBAC（`with_permission`），
//! handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::bulk_job, middleware::with_permission},
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
            "/bulk-selection-snapshots",
            with_permission(
                get(bulk_job::bulk_selection_snapshot_list),
                rbac,
                bulk_job::bulk_selection_snapshot_list_permission_key(),
            ),
        )
        .route(
            "/bulk-selection-snapshots",
            with_permission(
                post(bulk_job::bulk_selection_snapshot_create),
                rbac,
                bulk_job::bulk_selection_snapshot_create_permission_key(),
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/confirm",
            with_permission(
                post(bulk_job::bulk_selection_snapshot_confirm),
                rbac,
                bulk_job::bulk_selection_snapshot_confirm_permission_key(),
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/expire",
            with_permission(
                post(bulk_job::bulk_selection_snapshot_expire),
                rbac,
                bulk_job::bulk_selection_snapshot_expire_permission_key(),
            ),
        )
        .route(
            "/bulk-selection-snapshots/{id}/items",
            with_permission(
                get(bulk_job::bulk_selection_item_list),
                rbac,
                bulk_job::bulk_selection_item_list_permission_key(),
            ),
        )
        .route(
            "/background-jobs",
            with_permission(
                get(bulk_job::background_job_list),
                rbac,
                bulk_job::background_job_list_permission_key(),
            ),
        )
        .route(
            "/background-jobs",
            with_permission(
                post(bulk_job::background_job_create),
                rbac,
                bulk_job::background_job_create_permission_key(),
            ),
        )
        .route(
            "/background-jobs/{id}",
            with_permission(
                get(bulk_job::background_job_detail),
                rbac,
                bulk_job::background_job_detail_permission_key(),
            ),
        )
        .route(
            "/background-jobs/{id}/cancel",
            with_permission(
                post(bulk_job::background_job_cancel),
                rbac,
                bulk_job::background_job_cancel_permission_key(),
            ),
        )
        .route(
            "/background-jobs/{id}/items",
            with_permission(
                get(bulk_job::background_job_item_list),
                rbac,
                bulk_job::background_job_item_list_permission_key(),
            ),
        )
}
