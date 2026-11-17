//! 域 D12 `contract` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/contracts`；每条路由统一
//! 走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::contract;
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
    archived_routes()
        .merge(sales_access_routes())
        .merge(template_routes())
        .merge(application_routes())
        .merge(import_routes())
}

fn sales_access_routes() -> Router<HttpState> {
    Router::new().route(
        "/contracts/{id}/sales-access",
        permission_route(
            permission_route(put(contract::contract_sales_access), &permissions::contract::TERMINATE),
            &permissions::customer::ASSIGNMENT_APPLY,
        ),
    )
}

fn archived_routes() -> Router<HttpState> {
    Router::new()
        .merge(file_routes())
        .route("/contracts", permission_route(get(contract::contract_list), &permissions::contract::LIST))
        .route(
            "/contracts/upload",
            permission_route(
                upload::multipart_route(
                    post(contract::imports::upload),
                    upload::MAX_CONTRACT_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::contract::CREATE,
            ),
        )
        .route(
            "/contracts",
            permission_route(post(contract::contract_create), &permissions::contract::CREATE),
        )
        .route(
            "/contracts/{id}",
            permission_route(get(contract::contract_detail), &permissions::contract::DETAIL),
        )
        .route(
            "/contracts/{id}/revisions",
            permission_route(post(contract::contract_archive_revision), &permissions::contract::TERMINATE),
        )
        .route(
            "/contracts/{id}/terminate",
            permission_route(post(contract::contract_terminate), &permissions::contract::TERMINATE),
        )
}

/// Word 模板及本人申请接口的独立权限边界。
fn template_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/contract-templates",
            permission_route(get(contract::templates::list), &permissions::contract::TEMPLATES_LIST),
        )
        .route(
            "/contract-templates",
            permission_route(
                upload::multipart_route(
                    post(contract::templates::create),
                    upload::MAX_CONTRACT_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::contract::TEMPLATES_CREATE,
            ),
        )
        .route(
            "/contract-templates/{id}/status",
            permission_route(post(contract::templates::status), &permissions::contract::TEMPLATES_CREATE),
        )
        .route(
            "/contract-templates/{id}/sample",
            permission_route(get(contract::templates::sample), &permissions::contract::TEMPLATES_CREATE),
        )
        .route(
            "/contract-number-counters",
            permission_route(get(contract::templates::counters), &permissions::contract::TEMPLATES_CREATE),
        )
        .route(
            "/contract-number-counters",
            permission_route(post(contract::templates::configure), &permissions::contract::TEMPLATES_CREATE),
        )
}

fn application_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/contract-applications",
            permission_route(
                get(contract::templates::applications),
                &permissions::contract::TEMPLATES_APPLICATIONS,
            ),
        )
        .route(
            "/contract-applications",
            permission_route(post(contract::templates::apply), &permissions::contract::TEMPLATES_APPLY),
        )
        .route(
            "/contract-applications/{id}/download",
            permission_route(get(contract::templates::download), &permissions::contract::TEMPLATES_DOWNLOAD),
        )
}

fn file_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/contracts/{id}/files/{file_id}",
            permission_route(get(contract::contract_file), &permissions::contract::DETAIL),
        )
        .route(
            "/contracts/{id}/files/{file_id}/preview",
            permission_route(get(contract::contract_file_preview), &permissions::contract::DETAIL),
        )
}

fn import_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/contract-imports",
            permission_route(get(contract::imports::list), &permissions::contract::CREATE),
        )
        .route(
            "/contract-imports/{id}",
            permission_route(get(contract::imports::detail), &permissions::contract::CREATE),
        )
        .route(
            "/contract-imports/{id}/run",
            permission_route(post(contract::imports::run), &permissions::contract::CREATE),
        )
        .route(
            "/contract-imports/{id}/confirm",
            permission_route(post(contract::imports::confirm), &permissions::contract::CREATE),
        )
        .route(
            "/contract-imports/{id}/preview",
            permission_route(get(contract::imports::preview), &permissions::contract::CREATE),
        )
}
