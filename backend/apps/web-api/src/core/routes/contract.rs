//! 域 D12 `contract` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/contracts`；每条路由统一
//! 走 JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::contract, middleware::with_permission, upload},
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
            "/contracts",
            with_permission(
                get(contract::contract_list),
                rbac,
                contract::contract_list_permission_key(),
            ),
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
            with_permission(
                get(contract::contract_detail),
                rbac,
                contract::contract_detail_permission_key(),
            ),
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
