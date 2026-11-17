//! 审批流程定义管理路由。
//!
//! 共享 `routes/mod.rs` / `admin.rs` 接线归 P0-B；本模块由已接线的
//! `approval_instance` 路由在 P3-HTTP 期间 `#[path]` 引入以完成编译。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::approval_instance::approval_process;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 返回定义管理路由。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回目录、版本、详情、草稿、节点、发布、退役与定义期候选人路由。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    Router::new().merge(catalog_routes()).merge(definition_routes())
}

/// 审批类型、版本与候选读取路由。
fn catalog_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-processes/catalog",
            permission_route(
                get(approval_process::definition_catalog),
                &permissions::approval_process::DEFINITION_DETAIL,
            ),
        )
        .route(
            "/approval-processes/{document_type}/versions",
            permission_route(
                get(approval_process::definition_versions),
                &permissions::approval_process::DEFINITION_DETAIL,
            ),
        )
        .route(
            "/approval-processes/{document_type}/eligible-assignees",
            permission_route(
                get(approval_process::eligible_assignees),
                &permissions::approval_process::DEFINITION_DETAIL,
            ),
        )
}

/// 审批定义草稿、发布及退役路由。
fn definition_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-process-definitions/drafts",
            permission_route(
                post(approval_process::create_definition_draft),
                &permissions::approval_process::CREATE_DEFINITION_DRAFT,
            ),
        )
        .route(
            "/approval-process-definitions/{id}",
            permission_route(
                get(approval_process::definition_detail),
                &permissions::approval_process::DEFINITION_DETAIL,
            ),
        )
        .route(
            "/approval-process-definitions/{id}/nodes",
            permission_route(
                put(approval_process::replace_definition_nodes),
                &permissions::approval_process::REPLACE_DEFINITION_NODES,
            ),
        )
        .route(
            "/approval-process-definitions/{id}/publish",
            permission_route(
                post(approval_process::publish_definition),
                &permissions::approval_process::PUBLISH_DEFINITION,
            ),
        )
        .route(
            "/approval-process-definitions/{id}/retire",
            permission_route(
                post(approval_process::retire_definition),
                &permissions::approval_process::RETIRE_DEFINITION,
            ),
        )
}
