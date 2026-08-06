//! 域 D23 `mall_sync` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/mall-sales-sync-jobs`、
//! `/admin/mall-sales-sync-jobs/{id}`、`/admin/mall-sales-sync-jobs/{id}/complete`、
//! `/admin/mall-sales-order-snapshots`、`/admin/mall-sales-sync-cursors`、
//! `/admin/mall-sales-reconciliation-jobs`、`/admin/mall-sales-reconciliation-jobs/{id}/items`、
//! `/admin/mall-sales-reconciliation-items/{id}/resolve`、`/admin/master-mapping-tasks`、
//! `/admin/master-mapping-tasks/{id}/resolve`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::mall_sync, middleware::with_permission},
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
            "/mall-sales-sync-jobs",
            with_permission(
                get(mall_sync::mall_sales_sync_job_list),
                rbac,
                mall_sync::mall_sales_sync_job_list_permission_key(),
            ),
        )
        .route(
            "/mall-sales-sync-jobs",
            with_permission(
                post(mall_sync::mall_sales_sync_job_create),
                rbac,
                mall_sync::mall_sales_sync_job_create_permission_key(),
            ),
        )
        .route(
            "/mall-sales-sync-jobs/{id}",
            with_permission(
                get(mall_sync::mall_sales_sync_job_detail),
                rbac,
                mall_sync::mall_sales_sync_job_detail_permission_key(),
            ),
        )
        .route(
            "/mall-sales-sync-jobs/{id}/complete",
            with_permission(
                post(mall_sync::mall_sales_sync_job_complete),
                rbac,
                mall_sync::mall_sales_sync_job_complete_permission_key(),
            ),
        )
        .route(
            "/mall-sales-order-snapshots",
            with_permission(
                get(mall_sync::mall_sales_order_snapshot_list),
                rbac,
                mall_sync::mall_sales_order_snapshot_list_permission_key(),
            ),
        )
        .route(
            "/mall-sales-order-snapshots",
            with_permission(
                post(mall_sync::mall_sales_order_snapshot_create),
                rbac,
                mall_sync::mall_sales_order_snapshot_create_permission_key(),
            ),
        )
        .route(
            "/mall-sales-sync-cursors",
            with_permission(
                get(mall_sync::mall_sales_sync_cursor_detail),
                rbac,
                mall_sync::mall_sales_sync_cursor_detail_permission_key(),
            ),
        )
        .route(
            "/mall-sales-reconciliation-jobs",
            with_permission(
                get(mall_sync::mall_sales_reconciliation_job_list),
                rbac,
                mall_sync::mall_sales_reconciliation_job_list_permission_key(),
            ),
        )
        .route(
            "/mall-sales-reconciliation-jobs",
            with_permission(
                post(mall_sync::mall_sales_reconciliation_job_create),
                rbac,
                mall_sync::mall_sales_reconciliation_job_create_permission_key(),
            ),
        )
        .route(
            "/mall-sales-reconciliation-jobs/{id}/items",
            with_permission(
                get(mall_sync::mall_sales_reconciliation_item_list),
                rbac,
                mall_sync::mall_sales_reconciliation_item_list_permission_key(),
            ),
        )
        .route(
            "/mall-sales-reconciliation-items/{id}/resolve",
            with_permission(
                post(mall_sync::mall_sales_reconciliation_item_resolve),
                rbac,
                mall_sync::mall_sales_reconciliation_item_resolve_permission_key(),
            ),
        )
        .route(
            "/master-mapping-tasks",
            with_permission(
                get(mall_sync::master_mapping_task_list),
                rbac,
                mall_sync::master_mapping_task_list_permission_key(),
            ),
        )
        .route(
            "/master-mapping-tasks",
            with_permission(
                post(mall_sync::master_mapping_task_create),
                rbac,
                mall_sync::master_mapping_task_create_permission_key(),
            ),
        )
        .route(
            "/master-mapping-tasks/{id}/resolve",
            with_permission(
                post(mall_sync::master_mapping_task_resolve),
                rbac,
                mall_sync::master_mapping_task_resolve_permission_key(),
            ),
        )
}
