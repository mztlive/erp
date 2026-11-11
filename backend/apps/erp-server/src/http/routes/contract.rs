//! 域 D12 `contract` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/contracts`；每条路由统一
//! 走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::contract;
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
    archived_routes(rbac)
        .merge(sales_access_routes(rbac))
        .merge(template_routes(rbac))
        .merge(application_routes(rbac))
        .merge(import_routes(rbac))
}

fn sales_access_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new().route(
        "/contracts/{id}/sales-access",
        with_permission(
            with_permission(put(contract::contract_sales_access), rbac, permissions::contract::SALES_ACCESS),
            rbac,
            permissions::customer::ASSIGNMENT_APPLY,
        ),
    )
}

fn archived_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .merge(file_routes(rbac))
        .route("/contracts", with_permission(get(contract::contract_list), rbac, permissions::contract::LIST))
        .route(
            "/contracts/upload",
            with_permission(
                upload::multipart_route(
                    post(contract::imports::upload),
                    upload::MAX_CONTRACT_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::contract::IMPORTS_UPLOAD,
            ),
        )
        .route(
            "/contracts",
            with_permission(post(contract::contract_create), rbac, permissions::contract::CREATE),
        )
        .route(
            "/contracts/{id}",
            with_permission(get(contract::contract_detail), rbac, permissions::contract::DETAIL),
        )
        .route(
            "/contracts/{id}/revisions",
            with_permission(
                post(contract::contract_archive_revision),
                rbac,
                permissions::contract::ARCHIVE_REVISION,
            ),
        )
        .route(
            "/contracts/{id}/terminate",
            with_permission(post(contract::contract_terminate), rbac, permissions::contract::TERMINATE),
        )
}

/// Word 模板及本人申请接口的独立权限边界。
fn template_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/contract-templates",
            with_permission(get(contract::templates::list), rbac, permissions::contract::TEMPLATES_LIST),
        )
        .route(
            "/contract-templates",
            with_permission(
                upload::multipart_route(
                    post(contract::templates::create),
                    upload::MAX_CONTRACT_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::contract::TEMPLATES_CREATE,
            ),
        )
        .route(
            "/contract-templates/{id}/status",
            with_permission(post(contract::templates::status), rbac, permissions::contract::TEMPLATES_STATUS),
        )
        .route(
            "/contract-templates/{id}/sample",
            with_permission(get(contract::templates::sample), rbac, permissions::contract::TEMPLATES_SAMPLE),
        )
        .route(
            "/contract-number-counters",
            with_permission(
                get(contract::templates::counters),
                rbac,
                permissions::contract::TEMPLATES_COUNTERS,
            ),
        )
        .route(
            "/contract-number-counters",
            with_permission(
                post(contract::templates::configure),
                rbac,
                permissions::contract::TEMPLATES_CONFIGURE,
            ),
        )
}

fn application_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/contract-applications",
            with_permission(
                get(contract::templates::applications),
                rbac,
                permissions::contract::TEMPLATES_APPLICATIONS,
            ),
        )
        .route(
            "/contract-applications",
            with_permission(post(contract::templates::apply), rbac, permissions::contract::TEMPLATES_APPLY),
        )
        .route(
            "/contract-applications/{id}/download",
            with_permission(
                get(contract::templates::download),
                rbac,
                permissions::contract::TEMPLATES_DOWNLOAD,
            ),
        )
}

fn file_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/contracts/{id}/files/{file_id}",
            with_permission(get(contract::contract_file), rbac, permissions::contract::FILE),
        )
        .route(
            "/contracts/{id}/files/{file_id}/preview",
            with_permission(get(contract::contract_file_preview), rbac, permissions::contract::FILE_PREVIEW),
        )
}

fn import_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/contract-imports",
            with_permission(get(contract::imports::list), rbac, permissions::contract::IMPORTS_LIST),
        )
        .route(
            "/contract-imports/{id}",
            with_permission(get(contract::imports::detail), rbac, permissions::contract::IMPORTS_DETAIL),
        )
        .route(
            "/contract-imports/{id}/run",
            with_permission(post(contract::imports::run), rbac, permissions::contract::IMPORTS_RUN),
        )
        .route(
            "/contract-imports/{id}/confirm",
            with_permission(post(contract::imports::confirm), rbac, permissions::contract::IMPORTS_CONFIRM),
        )
        .route(
            "/contract-imports/{id}/preview",
            with_permission(get(contract::imports::preview), rbac, permissions::contract::IMPORTS_PREVIEW),
        )
}
