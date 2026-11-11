//! 普通采购付款申请的管理端入口；旧应付直付不授予执行资格。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::purchase_payment_request;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::http::upload;
use crate::permissions;

/// 装配申请、材料及付款路由。
/// # 参数
/// * `rbac` - 管理端共享权限服务。
/// # 返回
/// 经 JWT 与逐动作权限保护的路由集合。
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    query_routes(rbac).merge(candidate_routes(rbac)).merge(command_routes(rbac)).merge(material_routes(rbac))
}

fn query_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests",
            with_permission(
                get(purchase_payment_request::list),
                rbac,
                permissions::purchase_payment_request::LIST,
            ),
        )
        .route(
            "/purchase-payment-requests/candidates",
            with_permission(
                get(purchase_payment_request::candidates),
                rbac,
                permissions::purchase_payment_request::CANDIDATES,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}",
            with_permission(
                get(purchase_payment_request::detail),
                rbac,
                permissions::purchase_payment_request::DETAIL,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/submissions/{version}",
            with_permission(
                get(purchase_payment_request::materials::submission),
                rbac,
                permissions::purchase_payment_request::MATERIALS_SUBMISSION,
            ),
        )
        .route(
            "/work-items/{id}/purchase-payment-request",
            with_permission(
                get(purchase_payment_request::materials::task_context),
                rbac,
                permissions::purchase_payment_request::MATERIALS_TASK_CONTEXT,
            ),
        )
}

fn candidate_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/candidates/page",
            with_permission(
                get(purchase_payment_request::candidate_list::page),
                rbac,
                permissions::purchase_payment_request::CANDIDATE_LIST_PAGE,
            ),
        )
        .route(
            "/purchase-payment-requests/candidates/ids",
            with_permission(
                get(purchase_payment_request::candidate_list::ids),
                rbac,
                permissions::purchase_payment_request::CANDIDATE_LIST_IDS,
            ),
        )
}

fn command_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    draft_routes(rbac).merge(lifecycle_routes(rbac)).merge(payment_routes(rbac))
}

fn draft_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/batch",
            with_permission(
                post(purchase_payment_request::create_batch),
                rbac,
                permissions::purchase_payment_request::CREATE_BATCH,
            ),
        )
        .route(
            "/purchase-payment-requests",
            with_permission(
                post(purchase_payment_request::create),
                rbac,
                permissions::purchase_payment_request::CREATE,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}",
            with_permission(
                put(purchase_payment_request::update),
                rbac,
                permissions::purchase_payment_request::UPDATE,
            ),
        )
}

fn lifecycle_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/{id}/submit",
            with_permission(
                post(purchase_payment_request::submit),
                rbac,
                permissions::purchase_payment_request::SUBMIT,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/withdraw",
            with_permission(
                post(purchase_payment_request::withdraw),
                rbac,
                permissions::purchase_payment_request::WITHDRAW,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/close",
            with_permission(
                post(purchase_payment_request::close),
                rbac,
                permissions::purchase_payment_request::CLOSE,
            ),
        )
}

fn payment_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .merge(deposit_routes(rbac))
        .route(
            "/purchase-payment-requests/{id}/supplier-credits",
            with_permission(
                get(purchase_payment_request::credit::list),
                rbac,
                permissions::purchase_payment_request::CREDIT_LIST,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/supplier-credits/use",
            with_permission(
                post(purchase_payment_request::credit::apply),
                rbac,
                permissions::purchase_payment_request::CREDIT_APPLY,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/reveal-account",
            with_permission(
                post(purchase_payment_request::payments::reveal_account),
                rbac,
                permissions::purchase_payment_request::PAYMENTS_REVEAL_ACCOUNT,
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
                permissions::purchase_payment_request::PAYMENTS_PAY,
            ),
        )
}

fn deposit_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new().route(
        "/purchase-payment-requests/{id}/deposit-executions",
        with_permission(
            post(purchase_payment_request::deposit::execute),
            rbac,
            permissions::purchase_payment_request::DEPOSIT_EXECUTE,
        ),
    )
}

fn material_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/{id}/materials/{file_asset_id}/download",
            with_permission(
                get(purchase_payment_request::materials::material_download),
                rbac,
                permissions::purchase_payment_request::MATERIALS_MATERIAL_DOWNLOAD,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/materials/{file_asset_id}/preview",
            with_permission(
                get(purchase_payment_request::materials::material_preview),
                rbac,
                permissions::purchase_payment_request::MATERIALS_MATERIAL_PREVIEW,
            ),
        )
}
