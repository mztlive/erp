//! 演示主数据路由。

use axum::Router;
use axum::routing::{delete, get, post};
use erp_identity::SharedRbacService;

use crate::http::handler::demo_master_data;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

/// 演示主数据路由。
///
/// # 参数
/// * `rbac` - 授权引擎
///
/// # 返回
/// 返回查看、生成和删除三条路由。
pub(super) fn routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/demo-master-data",
            with_permission(
                get(demo_master_data::demo_master_data_status),
                rbac,
                permissions::demo_master_data::STATUS,
            ),
        )
        .route(
            "/demo-master-data",
            with_permission(
                post(demo_master_data::demo_master_data_apply),
                rbac,
                permissions::demo_master_data::APPLY,
            ),
        )
        .route(
            "/demo-master-data/foundation",
            with_permission(
                post(demo_master_data::demo_master_data_foundation),
                rbac,
                permissions::demo_master_data::FOUNDATION,
            ),
        )
        .route(
            "/demo-master-data",
            with_permission(
                delete(demo_master_data::demo_master_data_remove),
                rbac,
                permissions::demo_master_data::REMOVE,
            ),
        )
}
