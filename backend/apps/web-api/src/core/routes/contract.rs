//! 域 D12 `contract` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/contracts`；每条路由统一
//! 走 JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::contract;
use crate::core::middleware::with_permission;
use crate::core::upload;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    archived_routes(rbac).merge(template_routes(rbac)).merge(application_routes(rbac))
}

fn archived_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .merge(file_routes(rbac))
        .route(
            "/contracts",
            with_permission(get(contract::contract_list), rbac, contract::contract_list_permission_key()),
        )
        .route(
            "/contracts/upload",
            with_permission(
                upload::multipart_route(
                    post(contract::contract_upload),
                    upload::MAX_CONTRACT_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                contract::contract_upload_permission_key(),
            ),
        )
        .route(
            "/contracts",
            with_permission(
                post(contract::contract_create),
                rbac,
                contract::contract_create_permission_key(),
            ),
        )
        .route(
            "/contracts/{id}",
            with_permission(get(contract::contract_detail), rbac, contract::contract_detail_permission_key()),
        )
        .route(
            "/contracts/{id}/revisions",
            with_permission(
                post(contract::contract_archive_revision),
                rbac,
                contract::contract_archive_revision_permission_key(),
            ),
        )
        .route(
            "/contracts/{id}/terminate",
            with_permission(
                post(contract::contract_terminate),
                rbac,
                contract::contract_terminate_permission_key(),
            ),
        )
}

/// Word 模板及本人申请接口的独立权限边界。
fn template_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/contract-templates",
            with_permission(get(contract::templates::list), rbac, contract::templates::list_permission_key()),
        )
        .route(
            "/contract-templates",
            with_permission(
                upload::multipart_route(
                    post(contract::templates::create),
                    upload::MAX_CONTRACT_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                contract::templates::create_permission_key(),
            ),
        )
        .route(
            "/contract-templates/{id}/status",
            with_permission(
                post(contract::templates::status),
                rbac,
                contract::templates::status_permission_key(),
            ),
        )
        .route(
            "/contract-templates/{id}/sample",
            with_permission(
                get(contract::templates::sample),
                rbac,
                contract::templates::sample_permission_key(),
            ),
        )
        .route(
            "/contract-number-counters",
            with_permission(
                get(contract::templates::counters),
                rbac,
                contract::templates::counters_permission_key(),
            ),
        )
        .route(
            "/contract-number-counters",
            with_permission(
                post(contract::templates::configure),
                rbac,
                contract::templates::configure_permission_key(),
            ),
        )
}

fn application_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/contract-applications",
            with_permission(
                get(contract::templates::applications),
                rbac,
                contract::templates::applications_permission_key(),
            ),
        )
        .route(
            "/contract-applications",
            with_permission(
                post(contract::templates::apply),
                rbac,
                contract::templates::apply_permission_key(),
            ),
        )
        .route(
            "/contract-applications/{id}/download",
            with_permission(
                get(contract::templates::download),
                rbac,
                contract::templates::download_permission_key(),
            ),
        )
}

fn file_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/contracts/{id}/files/{file_id}",
            with_permission(get(contract::contract_file), rbac, contract::contract_file_permission_key()),
        )
        .route(
            "/contracts/{id}/files/{file_id}/preview",
            with_permission(
                get(contract::contract_file_preview),
                rbac,
                contract::contract_file_preview_permission_key(),
            ),
        )
}
