//! 域 D32 `supplier_fulfillment` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-fulfillment-orders`、
//! `/admin/supplier-fulfillment-orders/{id}/refund-facts`；每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，
//! 权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::supplier_fulfillment;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

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
    execution_routes().merge(order_routes()).merge(order_action_routes()).merge(order_handover_routes())
}

fn order_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-fulfillment-orders",
            permission_route(
                get(supplier_fulfillment::supplier_fulfillment_order_list),
                &permissions::supplier_fulfillment::ORDER_LIST,
            ),
        )
        .route(
            "/supplier-fulfillment-orders",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_submit),
                &permissions::supplier_fulfillment::ORDER_SUBMIT,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/investigations",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_investigation),
                &permissions::supplier_fulfillment::ORDER_INVESTIGATION,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/task-investigations",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_task_investigation),
                &permissions::supplier_fulfillment::ORDER_INVESTIGATION,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/task-completions",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_task_completion),
                &permissions::supplier_fulfillment::ORDER_TASK_COMPLETION,
            ),
        )
}

fn order_action_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-fulfillment-orders/{id}",
            permission_route(
                get(supplier_fulfillment::supplier_fulfillment_order_detail),
                &permissions::supplier_fulfillment::ORDER_DETAIL,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/cancel",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_cancel),
                &permissions::supplier_fulfillment::ORDER_CANCEL,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/refund",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_refund),
                &permissions::supplier_fulfillment::ORDER_REFUND,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/reject",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_reject),
                &permissions::supplier_fulfillment::ORDER_REJECT,
            ),
        )
}

fn order_handover_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-fulfillment-orders/{id}/handover",
            permission_route(
                post(supplier_fulfillment::supplier_fulfillment_order_handover),
                &permissions::supplier_fulfillment::ORDER_HANDOVER,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/handover-candidates",
            permission_route(
                get(supplier_fulfillment::supplier_fulfillment_order_handover_candidates),
                &permissions::supplier_fulfillment::ORDER_HANDOVER,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/refund-facts",
            permission_route(
                post(supplier_fulfillment::supplier_refund_fact_post),
                &permissions::supplier_fulfillment::SUPPLIER_REFUND_FACT_POST,
            ),
        )
}

/// 将配送补充路由单独组装，保持读写各自既有动作授权。
fn execution_routes() -> Router<HttpState> {
    execution_query_routes().route(
        "/supplier-fulfillment-orders/{id}/execution-setup",
        permission_route(
            get(supplier_fulfillment::execution_setup::supplier_order_execution_setup),
            &permissions::supplier_fulfillment::ORDER_DETAIL,
        )
        .merge(permission_route(
            post(supplier_fulfillment::execution_setup::save_supplier_order_execution_setup),
            &permissions::supplier_fulfillment::ORDER_SUBMIT,
        )),
    )
}

fn execution_query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-fulfillment-orders/{id}/execution-address-candidates",
            permission_route(
                get(supplier_fulfillment::execution_setup::supplier_order_execution_address_candidates),
                &permissions::supplier_fulfillment::ORDER_SUBMIT,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/execution-query",
            permission_route(
                post(supplier_fulfillment::execution_setup::query_supplier_order_execution_result),
                &permissions::supplier_fulfillment::ORDER_SUBMIT,
            ),
        )
}
