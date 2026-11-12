//! S3-05 M10 客户经营质量双口径管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/customer-quality/current`、
//! `/admin/customer-quality/history` 及各自导出；每条路由统一走 JWT + RBAC。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::customer_quality;
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
            "/customer-quality/history/directory",
            with_permission(
                get(customer_quality::quality_history_directory),
                rbac,
                &permissions::sales_order::LIST,
            ),
        )
        .route(
            "/customer-quality/current",
            with_permission(get(customer_quality::quality_current), rbac, &permissions::customer::LIST),
        )
        .route(
            "/customer-quality/history",
            with_permission(get(customer_quality::quality_history), rbac, &permissions::sales_order::LIST),
        )
        .route(
            "/customer-quality/current/exports",
            with_permission(
                post(customer_quality::quality_current_export),
                rbac,
                &permissions::customer::LIST,
            ),
        )
        .route(
            "/customer-quality/history/exports",
            with_permission(
                post(customer_quality::quality_history_export),
                rbac,
                &permissions::sales_order::LIST,
            ),
        )
}
