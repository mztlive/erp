//! 域 D22 `legacy_import` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/legacy-import-batches`、
//! `/admin/legacy-import-batches/{id}`、`/admin/legacy-import-batches/{id}/rows`、
//! `/admin/legacy-import-batches/{id}/apply`、`/admin/legacy-import-confirmations`、
//! `/admin/legacy-import-batches/{id}/commands`、`/admin/legacy-import-confirmations/complete`；
//! 每条路由统一走
//! JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::legacy_import;
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
            "/legacy-import-batches",
            with_permission(
                get(legacy_import::legacy_import_batch_list),
                rbac,
                &permissions::legacy_import::BATCH_LIST,
            ),
        )
        .route(
            "/legacy-import-batches",
            with_permission(
                post(legacy_import::legacy_import_batch_create),
                rbac,
                &permissions::legacy_import::BATCH_CREATE,
            ),
        )
        .route(
            "/legacy-import-batches/{id}",
            with_permission(
                get(legacy_import::legacy_import_batch_detail),
                rbac,
                &permissions::legacy_import::BATCH_DETAIL,
            ),
        )
        .route(
            "/legacy-import-batches/{id}/rows",
            with_permission(
                get(legacy_import::legacy_import_row_list),
                rbac,
                &permissions::legacy_import::ROW_LIST,
            ),
        )
        .route(
            "/legacy-import-batches/{id}/apply",
            with_permission(
                post(legacy_import::legacy_import_batch_apply),
                rbac,
                &permissions::legacy_import::BATCH_APPLY,
            ),
        )
        .route(
            "/legacy-import-batches/{id}/commands",
            with_permission(
                post(legacy_import::legacy_import_execution_command),
                rbac,
                &permissions::legacy_import::EXECUTION_COMMAND,
            ),
        )
        .route(
            "/legacy-import-confirmations",
            with_permission(
                get(legacy_import::legacy_import_confirmation_list),
                rbac,
                &permissions::legacy_import::CONFIRMATION_LIST,
            ),
        )
        .route(
            "/legacy-import-confirmations",
            with_permission(
                post(legacy_import::legacy_import_confirmation_create),
                rbac,
                &permissions::legacy_import::CONFIRMATION_CREATE,
            ),
        )
        .route(
            "/legacy-import-confirmations/complete",
            with_permission(
                post(legacy_import::legacy_import_confirmation_complete),
                rbac,
                &permissions::legacy_import::CONFIRMATION_COMPLETE,
            ),
        )
}
