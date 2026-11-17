//! 域 D09 `supplier` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，供应商资料只通过
//! `/admin/supplier-profiles` 根级命令维护；列表、详情、停用、幂等查询与敏感
//! 字段揭示分别使用独立入口。所有路由统一走 JWT 认证，业务用例执行授权。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_permissions as permissions;

use crate::http::handler::supplier;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
use crate::http::upload;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回声明公共权限目录信息的路由集合。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    Router::new().merge(import_routes()).merge(profile_routes()).merge(supplier_routes())
}

/// 供应商资料导入路由。
fn import_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-profiles/import/jobs",
            permission_route(
                post(supplier::import::supplier_import_submit)
                    .layer(axum::extract::DefaultBodyLimit::max(10 * 1024 * 1024)),
                &permissions::supplier::IMPORT_SUPPLIER_IMPORT,
            ),
        )
        .route(
            "/supplier-profiles/import/jobs/{id}/failures",
            permission_route(
                get(supplier::import::supplier_import_failures),
                &permissions::supplier::IMPORT_SUPPLIER_IMPORT,
            ),
        )
        .route(
            "/supplier-profiles/import",
            permission_route(
                post(supplier::import::supplier_import),
                &permissions::supplier::IMPORT_SUPPLIER_IMPORT,
            ),
        )
}

/// 供应商资料与敏感字段维护路由。
fn profile_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-profiles",
            permission_route(
                post(supplier::supplier_profile_create),
                &permissions::supplier::IMPORT_SUPPLIER_IMPORT,
            ),
        )
        .route(
            "/supplier-profiles/with-assets",
            permission_route(
                upload::multipart_route(
                    post(supplier::supplier_profile_create_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::supplier::IMPORT_SUPPLIER_IMPORT,
            ),
        )
        .route(
            "/supplier-profiles/{id}",
            permission_route(put(supplier::supplier_profile_update), &permissions::supplier::HANDOVER),
        )
        .route(
            "/supplier-profiles/{id}/with-assets",
            permission_route(
                upload::multipart_route(
                    put(supplier::supplier_profile_update_with_assets),
                    upload::MAX_BATCH_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::supplier::HANDOVER,
            ),
        )
        .route(
            "/supplier-profile-commands/{idempotency_key}",
            permission_route(get(supplier::supplier_profile_command_detail), &permissions::supplier::DETAIL),
        )
        .route(
            "/supplier-sensitive-fields/reveal",
            permission_route(
                post(supplier::supplier_sensitive_reveal),
                &permissions::supplier::SENSITIVE_REVEAL,
            ),
        )
}

/// 供应商档案与责任交接路由。
fn supplier_routes() -> Router<HttpState> {
    Router::new()
        .route("/suppliers", permission_route(get(supplier::supplier_list), &permissions::supplier::LIST))
        .route(
            "/suppliers/{id}",
            permission_route(get(supplier::supplier_detail), &permissions::supplier::DETAIL),
        )
        .route(
            "/suppliers/{id}",
            permission_route(delete(supplier::supplier_delete), &permissions::supplier::DELETE),
        )
        .route(
            "/suppliers/{id}/handover",
            permission_route(post(supplier::supplier_handover), &permissions::supplier::HANDOVER),
        )
        .route(
            "/suppliers/{id}/handover-candidates",
            permission_route(get(supplier::supplier_handover_candidates), &permissions::supplier::HANDOVER),
        )
        .route(
            "/suppliers/{id}/capabilities/{capability_id}/handover",
            permission_route(post(supplier::supplier_capability_handover), &permissions::supplier::HANDOVER),
        )
}
