//! 域 D31 `mall_backfill` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为
//! `/admin/mall-consumption-backfill-jobs`、`/admin/mall-consumption-backfill-items`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::mall_backfill, middleware::with_permission},
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
            "/mall-consumption-backfill-jobs",
            with_permission(
                get(mall_backfill::backfill_job_list),
                rbac,
                mall_backfill::backfill_job_list_permission_key(),
            ),
        )
        .route(
            "/mall-consumption-backfill-jobs",
            with_permission(
                post(mall_backfill::backfill_job_create),
                rbac,
                mall_backfill::backfill_job_create_permission_key(),
            ),
        )
        .route(
            "/mall-consumption-backfill-jobs/{id}",
            with_permission(
                get(mall_backfill::backfill_job_detail),
                rbac,
                mall_backfill::backfill_job_detail_permission_key(),
            ),
        )
        .route(
            "/mall-consumption-backfill-jobs/{id}/commands",
            with_permission(
                post(mall_backfill::backfill_job_command),
                rbac,
                mall_backfill::backfill_job_command_permission_key(),
            ),
        )
        .route(
            "/mall-consumption-backfill-items",
            with_permission(
                get(mall_backfill::backfill_item_list),
                rbac,
                mall_backfill::backfill_item_list_permission_key(),
            ),
        )
}
