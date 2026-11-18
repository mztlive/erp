//! S3-05 M10 客户经营质量双口径管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/customer-quality/current`、
//! `/admin/customer-quality/history` 及各自导出；每条路由统一走 JWT 认证，业务用例执行授权。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::customer_quality;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载客户资料质量接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-quality/history/directory",
            permission_route(
                get(customer_quality::quality_history_directory),
                &permissions::sales_order::LIST,
            ),
        )
        .route(
            "/customer-quality/current",
            permission_route(get(customer_quality::quality_current), &permissions::customer::LIST),
        )
        .route(
            "/customer-quality/history",
            permission_route(get(customer_quality::quality_history), &permissions::sales_order::LIST),
        )
        .route(
            "/customer-quality/current/exports",
            permission_route(post(customer_quality::quality_current_export), &permissions::customer::LIST),
        )
        .route(
            "/customer-quality/history/exports",
            permission_route(post(customer_quality::quality_history_export), &permissions::sales_order::LIST),
        )
}
