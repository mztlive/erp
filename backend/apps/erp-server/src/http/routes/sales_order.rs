//! 域 D13 `sales_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-orders`；每条路由
//! 统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::fulfillment::address_arrangement;
use crate::http::handler::sales_order;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 为管理端挂载销售单与销售凭证接口，按操作登记公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/delivery-arrangements",
            permission_route(get(address_arrangement::sales_get), &permissions::sales_order::DETAIL),
        )
        .route(
            "/sales-orders/{id}/delivery-arrangements",
            permission_route(put(address_arrangement::sales_save), &permissions::sales_order::HANDOVER),
        )
        .merge(evidence_contract_routes())
        .merge(read_create_routes())
        .merge(lifecycle_routes())
}

// 挂载销售单查询、方案预填和建单入口，各自关联原有的资源动作权限。
fn read_create_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/timeline",
            permission_route(get(sales_order::timeline::sales_timeline), &permissions::sales_order::DETAIL),
        )
        .route(
            "/sales-orders/selection-proposals/{id}/basis",
            permission_route(get(sales_order::selection::selection_basis), &permissions::sales_order::CREATE),
        )
        .route(
            "/sales-orders",
            permission_route(get(sales_order::sales_order_list), &permissions::sales_order::LIST),
        )
        .route(
            "/sales-orders",
            permission_route(post(sales_order::sales_order_create), &permissions::sales_order::CREATE),
        )
        .route(
            "/sales-orders/{id}",
            permission_route(get(sales_order::sales_order_detail), &permissions::sales_order::DETAIL),
        )
}

// 为父路由挂载已有销售单的编辑、提交、撤回、作废和交接入口。
fn lifecycle_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/working-copy",
            permission_route(
                put(sales_order::sales_order_save_working_copy),
                &permissions::sales_order::HANDOVER,
            ),
        )
        .route(
            "/sales-orders/{id}/submit",
            permission_route(post(sales_order::sales_order_submit), &permissions::sales_order::SUBMIT),
        )
        .route(
            "/sales-orders/{id}/cancel-approval",
            permission_route(
                post(sales_order::sales_order_cancel_approval),
                &permissions::sales_order::CANCEL_APPROVAL,
            ),
        )
        .route(
            "/sales-orders/{id}/void",
            permission_route(post(sales_order::sales_order_void), &permissions::sales_order::VOID),
        )
        .route(
            "/sales-orders/{id}/handover",
            permission_route(post(sales_order::sales_order_handover), &permissions::sales_order::HANDOVER),
        )
        .route(
            "/sales-orders/{id}/handover-candidates",
            permission_route(
                get(sales_order::sales_order_handover_candidates),
                &permissions::sales_order::LIST,
            ),
        )
}

// 为父路由挂载销售凭证下载和补签合同路由。
fn evidence_contract_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/contract-check",
            permission_route(
                get(sales_order::contract_check::sales_order_contract_check),
                &permissions::sales_order::HANDOVER,
            ),
        )
        .route(
            "/sales-orders/{id}/evidence-files/{asset_id}/download",
            permission_route(
                get(sales_order::evidence::sales_order_evidence_download),
                &permissions::sales_order::DETAIL,
            ),
        )
        .route(
            "/sales-orders/{id}/contract",
            permission_route(
                post(sales_order::sales_order_bind_contract),
                &permissions::sales_order::HANDOVER,
            ),
        )
}
