//! 域 D28 `card_instance` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/consumption-cutovers`、
//! `/admin/card-instances`；每条路由统一走 JWT + RBAC（`with_permission`），
//! handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::card_instance, middleware::with_permission},
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
            "/consumption-cutovers",
            with_permission(
                get(card_instance::cutover_list),
                rbac,
                card_instance::cutover_list_permission_key(),
            ),
        )
        .route(
            "/consumption-cutovers",
            with_permission(
                post(card_instance::cutover_create),
                rbac,
                card_instance::cutover_create_permission_key(),
            ),
        )
        .route(
            "/consumption-cutovers/{id}",
            with_permission(
                get(card_instance::cutover_detail),
                rbac,
                card_instance::cutover_detail_permission_key(),
            ),
        )
        .route(
            "/consumption-cutovers/{id}/enable",
            with_permission(
                put(card_instance::cutover_enable),
                rbac,
                card_instance::cutover_enable_permission_key(),
            ),
        )
        .route(
            "/card-instances",
            with_permission(
                get(card_instance::card_instance_list),
                rbac,
                card_instance::card_instance_list_permission_key(),
            ),
        )
        .route(
            "/card-instances",
            with_permission(
                post(card_instance::card_instance_create),
                rbac,
                card_instance::card_instance_create_permission_key(),
            ),
        )
        .route(
            "/card-instances/{id}",
            with_permission(
                get(card_instance::card_instance_detail),
                rbac,
                card_instance::card_instance_detail_permission_key(),
            ),
        )
        .route(
            "/card-instances/{id}/balance-snapshots",
            with_permission(
                get(card_instance::balance_snapshot_list),
                rbac,
                card_instance::balance_snapshot_list_permission_key(),
            ),
        )
        .route(
            "/card-instances/{id}/balance-snapshots",
            with_permission(
                post(card_instance::balance_snapshot_create),
                rbac,
                card_instance::balance_snapshot_create_permission_key(),
            ),
        )
        .route(
            "/card-instances/{id}/corrections",
            with_permission(
                get(card_instance::correction_list),
                rbac,
                card_instance::correction_list_permission_key(),
            ),
        )
}
