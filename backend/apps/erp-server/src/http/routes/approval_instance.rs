//! 审批实例、决定、恢复与受阻取消路由。
//!
//! 已删除 `POST /approval-instances/{id}/recover`。定义管理路由经 `#[path]` 合并，
//! P0-B 接线 `routes/mod.rs` 后应改为独立 merge。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::approval_instance;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// P0-B 声明 `mod approval_process` 后应删除此 `#[path]`。
#[path = "approval_process.rs"]
mod approval_process;

/// 返回审批实例与定义管理路由。
/// 返回合同 §3 运行 API 与定义管理 API。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(approval_process::routes())
        .merge(material_routes())
        .merge(copy_routes())
        .merge(query_routes())
        .merge(progress_routes())
        .merge(command_routes())
}

// 为父路由挂载提交资料和冻结附件读取路由。
fn material_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-instances/materials/batch",
            permission_route(
                post(approval_instance::materials::batch),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/invoice-review",
            permission_route(
                get(approval_instance::materials::invoice_review),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/materials",
            permission_route(
                get(approval_instance::materials::list),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/materials/{file_asset_id}/preview",
            permission_route(
                get(approval_instance::materials::preview),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/materials/{file_asset_id}/download",
            permission_route(
                get(approval_instance::materials::download),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
}

// 为审批工作台挂载实例列表、详情、历史及受阻恢复选项的只读入口。
fn query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-instances/{id}/timeline",
            permission_route(
                get(approval_instance::timeline::approval_timeline),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances",
            permission_route(
                get(approval_instance::instance_list),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}",
            permission_route(
                get(approval_instance::instance_detail),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/history",
            permission_route(
                get(approval_instance::instance_history),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/recovery-options",
            permission_route(
                get(approval_instance::recovery_options),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
}

// 挂载审批任务决定与管理员运行维护接口，供开放任务办理和受阻处置使用。
fn command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-decisions",
            permission_route(
                post(approval_instance::submit_decision),
                &permissions::approval_instance::SUBMIT_DECISION,
            ),
        )
        .route(
            "/approval-instances/{id}/resume-current-approver",
            permission_route(
                post(approval_instance::resume_current_approver),
                &permissions::approval_instance::RESUME_CURRENT_APPROVER,
            ),
        )
        .route(
            "/approval-instances/{id}/cancel-blocked",
            permission_route(
                post(approval_instance::cancel_blocked),
                &permissions::approval_instance::CANCEL_BLOCKED,
            ),
        )
        .route(
            "/business-documents/{document_type}/{id}/approval-definition/upgrade",
            permission_route(
                post(approval_instance::upgrade_binding),
                &permissions::approval_instance::UPGRADE_BINDING,
            ),
        )
}

fn copy_routes() -> Router<HttpState> {
    Router::new().route(
        "/approval-instances/{id}/copies",
        permission_route(
            get(approval_instance::copies::history),
            &permissions::approval_instance::INSTANCE_LIST,
        ),
    )
}

// 挂载发起人个人进展提醒，沿用实例读取资格；只经服务层核验后交付本人记录。
fn progress_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/approval-instances/started-unread-count",
            permission_route(
                get(approval_instance::progress::count),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
        .route(
            "/approval-instances/{id}/progress-read",
            permission_route(
                post(approval_instance::progress::mark_read),
                &permissions::approval_instance::INSTANCE_LIST,
            ),
        )
}
