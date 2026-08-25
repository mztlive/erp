//! 域 D09 `supplier` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，供应商资料只通过
//! `/admin/supplier-profiles` 根级命令维护；列表、详情、停用、幂等查询与敏感
//! 字段揭示分别使用独立入口。所有路由统一走 JWT + RBAC。

use axum::{
    routing::{delete, get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::supplier, middleware::with_permission, upload},
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
            "/supplier-profiles",
            with_permission(
                post(supplier::supplier_profile_create),
                rbac,
                supplier::supplier_profile_create_permission_key(),
            ),
        )
        .route(
            "/supplier-profiles/with-assets",
            with_permission(
                upload::multipart_route(
                    post(supplier::supplier_profile_create_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                supplier::supplier_profile_create_with_assets_permission_key(),
            ),
        )
        .route(
            "/supplier-profiles/{id}",
            with_permission(
                put(supplier::supplier_profile_update),
                rbac,
                supplier::supplier_profile_update_permission_key(),
            ),
        )
        .route(
            "/supplier-profiles/{id}/with-assets",
            with_permission(
                upload::multipart_route(
                    put(supplier::supplier_profile_update_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                supplier::supplier_profile_update_with_assets_permission_key(),
            ),
        )
        .route(
            "/supplier-profile-commands/{idempotency_key}",
            with_permission(
                get(supplier::supplier_profile_command_detail),
                rbac,
                supplier::supplier_profile_command_detail_permission_key(),
            ),
        )
        .route(
            "/supplier-sensitive-fields/reveal",
            with_permission(
                post(supplier::supplier_sensitive_reveal),
                rbac,
                supplier::supplier_sensitive_reveal_permission_key(),
            ),
        )
        .route(
            "/suppliers",
            with_permission(
                get(supplier::supplier_list),
                rbac,
                supplier::supplier_list_permission_key(),
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
                delete(supplier::supplier_delete),
                rbac,
                supplier::supplier_delete_permission_key(),
            ),
        )
}
