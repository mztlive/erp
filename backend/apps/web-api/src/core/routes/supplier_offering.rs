//! 供应商供给管理路由。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::supplier_offering;
use crate::core::middleware::with_permission;

/// 构建供应商供给管理路由。
///
/// # 参数
/// * `rbac` - 授权服务
///
/// # 返回
/// 返回已挂载权限门禁的路由。
///
/// # 错误
/// 无。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().merge(read_routes(rbac)).merge(batch_routes(rbac)).merge(command_routes(rbac))
}

/// 注册供给列表、单条资料和历史条款的读取入口。
fn read_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-offerings/{id}",
            with_permission(
                get(supplier_offering::read::detail),
                rbac,
                supplier_offering::read::detail_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/{id}/revisions",
            with_permission(
                get(supplier_offering::read::history),
                rbac,
                supplier_offering::read::history_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings",
            with_permission(get(supplier_offering::list), rbac, supplier_offering::list_permission_key()),
        )
        .route(
            "/supplier-offerings/{id}/handover-candidates",
            with_permission(
                get(supplier_offering::offering_handover_candidates),
                rbac,
                supplier_offering::offering_handover_candidates_permission_key(),
            ),
        )
}

/// 注册独立的批量新增、修订和可供更新命令。
fn batch_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-offerings/batch/create",
            with_permission(
                post(supplier_offering::batch::create),
                rbac,
                supplier_offering::batch::create_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/batch/revise",
            with_permission(
                post(supplier_offering::batch::revise),
                rbac,
                supplier_offering::batch::revise_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/batch/availability",
            with_permission(
                post(supplier_offering::batch::availability),
                rbac,
                supplier_offering::batch::availability_permission_key(),
            ),
        )
}

/// 注册单条供给维护与任务处置命令。
fn command_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-offerings",
            with_permission(
                post(supplier_offering::create),
                rbac,
                supplier_offering::create_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/{id}/revisions",
            with_permission(
                post(supplier_offering::revise),
                rbac,
                supplier_offering::revise_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/{id}/availability",
            with_permission(
                post(supplier_offering::update_availability),
                rbac,
                supplier_offering::update_availability_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/{id}/supply-exception-task/complete",
            with_permission(
                post(supplier_offering::complete_supply_exception_task),
                rbac,
                supplier_offering::complete_supply_exception_task_permission_key(),
            ),
        )
        .route(
            "/supplier-offerings/{id}/handover",
            with_permission(
                post(supplier_offering::offering_handover),
                rbac,
                supplier_offering::offering_handover_permission_key(),
            ),
        )
}
