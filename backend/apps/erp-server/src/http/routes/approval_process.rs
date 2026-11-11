//! 审批流程定义管理路由。
//!
//! 共享 `routes/mod.rs` / `admin.rs` 接线归 P0-B；本模块由已接线的
//! `approval_instance` 路由在 P3-HTTP 期间 `#[path]` 引入以完成编译。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::approval_instance::approval_process;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

/// 返回定义管理路由。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回
/// 返回目录、版本、详情、草稿、节点、发布、退役与定义期候选人路由。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-processes/catalog",
            with_permission(
                get(approval_process::definition_catalog),
                rbac,
                permissions::approval_process::DEFINITION_CATALOG,
            ),
        )
        .route(
            "/approval-processes/{document_type}/versions",
            with_permission(
                get(approval_process::definition_versions),
                rbac,
                permissions::approval_process::DEFINITION_VERSIONS,
            ),
        )
        .route(
            "/approval-processes/{document_type}/eligible-assignees",
            with_permission(
                get(approval_process::eligible_assignees),
                rbac,
                permissions::approval_process::ELIGIBLE_ASSIGNEES,
            ),
        )
        .route(
            "/approval-process-definitions/drafts",
            with_permission(
                post(approval_process::create_definition_draft),
                rbac,
                permissions::approval_process::CREATE_DEFINITION_DRAFT,
            ),
        )
        .route(
            "/approval-process-definitions/{id}",
            with_permission(
                get(approval_process::definition_detail),
                rbac,
                permissions::approval_process::DEFINITION_DETAIL,
            ),
        )
        .route(
            "/approval-process-definitions/{id}/nodes",
            with_permission(
                put(approval_process::replace_definition_nodes),
                rbac,
                permissions::approval_process::REPLACE_DEFINITION_NODES,
            ),
        )
        .route(
            "/approval-process-definitions/{id}/publish",
            with_permission(
                post(approval_process::publish_definition),
                rbac,
                permissions::approval_process::PUBLISH_DEFINITION,
            ),
        )
        .route(
            "/approval-process-definitions/{id}/retire",
            with_permission(
                post(approval_process::retire_definition),
                rbac,
                permissions::approval_process::RETIRE_DEFINITION,
            ),
        )
}
