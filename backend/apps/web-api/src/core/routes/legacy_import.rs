//! 域 D22 `legacy_import` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/legacy-import-batches`、
//! `/admin/legacy-import-batches/{id}`、`/admin/legacy-import-batches/{id}/rows`、
//! `/admin/legacy-import-batches/{id}/apply`、`/admin/legacy-import-confirmations`、
//! `/admin/legacy-import-batches/{id}/commands`、`/admin/legacy-import-confirmations/complete`；
//! 每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::legacy_import, middleware::with_permission},
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
            "/legacy-import-batches",
            with_permission(
                get(legacy_import::legacy_import_batch_list),
                rbac,
                legacy_import::legacy_import_batch_list_permission_key(),
            ),
        )
        .route(
            "/legacy-import-batches",
            with_permission(
                post(legacy_import::legacy_import_batch_create),
                rbac,
                legacy_import::legacy_import_batch_create_permission_key(),
            ),
        )
        .route(
            "/legacy-import-batches/{id}",
            with_permission(
                get(legacy_import::legacy_import_batch_detail),
                rbac,
                legacy_import::legacy_import_batch_detail_permission_key(),
            ),
        )
        .route(
            "/legacy-import-batches/{id}/rows",
            with_permission(
                get(legacy_import::legacy_import_row_list),
                rbac,
                legacy_import::legacy_import_row_list_permission_key(),
            ),
        )
        .route(
            "/legacy-import-batches/{id}/apply",
            with_permission(
                post(legacy_import::legacy_import_batch_apply),
                rbac,
                legacy_import::legacy_import_batch_apply_permission_key(),
            ),
        )
        .route(
            "/legacy-import-batches/{id}/commands",
            with_permission(
                post(legacy_import::legacy_import_execution_command),
                rbac,
                legacy_import::legacy_import_execution_command_permission_key(),
            ),
        )
        .route(
            "/legacy-import-confirmations",
            with_permission(
                get(legacy_import::legacy_import_confirmation_list),
                rbac,
                legacy_import::legacy_import_confirmation_list_permission_key(),
            ),
        )
        .route(
            "/legacy-import-confirmations",
            with_permission(
                post(legacy_import::legacy_import_confirmation_create),
                rbac,
                legacy_import::legacy_import_confirmation_create_permission_key(),
            ),
        )
        .route(
            "/legacy-import-confirmations/complete",
            with_permission(
                post(legacy_import::legacy_import_confirmation_complete),
                rbac,
                legacy_import::legacy_import_confirmation_complete_permission_key(),
            ),
        )
}
