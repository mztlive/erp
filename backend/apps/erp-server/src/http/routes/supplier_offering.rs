//! 供应商供给管理路由。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::supplier_offering;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

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
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .merge(read_routes(rbac))
        .merge(batch_routes(rbac))
        .merge(command_routes(rbac))
        .merge(delivery_template_routes(rbac))
}

/// 注册供应商配送模板维护与供给指定入口。
fn delivery_template_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/suppliers/{supplier_id}/delivery-templates",
            with_permission(
                get(supplier_offering::delivery_template::list),
                rbac,
                permissions::supplier_offering::DELIVERY_TEMPLATE_LIST,
            ),
        )
        .route(
            "/suppliers/{supplier_id}/delivery-templates",
            with_permission(
                post(supplier_offering::delivery_template::create),
                rbac,
                permissions::supplier_offering::DELIVERY_TEMPLATE_CREATE,
            ),
        )
        .route(
            "/supplier-delivery-templates/{id}",
            with_permission(
                put(supplier_offering::delivery_template::update),
                rbac,
                permissions::supplier_offering::DELIVERY_TEMPLATE_UPDATE,
            ),
        )
        .route(
            "/supplier-delivery-templates/{id}",
            with_permission(
                delete(supplier_offering::delivery_template::delete),
                rbac,
                permissions::supplier_offering::DELIVERY_TEMPLATE_DELETE,
            ),
        )
        .route(
            "/supplier-offerings/delivery-template",
            with_permission(
                post(supplier_offering::delivery_template::assign),
                rbac,
                permissions::supplier_offering::DELIVERY_TEMPLATE_ASSIGN,
            ),
        )
}

/// 注册供给列表、单条资料和历史条款的读取入口。
fn read_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-offerings/{id}",
            with_permission(
                get(supplier_offering::read::detail),
                rbac,
                permissions::supplier_offering::READ_DETAIL,
            ),
        )
        .route(
            "/supplier-offerings/{id}/revisions",
            with_permission(
                get(supplier_offering::read::history),
                rbac,
                permissions::supplier_offering::READ_HISTORY,
            ),
        )
        .route(
            "/supplier-offerings",
            with_permission(get(supplier_offering::list), rbac, permissions::supplier_offering::LIST),
        )
        .route(
            "/supplier-offerings/{id}/handover-candidates",
            with_permission(
                get(supplier_offering::offering_handover_candidates),
                rbac,
                permissions::supplier_offering::OFFERING_HANDOVER_CANDIDATES,
            ),
        )
}

/// 注册独立的批量条款修订和可供更新命令。
fn batch_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-offerings/batch/revise",
            with_permission(
                post(supplier_offering::batch::revise),
                rbac,
                permissions::supplier_offering::BATCH_REVISE,
            ),
        )
        .route(
            "/supplier-offerings/batch/availability",
            with_permission(
                post(supplier_offering::batch::availability),
                rbac,
                permissions::supplier_offering::BATCH_AVAILABILITY,
            ),
        )
}

/// 注册单条供给维护与任务处置命令。
fn command_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-offerings",
            with_permission(post(supplier_offering::create), rbac, permissions::supplier_offering::CREATE),
        )
        .route(
            "/supplier-offerings/{id}/revisions",
            with_permission(post(supplier_offering::revise), rbac, permissions::supplier_offering::REVISE),
        )
        .route(
            "/supplier-offerings/{id}/availability",
            with_permission(
                post(supplier_offering::update_availability),
                rbac,
                permissions::supplier_offering::UPDATE_AVAILABILITY,
            ),
        )
        .route(
            "/supplier-offerings/{id}/supply-exception-task/complete",
            with_permission(
                post(supplier_offering::complete_supply_exception_task),
                rbac,
                permissions::supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK,
            ),
        )
        .route(
            "/supplier-offerings/{id}/handover",
            with_permission(
                post(supplier_offering::offering_handover),
                rbac,
                permissions::supplier_offering::OFFERING_HANDOVER,
            ),
        )
}
