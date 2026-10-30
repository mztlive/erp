//! 预存款管理路由；逐动作JWT/RBAC与Process对象范围共同执行。
use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::supplier_deposit;
use crate::core::middleware::with_permission;
/// 装配已明确授权的财务账户与资金命令。
/// # 参数
/// * `rbac` - 当前应用共享权限服务。
/// # 返回
/// 返回逐动作权限保护的路由。
/// # 错误
/// 不执行I/O，不返回错误。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    reads(rbac)
        .merge(recharge(rbac))
        .merge(authorizations(rbac))
        .merge(execution(rbac))
        .merge(resolution(rbac))
}

fn resolution(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/reconciliations/{reconciliation}/resolve",
            with_permission(
                post(supplier_deposit::resolution::resolve),
                rbac,
                supplier_deposit::resolution::resolve_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/reservations/{reservation}/release",
            with_permission(
                post(supplier_deposit::resolution::release),
                rbac,
                supplier_deposit::resolution::release_permission_key(),
            ),
        )
}

fn reads(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts",
            with_permission(get(supplier_deposit::list), rbac, supplier_deposit::list_permission_key()),
        )
        .route(
            "/supplier-deposits/accounts",
            with_permission(post(supplier_deposit::open), rbac, supplier_deposit::open_permission_key()),
        )
        .route(
            "/supplier-deposits/accounts/{id}",
            with_permission(get(supplier_deposit::detail), rbac, supplier_deposit::detail_permission_key()),
        )
}

fn recharge(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests",
            with_permission(
                post(supplier_deposit::request_recharge),
                rbac,
                supplier_deposit::request_recharge_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests/{request}/approve",
            with_permission(
                post(supplier_deposit::approve_recharge),
                rbac,
                supplier_deposit::approve_recharge_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharge-requests/{request}/pay",
            with_permission(
                post(supplier_deposit::pay_recharge),
                rbac,
                supplier_deposit::pay_recharge_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/recharges",
            with_permission(
                post(supplier_deposit::receive_recharge),
                rbac,
                supplier_deposit::receive_recharge_permission_key(),
            ),
        )
}

fn authorizations(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/authorizations",
            with_permission(
                post(supplier_deposit::authorize),
                rbac,
                supplier_deposit::authorize_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/authorizations/{authorization}/revoke",
            with_permission(post(supplier_deposit::revoke), rbac, supplier_deposit::revoke_permission_key()),
        )
        .route(
            "/supplier-deposits/accounts/{id}/reconciliations",
            with_permission(
                post(supplier_deposit::reconcile),
                rbac,
                supplier_deposit::reconcile_permission_key(),
            ),
        )
}

fn execution(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/supplier-deposits/accounts/{id}/reservations",
            with_permission(
                post(supplier_deposit::reserve),
                rbac,
                supplier_deposit::reserve_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/debits",
            with_permission(
                post(supplier_deposit::record_debit),
                rbac,
                supplier_deposit::record_debit_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/usages/{usage}/settlements",
            with_permission(post(supplier_deposit::settle), rbac, supplier_deposit::settle_permission_key()),
        )
        .route(
            "/supplier-deposits/accounts/{id}/returns",
            with_permission(
                post(supplier_deposit::return_usage),
                rbac,
                supplier_deposit::return_usage_permission_key(),
            ),
        )
        .route(
            "/supplier-deposits/accounts/{id}/connections",
            with_permission(post(supplier_deposit::bind), rbac, supplier_deposit::bind_permission_key()),
        )
}
