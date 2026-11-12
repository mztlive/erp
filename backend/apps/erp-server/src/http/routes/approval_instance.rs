//! 审批实例、决定、恢复与受阻取消路由。
//!
//! 已删除 `POST /approval-instances/{id}/recover`。定义管理路由经 `#[path]` 合并，
//! P0-B 接线 `routes/mod.rs` 后应改为独立 merge。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;
use erp_permissions as permissions;

use crate::http::handler::approval_instance;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;

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
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .merge(approval_process::routes(rbac))
        .merge(material_routes(rbac))
        .merge(copy_routes(rbac))
        .merge(query_routes(rbac))
        .merge(progress_routes(rbac))
        .merge(command_routes(rbac))
}

/// 提交资料和冻结附件读取路由。
fn material_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-instances/{id}/invoice-review",
            with_permission(
                get(approval_instance::materials::invoice_review),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/materials",
            with_permission(
                get(approval_instance::materials::list),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/materials/{file_asset_id}/preview",
            with_permission(
                get(approval_instance::materials::preview),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/materials/{file_asset_id}/download",
            with_permission(
                get(approval_instance::materials::download),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
}

/// 审批实例列表、详情、历史与恢复选项。
fn query_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-instances/{id}/timeline",
            with_permission(
                get(approval_instance::timeline::approval_timeline),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances",
            with_permission(
                get(approval_instance::instance_list),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}",
            with_permission(
                get(approval_instance::instance_detail),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/history",
            with_permission(
                get(approval_instance::instance_history),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/recovery-options",
            with_permission(
                get(approval_instance::recovery_options),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
}

/// 审批决定及管理写命令。
fn command_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-decisions",
            with_permission(
                post(approval_instance::submit_decision),
                rbac,
                &permissions::approval_instance::SUBMIT_DECISION,
            ),
        )
        .route(
            "/approval-instances/{id}/resume-current-approver",
            with_permission(
                post(approval_instance::resume_current_approver),
                rbac,
                &permissions::approval_instance::RESUME_CURRENT_APPROVER,
            ),
        )
        .route(
            "/approval-instances/{id}/cancel-blocked",
            with_permission(
                post(approval_instance::cancel_blocked),
                rbac,
                &permissions::approval_instance::CANCEL_BLOCKED,
            ),
        )
        .route(
            "/business-documents/{document_type}/{id}/approval-definition/upgrade",
            with_permission(
                post(approval_instance::upgrade_binding),
                rbac,
                &permissions::approval_instance::UPGRADE_BINDING,
            ),
        )
}

fn copy_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new().route(
        "/approval-instances/{id}/copies",
        with_permission(
            get(approval_instance::copies::history),
            rbac,
            &permissions::approval_instance::INSTANCE_LIST,
        ),
    )
}

/// 发起人个人提醒复用实例读取权限，并在服务层校验本人身份。
fn progress_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-instances/started-unread-count",
            with_permission(
                get(approval_instance::progress::count),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/progress-read",
            with_permission(
                post(approval_instance::progress::mark_read),
                rbac,
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
}
