//! 域 D09 `supplier` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，供应商资料只通过
//! `/admin/supplier-profiles` 根级命令维护；列表、详情、停用、幂等查询与敏感
//! 字段揭示分别使用独立入口。所有路由统一走 JWT + RBAC。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::supplier;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::http::upload;
use crate::permissions;

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
            "/supplier-profiles/import/jobs",
            with_permission(
                post(supplier::import::supplier_import_submit)
                    .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)),
                rbac,
                permissions::supplier::IMPORT_SUPPLIER_IMPORT_SUBMIT,
            ),
        )
        .route(
            "/supplier-profiles/import/jobs/{id}/failures",
            with_permission(
                get(supplier::import::supplier_import_failures),
                rbac,
                permissions::supplier::IMPORT_SUPPLIER_IMPORT_FAILURES,
            ),
        )
        .route(
            "/supplier-profiles/import",
            with_permission(
                post(supplier::import::supplier_import),
                rbac,
                permissions::supplier::IMPORT_SUPPLIER_IMPORT,
            ),
        )
        .route(
            "/supplier-profiles",
            with_permission(
                post(supplier::supplier_profile_create),
                rbac,
                permissions::supplier::PROFILE_CREATE,
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
                permissions::supplier::PROFILE_CREATE_WITH_ASSETS,
            ),
        )
        .route(
            "/supplier-profiles/{id}",
            with_permission(
                put(supplier::supplier_profile_update),
                rbac,
                permissions::supplier::PROFILE_UPDATE,
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
                permissions::supplier::PROFILE_UPDATE_WITH_ASSETS,
            ),
        )
        .route(
            "/supplier-profile-commands/{idempotency_key}",
            with_permission(
                get(supplier::supplier_profile_command_detail),
                rbac,
                permissions::supplier::PROFILE_COMMAND_DETAIL,
            ),
        )
        .route(
            "/supplier-sensitive-fields/reveal",
            with_permission(
                post(supplier::supplier_sensitive_reveal),
                rbac,
                permissions::supplier::SENSITIVE_REVEAL,
            ),
        )
        .route("/suppliers", with_permission(get(supplier::supplier_list), rbac, permissions::supplier::LIST))
        .route(
            "/suppliers/{id}",
            with_permission(get(supplier::supplier_detail), rbac, permissions::supplier::DETAIL),
        )
        .route(
            "/suppliers/{id}",
            with_permission(delete(supplier::supplier_delete), rbac, permissions::supplier::DELETE),
        )
        .route(
            "/suppliers/{id}/handover",
            with_permission(post(supplier::supplier_handover), rbac, permissions::supplier::HANDOVER),
        )
        .route(
            "/suppliers/{id}/handover-candidates",
            with_permission(
                get(supplier::supplier_handover_candidates),
                rbac,
                permissions::supplier::HANDOVER_CANDIDATES,
            ),
        )
        .route(
            "/suppliers/{id}/capabilities/{capability_id}/handover",
            with_permission(
                post(supplier::supplier_capability_handover),
                rbac,
                permissions::supplier::CAPABILITY_HANDOVER,
            ),
        )
}
