//! 审批流程定义管理路由。
//!
//! 共享 `routes/mod.rs` / `admin.rs` 接线归 P0-B；本模块由已接线的
//! `approval_instance` 路由在 P3-HTTP 期间 `#[path]` 引入以完成编译。

use axum::{
    routing::{get, post, put},
    Router,
};
use services::iam::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::approval_instance::approval_process, middleware::with_permission},
};

/// 返回定义管理路由。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回
/// 返回目录、版本、详情、草稿、节点、发布、退役与定义期候选人路由。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/approval-processes/catalog",
            with_permission(
                get(approval_process::definition_catalog),
                rbac,
                approval_process::definition_catalog_permission_key(),
            ),
        )
        .route(
            "/approval-processes/{document_type}/versions",
            with_permission(
                get(approval_process::definition_versions),
                rbac,
                approval_process::definition_versions_permission_key(),
            ),
        )
        .route(
            "/approval-processes/{document_type}/eligible-assignees",
            with_permission(
                get(approval_process::eligible_assignees),
                rbac,
                approval_process::eligible_assignees_permission_key(),
            ),
        )
        .route(
            "/approval-process-definitions/drafts",
            with_permission(
                post(approval_process::create_definition_draft),
                rbac,
                approval_process::create_definition_draft_permission_key(),
            ),
        )
        .route(
            "/approval-process-definitions/{id}",
            with_permission(
                get(approval_process::definition_detail),
                rbac,
                approval_process::definition_detail_permission_key(),
            ),
        )
        .route(
            "/approval-process-definitions/{id}/nodes",
            with_permission(
                put(approval_process::replace_definition_nodes),
                rbac,
                approval_process::replace_definition_nodes_permission_key(),
            ),
        )
        .route(
            "/approval-process-definitions/{id}/publish",
            with_permission(
                post(approval_process::publish_definition),
                rbac,
                approval_process::publish_definition_permission_key(),
            ),
        )
        .route(
            "/approval-process-definitions/{id}/retire",
            with_permission(
                post(approval_process::retire_definition),
                rbac,
                approval_process::retire_definition_permission_key(),
            ),
        )
}
