//! 普通采购付款申请的管理端入口；旧应付直付不授予执行资格。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::purchase_payment_request;
use crate::core::middleware::with_permission;
use crate::core::upload;

/// 装配申请、材料及付款路由。
/// # 参数
/// * `rbac` - 管理端共享权限服务。
/// # 返回
/// 经 JWT 与逐动作权限保护的路由集合。
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    query_routes(rbac).merge(candidate_routes(rbac)).merge(command_routes(rbac)).merge(material_routes(rbac))
}

fn query_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-payment-requests",
            with_permission(
                get(purchase_payment_request::list),
                rbac,
                purchase_payment_request::list_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/candidates",
            with_permission(
                get(purchase_payment_request::candidates),
                rbac,
                purchase_payment_request::candidates_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}",
            with_permission(
                get(purchase_payment_request::detail),
                rbac,
                purchase_payment_request::detail_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/submissions/{version}",
            with_permission(
                get(purchase_payment_request::materials::submission),
                rbac,
                purchase_payment_request::materials::submission_permission_key(),
            ),
        )
        .route(
            "/work-items/{id}/purchase-payment-request",
            with_permission(
                get(purchase_payment_request::materials::task_context),
                rbac,
                purchase_payment_request::materials::task_context_permission_key(),
            ),
        )
}

fn candidate_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-payment-requests/candidates/page",
            with_permission(
                get(purchase_payment_request::candidate_list::page),
                rbac,
                purchase_payment_request::candidate_list::page_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/candidates/ids",
            with_permission(
                get(purchase_payment_request::candidate_list::ids),
                rbac,
                purchase_payment_request::candidate_list::ids_permission_key(),
            ),
        )
}

fn command_routes(rbac: &SharedRbacService) -> Router<AppState> {
    draft_routes(rbac).merge(lifecycle_routes(rbac)).merge(payment_routes(rbac))
}

fn draft_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-payment-requests/batch",
            with_permission(
                post(purchase_payment_request::create_batch),
                rbac,
                purchase_payment_request::create_batch_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests",
            with_permission(
                post(purchase_payment_request::create),
                rbac,
                purchase_payment_request::create_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}",
            with_permission(
                put(purchase_payment_request::update),
                rbac,
                purchase_payment_request::update_permission_key(),
            ),
        )
}

fn lifecycle_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-payment-requests/{id}/submit",
            with_permission(
                post(purchase_payment_request::submit),
                rbac,
                purchase_payment_request::submit_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/withdraw",
            with_permission(
                post(purchase_payment_request::withdraw),
                rbac,
                purchase_payment_request::withdraw_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/close",
            with_permission(
                post(purchase_payment_request::close),
                rbac,
                purchase_payment_request::close_permission_key(),
            ),
        )
}

fn payment_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-payment-requests/{id}/supplier-credits",
            with_permission(
                get(purchase_payment_request::credit::list),
                rbac,
                purchase_payment_request::credit::list_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/supplier-credits/use",
            with_permission(
                post(purchase_payment_request::credit::apply),
                rbac,
                purchase_payment_request::credit::apply_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/reveal-account",
            with_permission(
                post(purchase_payment_request::payments::reveal_account),
                rbac,
                purchase_payment_request::payments::reveal_account_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/payments/commit",
            with_permission(
                upload::multipart_route(
                    post(purchase_payment_request::payments::pay),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                purchase_payment_request::payments::pay_permission_key(),
            ),
        )
}

fn material_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-payment-requests/{id}/materials/{file_asset_id}/download",
            with_permission(
                get(purchase_payment_request::materials::material_download),
                rbac,
                purchase_payment_request::materials::material_download_permission_key(),
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/materials/{file_asset_id}/preview",
            with_permission(
                get(purchase_payment_request::materials::material_preview),
                rbac,
                purchase_payment_request::materials::material_preview_permission_key(),
            ),
        )
}
