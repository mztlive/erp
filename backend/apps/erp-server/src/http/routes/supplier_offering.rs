//! 供应商供给管理路由。

use axum::Router;
use axum::routing::{delete, get, post, put};
use erp_permissions as permissions;

use crate::http::handler::supplier_offering;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载供应商供给与配送模板接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(read_routes())
        .merge(batch_routes())
        .merge(command_routes())
        .merge(delivery_template_routes())
}

// 把供应商配送模板的维护入口与供给分配入口一起接入管理端。
fn delivery_template_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/suppliers/{supplier_id}/delivery-templates",
            permission_route(
                get(supplier_offering::delivery_template::list),
                &permissions::supplier_offering::LIST,
            ),
        )
        .route(
            "/suppliers/{supplier_id}/delivery-templates",
            permission_route(
                post(supplier_offering::delivery_template::create),
                &permissions::supplier_offering::REVISE,
            ),
        )
        .route(
            "/supplier-delivery-templates/{id}",
            permission_route(
                put(supplier_offering::delivery_template::update),
                &permissions::supplier_offering::REVISE,
            ),
        )
        .route(
            "/supplier-delivery-templates/{id}",
            permission_route(
                delete(supplier_offering::delivery_template::delete),
                &permissions::supplier_offering::REVISE,
            ),
        )
        .route(
            "/supplier-offerings/delivery-template",
            permission_route(
                post(supplier_offering::delivery_template::assign),
                &permissions::supplier_offering::REVISE,
            ),
        )
}

// 挂载供给目录、单条资料以及历史条款的只读接口，供管理端查看与核对。
fn read_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-offerings/{id}",
            permission_route(get(supplier_offering::read::detail), &permissions::supplier_offering::LIST),
        )
        .route(
            "/supplier-offerings/{id}/revisions",
            permission_route(get(supplier_offering::read::history), &permissions::supplier_offering::LIST),
        )
        .route(
            "/supplier-offerings",
            permission_route(get(supplier_offering::list), &permissions::supplier_offering::LIST),
        )
        .route(
            "/supplier-offerings/{id}/handover-candidates",
            permission_route(
                get(supplier_offering::offering_handover_candidates),
                &permissions::supplier_offering::REVISE,
            ),
        )
}

// 挂载批量修订条款和更新可供状态的专用写入口，不混入单项供给维护。
fn batch_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-offerings/batch/revise",
            permission_route(post(supplier_offering::batch::revise), &permissions::supplier_offering::REVISE),
        )
        .route(
            "/supplier-offerings/batch/availability",
            permission_route(
                post(supplier_offering::batch::availability),
                &permissions::supplier_offering::BATCH_AVAILABILITY,
            ),
        )
}

// 组装面向单条供应商供给的维护和任务处置入口。
fn command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-offerings",
            permission_route(post(supplier_offering::create), &permissions::supplier_offering::CREATE),
        )
        .route(
            "/supplier-offerings/{id}/revisions",
            permission_route(post(supplier_offering::revise), &permissions::supplier_offering::REVISE),
        )
        .route(
            "/supplier-offerings/{id}/availability",
            permission_route(
                post(supplier_offering::update_availability),
                &permissions::supplier_offering::BATCH_AVAILABILITY,
            ),
        )
        .route(
            "/supplier-offerings/{id}/supply-exception-task/complete",
            permission_route(
                post(supplier_offering::complete_supply_exception_task),
                &permissions::supplier_offering::COMPLETE_SUPPLY_EXCEPTION_TASK,
            ),
        )
        .route(
            "/supplier-offerings/{id}/handover",
            permission_route(
                post(supplier_offering::offering_handover),
                &permissions::supplier_offering::REVISE,
            ),
        )
}
