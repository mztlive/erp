//! 审批实例、决定、恢复与受阻取消路由。
//!
//! 已删除 `POST /approval-instances/{id}/recover`。定义管理路由经 `#[path]` 合并，
//! P0-B 接线 `routes/mod.rs` 后应改为独立 merge。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::approval_instance;
use crate::core::middleware::with_permission;

/// P0-B 声明 `mod approval_process` 后应删除此 `#[path]`。
#[path = "approval_process.rs"]
mod approval_process;

/// 返回审批实例与定义管理路由。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
///
/// # 返回
/// 返回合同 §3 运行 API 与定义管理 API。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .merge(approval_process::routes(rbac))
        .merge(material_routes(rbac))
        .merge(copy_routes(rbac))
        .merge(query_routes(rbac))
        .merge(command_routes(rbac))
}

/// 提交资料和冻结附件读取路由。
fn material_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/approval-instances/{id}/invoice-review",
            with_permission(
                get(approval_instance::materials::invoice_review),
                rbac,
                approval_instance::materials::invoice_review_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/materials",
            with_permission(
                get(approval_instance::materials::list),
                rbac,
                approval_instance::materials::list_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/materials/{file_asset_id}/preview",
            with_permission(
                get(approval_instance::materials::preview),
                rbac,
                approval_instance::materials::preview_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/materials/{file_asset_id}/download",
            with_permission(
                get(approval_instance::materials::download),
                rbac,
                approval_instance::materials::download_permission_key(),
            ),
        )
}

/// 审批实例列表、详情、历史与恢复选项。
fn query_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/approval-instances",
            with_permission(
                get(approval_instance::instance_list),
                rbac,
                approval_instance::instance_list_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}",
            with_permission(
                get(approval_instance::instance_detail),
                rbac,
                approval_instance::instance_detail_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/history",
            with_permission(
                get(approval_instance::instance_history),
                rbac,
                approval_instance::instance_history_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/recovery-options",
            with_permission(
                get(approval_instance::recovery_options),
                rbac,
                approval_instance::recovery_options_permission_key(),
            ),
        )
}

/// 审批决定及管理写命令。
fn command_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/approval-decisions",
            with_permission(
                post(approval_instance::submit_decision),
                rbac,
                approval_instance::submit_decision_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/resume-current-approver",
            with_permission(
                post(approval_instance::resume_current_approver),
                rbac,
                approval_instance::resume_current_approver_permission_key(),
            ),
        )
        .route(
            "/approval-instances/{id}/cancel-blocked",
            with_permission(
                post(approval_instance::cancel_blocked),
                rbac,
                approval_instance::cancel_blocked_permission_key(),
            ),
        )
        .route(
            "/business-documents/{document_type}/{id}/approval-definition/upgrade",
            with_permission(
                post(approval_instance::upgrade_binding),
                rbac,
                approval_instance::upgrade_binding_permission_key(),
            ),
        )
}

fn copy_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/approval-instances/{id}/copies",
            with_permission(
                get(approval_instance::copies::history),
                rbac,
                approval_instance::copies::history_permission_key(),
            ),
        )
        .route(
            "/approval-copies",
            with_permission(
                get(approval_instance::copies::list),
                rbac,
                approval_instance::copies::list_permission_key(),
            ),
        )
        .route(
            "/approval-copies/read",
            with_permission(
                post(approval_instance::copies::mark_read),
                rbac,
                approval_instance::copies::mark_read_permission_key(),
            ),
        )
        .route(
            "/approval-copies/{id}",
            with_permission(
                get(approval_instance::copies::detail),
                rbac,
                approval_instance::copies::detail_permission_key(),
            ),
        )
        .route(
            "/approval-copies/{id}/materials/{file}/preview",
            with_permission(
                get(approval_instance::copies::preview),
                rbac,
                approval_instance::copies::preview_permission_key(),
            ),
        )
        .route(
            "/approval-copies/{id}/materials/{file}/download",
            with_permission(
                get(approval_instance::copies::download),
                rbac,
                approval_instance::copies::download_permission_key(),
            ),
        )
}
