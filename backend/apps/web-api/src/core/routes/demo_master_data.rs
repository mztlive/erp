//! 演示主数据路由。

use axum::Router;
use axum::routing::{delete, get, post};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::demo_master_data;
use crate::core::middleware::with_permission;

/// 演示主数据路由。
///
/// # 参数
/// * `rbac` - 授权引擎
///
/// # 返回
/// 返回查看、生成和删除三条路由。
pub(super) fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/demo-master-data",
            with_permission(
                get(demo_master_data::demo_master_data_status),
                rbac,
                demo_master_data::demo_master_data_status_permission_key(),
            ),
        )
        .route(
            "/demo-master-data",
            with_permission(
                post(demo_master_data::demo_master_data_apply),
                rbac,
                demo_master_data::demo_master_data_apply_permission_key(),
            ),
        )
        .route(
            "/demo-master-data/foundation",
            with_permission(
                post(demo_master_data::demo_master_data_foundation),
                rbac,
                demo_master_data::demo_master_data_foundation_permission_key(),
            ),
        )
        .route(
            "/demo-master-data",
            with_permission(
                delete(demo_master_data::demo_master_data_remove),
                rbac,
                demo_master_data::demo_master_data_remove_permission_key(),
            ),
        )
}
