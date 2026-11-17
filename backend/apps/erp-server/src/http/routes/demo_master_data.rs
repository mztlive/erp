//! 演示主数据路由。

use axum::Router;
use axum::routing::{delete, get, post};
use erp_permissions as permissions;

use crate::http::handler::demo_master_data;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 演示主数据路由。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回查看、生成和删除三条路由。
pub(super) fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/demo-master-data",
            permission_route(
                get(demo_master_data::demo_master_data_status),
                &permissions::demo_master_data::STATUS,
            ),
        )
        .route(
            "/demo-master-data",
            permission_route(
                post(demo_master_data::demo_master_data_apply),
                &permissions::demo_master_data::APPLY,
            ),
        )
        .route(
            "/demo-master-data/foundation",
            permission_route(
                post(demo_master_data::demo_master_data_foundation),
                &permissions::demo_master_data::APPLY,
            ),
        )
        .route(
            "/demo-master-data",
            permission_route(
                delete(demo_master_data::demo_master_data_remove),
                &permissions::demo_master_data::REMOVE,
            ),
        )
}
