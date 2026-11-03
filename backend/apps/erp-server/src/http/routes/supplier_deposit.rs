//! 预存款管理路由；逐动作JWT/RBAC与Process对象范围共同执行。
use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::http::handler::supplier_deposit;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;
/// 装配已明确授权的财务账户与资金命令。
/// # 参数
/// * `rbac` - 当前应用共享权限服务。
/// # 返回
/// 返回逐动作权限保护的路由。
/// # 错误
/// 不执行I/O，不返回错误。
pub fn routes(rbac: &SharedRbacService) -> Router<HttpState> {
    reads(rbac)
        .merge(recharge(rbac))
        .merge(authorizations(rbac))
        .merge(execution(rbac))
        .merge(resolution(rbac))
}

fn resolution(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/reconciliations/{reconciliation}/resolve",
            with_permission(
                post(supplier_deposit::resolution::resolve),
                rbac,
                permissions::supplier_deposit::RESOLUTION_RESOLVE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/reservations/{reservation}/release",
            with_permission(
                post(supplier_deposit::resolution::release),
                rbac,
                permissions::supplier_deposit::RESOLUTION_RELEASE,
            ),
        )
}

fn reads(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts",
            with_permission(get(supplier_deposit::list), rbac, permissions::supplier_deposit::LIST),
        )
        .route(
            "/supplier-deposits/accounts",
            with_permission(post(supplier_deposit::open), rbac, permissions::supplier_deposit::OPEN),
        )
        .route(
            "/supplier-deposits/accounts/{id}",
            with_permission(get(supplier_deposit::detail), rbac, permissions::supplier_deposit::DETAIL),
        )
}

fn recharge(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests",
            with_permission(
                post(supplier_deposit::request_recharge),
                rbac,
                permissions::supplier_deposit::REQUEST_RECHARGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests/{request}/approve",
            with_permission(
                post(supplier_deposit::approve_recharge),
                rbac,
                permissions::supplier_deposit::APPROVE_RECHARGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests/{request}/pay",
            with_permission(
                post(supplier_deposit::pay_recharge),
                rbac,
                permissions::supplier_deposit::PAY_RECHARGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharges",
            with_permission(
                post(supplier_deposit::receive_recharge),
                rbac,
                permissions::supplier_deposit::RECEIVE_RECHARGE,
            ),
        )
}

fn authorizations(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/authorizations",
            with_permission(
                post(supplier_deposit::authorize),
                rbac,
                permissions::supplier_deposit::AUTHORIZE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/authorizations/{authorization}/revoke",
            with_permission(post(supplier_deposit::revoke), rbac, permissions::supplier_deposit::REVOKE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/reconciliations",
            with_permission(
                post(supplier_deposit::reconcile),
                rbac,
                permissions::supplier_deposit::RECONCILE,
            ),
        )
}

fn execution(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/reservations",
            with_permission(post(supplier_deposit::reserve), rbac, permissions::supplier_deposit::RESERVE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/debits",
            with_permission(
                post(supplier_deposit::record_debit),
                rbac,
                permissions::supplier_deposit::RECORD_DEBIT,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/usages/{usage}/settlements",
            with_permission(post(supplier_deposit::settle), rbac, permissions::supplier_deposit::SETTLE),
        )
        .route(
            "/supplier-deposits/accounts/{id}/returns",
            with_permission(
                post(supplier_deposit::return_usage),
                rbac,
                permissions::supplier_deposit::RETURN_USAGE,
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/connections",
            with_permission(post(supplier_deposit::bind), rbac, permissions::supplier_deposit::BIND),
        )
}
