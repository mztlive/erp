//! 域 D09 `supplier` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/suppliers`、
//! `/admin/supplier-capabilities/{id}` 等；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier, middleware::with_permission},
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
            "/suppliers",
            with_permission(
                get(supplier::supplier_list),
                rbac,
                supplier::supplier_list_permission_key(),
            ),
        )
        .route(
            "/suppliers",
            with_permission(
                post(supplier::supplier_create),
                rbac,
                supplier::supplier_create_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}",
            with_permission(
                get(supplier::supplier_detail),
                rbac,
                supplier::supplier_detail_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}",
            with_permission(
                put(supplier::supplier_update),
                rbac,
                supplier::supplier_update_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}",
            with_permission(
                delete(supplier::supplier_delete),
                rbac,
                supplier::supplier_delete_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/commercial-profiles",
            with_permission(
                get(supplier::supplier_commercial_profile_list),
                rbac,
                supplier::supplier_commercial_profile_list_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/commercial-profiles",
            with_permission(
                post(supplier::supplier_commercial_profile_create),
                rbac,
                supplier::supplier_commercial_profile_create_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/capabilities",
            with_permission(
                get(supplier::supplier_capability_list),
                rbac,
                supplier::supplier_capability_list_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/capabilities",
            with_permission(
                post(supplier::supplier_capability_create),
                rbac,
                supplier::supplier_capability_create_permission_key(),
            ),
        )
        .route(
            "/supplier-capabilities/{id}",
            with_permission(
                put(supplier::supplier_capability_update),
                rbac,
                supplier::supplier_capability_update_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/qualifications",
            with_permission(
                get(supplier::supplier_qualification_list),
                rbac,
                supplier::supplier_qualification_list_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/qualifications",
            with_permission(
                post(supplier::supplier_qualification_create),
                rbac,
                supplier::supplier_qualification_create_permission_key(),
            ),
        )
        .route(
            "/supplier-qualifications/{id}",
            with_permission(
                put(supplier::supplier_qualification_update),
                rbac,
                supplier::supplier_qualification_update_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/ratings",
            with_permission(
                get(supplier::supplier_rating_list),
                rbac,
                supplier::supplier_rating_list_permission_key(),
            ),
        )
        .route(
            "/suppliers/{id}/ratings",
            with_permission(
                post(supplier::supplier_rating_create),
                rbac,
                supplier::supplier_rating_create_permission_key(),
            ),
        )
}
