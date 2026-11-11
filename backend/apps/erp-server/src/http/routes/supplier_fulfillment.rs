//! 域 D32 `supplier_fulfillment` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/supplier-fulfillment-orders`、
//! `/admin/supplier-fulfillment-orders/{id}/refund-facts`；每条路由统一走 JWT + RBAC（`with_permission`），
//! 权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::AuthorizationService;

use crate::http::handler::supplier_fulfillment;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    execution_routes(rbac)
        .merge(order_routes(rbac))
        .merge(order_action_routes(rbac))
        .merge(order_handover_routes(rbac))
}

fn order_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-fulfillment-orders",
            with_permission(
                get(supplier_fulfillment::supplier_fulfillment_order_list),
                rbac,
                permissions::supplier_fulfillment::ORDER_LIST,
            ),
        )
        .route(
            "/supplier-fulfillment-orders",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_submit),
                rbac,
                permissions::supplier_fulfillment::ORDER_SUBMIT,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/investigations",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_investigation),
                rbac,
                permissions::supplier_fulfillment::ORDER_INVESTIGATION,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/task-investigations",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_task_investigation),
                rbac,
                permissions::supplier_fulfillment::ORDER_TASK_INVESTIGATION,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/task-completions",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_task_completion),
                rbac,
                permissions::supplier_fulfillment::ORDER_TASK_COMPLETION,
            ),
        )
}

fn order_action_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-fulfillment-orders/{id}",
            with_permission(
                get(supplier_fulfillment::supplier_fulfillment_order_detail),
                rbac,
                permissions::supplier_fulfillment::ORDER_DETAIL,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/cancel",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_cancel),
                rbac,
                permissions::supplier_fulfillment::ORDER_CANCEL,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/refund",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_refund),
                rbac,
                permissions::supplier_fulfillment::ORDER_REFUND,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/reject",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_reject),
                rbac,
                permissions::supplier_fulfillment::ORDER_REJECT,
            ),
        )
}

fn order_handover_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-fulfillment-orders/{id}/handover",
            with_permission(
                post(supplier_fulfillment::supplier_fulfillment_order_handover),
                rbac,
                permissions::supplier_fulfillment::ORDER_HANDOVER,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/handover-candidates",
            with_permission(
                get(supplier_fulfillment::supplier_fulfillment_order_handover_candidates),
                rbac,
                permissions::supplier_fulfillment::ORDER_HANDOVER_CANDIDATES,
            ),
        )
        .route(
            "/supplier-fulfillment-orders/{id}/refund-facts",
            with_permission(
                post(supplier_fulfillment::supplier_refund_fact_post),
                rbac,
                permissions::supplier_fulfillment::SUPPLIER_REFUND_FACT_POST,
            ),
        )
}

/// 将配送补充路由单独组装，保持读写各自既有动作授权。
fn execution_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    execution_query_routes(rbac).route(
        "/supplier-fulfillment-orders/{id}/execution-setup",
        with_permission(
            get(supplier_fulfillment::execution_setup::supplier_order_execution_setup),
            rbac,
            permissions::supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_SETUP,
        )
        .merge(with_permission(
            post(supplier_fulfillment::execution_setup::save_supplier_order_execution_setup),
            rbac,
            permissions::supplier_fulfillment::EXECUTION_SETUP_SAVE_SUPPLIER_ORDER_EXECUTION_SETUP,
        )),
    )
}

fn execution_query_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route("/supplier-fulfillment-orders/{id}/execution-address-candidates", with_permission(
            get(supplier_fulfillment::execution_setup::supplier_order_execution_address_candidates), rbac,
            permissions::supplier_fulfillment::EXECUTION_SETUP_SUPPLIER_ORDER_EXECUTION_ADDRESS_CANDIDATES,
        ))
        .route("/supplier-fulfillment-orders/{id}/execution-query", with_permission(
            post(supplier_fulfillment::execution_setup::query_supplier_order_execution_result), rbac,
            permissions::supplier_fulfillment::EXECUTION_SETUP_QUERY_SUPPLIER_ORDER_EXECUTION_RESULT,
        ))
}
