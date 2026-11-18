//! 预存款管理路由；JWT 认证及 Process 统一授权共同执行。
use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::supplier_deposit;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
/// 装配已明确授权的财务账户与资金命令。
/// 按操作声明公共权限目录信息。
pub fn routes() -> Router<HttpState> {
    reads().merge(recharge()).merge(authorizations()).merge(execution()).merge(resolution())
}

fn resolution() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/reconciliations/{reconciliation}/resolve",
            permission_route(
                post(supplier_deposit::resolution::resolve),
                &permissions::supplier_deposit::RESOLUTION_RESOLVE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/reservations/{reservation}/release",
            permission_route(
                post(supplier_deposit::resolution::release),
                &permissions::supplier_deposit::RESOLUTION_RELEASE,
            ),
        )
}

fn reads() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts",
            permission_route(get(supplier_deposit::list), &permissions::supplier_deposit::LIST),
        )
        .route(
            "/supplier-deposits/accounts",
            permission_route(post(supplier_deposit::open), &permissions::supplier_deposit::OPEN),
        )
        .route(
            "/supplier-deposits/accounts/{id}",
            permission_route(get(supplier_deposit::detail), &permissions::supplier_deposit::DETAIL),
        )
}

fn recharge() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests",
            permission_route(
                post(supplier_deposit::request_recharge),
                &permissions::supplier_deposit::REQUEST_RECHARGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests/{request}/approve",
            permission_route(
                post(supplier_deposit::approve_recharge),
                &permissions::supplier_deposit::APPROVE_RECHARGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests/{request}/pay",
            permission_route(
                post(supplier_deposit::pay_recharge),
                &permissions::supplier_deposit::PAY_RECHARGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharges",
            permission_route(
                post(supplier_deposit::receive_recharge),
                &permissions::supplier_deposit::RECEIVE_RECHARGE,
            ),
        )
}

fn authorizations() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/authorizations",
            permission_route(post(supplier_deposit::authorize), &permissions::supplier_deposit::AUTHORIZE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/authorizations/{authorization}/revoke",
            permission_route(post(supplier_deposit::revoke), &permissions::supplier_deposit::REVOKE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/reconciliations",
            permission_route(post(supplier_deposit::reconcile), &permissions::supplier_deposit::RECONCILE),
        )
}

fn execution() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/reservations",
            permission_route(post(supplier_deposit::reserve), &permissions::supplier_deposit::RESERVE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/debits",
            permission_route(post(supplier_deposit::record_debit), &permissions::supplier_deposit::SETTLE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/usages/{usage}/settlements",
            permission_route(post(supplier_deposit::settle), &permissions::supplier_deposit::SETTLE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/returns",
            permission_route(
                post(supplier_deposit::return_usage),
                &permissions::supplier_deposit::RETURN_USAGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/connections",
            permission_route(post(supplier_deposit::bind), &permissions::supplier_deposit::BIND),
        )
}
