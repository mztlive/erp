//! 普通采购付款申请的管理端入口；旧应付直付不授予执行资格。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::purchase_payment_request;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
use crate::http::upload;

/// 装配申请、材料及付款路由。
/// 父路由执行 JWT 认证，业务用例校验相应操作权限。
pub fn routes() -> Router<HttpState> {
    query_routes().merge(candidate_routes()).merge(command_routes()).merge(material_routes())
}

fn query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests",
            permission_route(
                get(purchase_payment_request::list),
                &permissions::purchase_payment_request::LIST,
            ),
        )
        .route(
            "/purchase-payment-requests/candidates",
            permission_route(
                get(purchase_payment_request::candidates),
                &permissions::purchase_payment_request::CREATE,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}",
            permission_route(
                get(purchase_payment_request::detail),
                &permissions::purchase_payment_request::DETAIL,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/submissions/{version}",
            permission_route(
                get(purchase_payment_request::materials::submission),
                &permissions::purchase_payment_request::DETAIL,
            ),
        )
        .route(
            "/work-items/{id}/purchase-payment-request",
            permission_route(
                get(purchase_payment_request::materials::task_context),
                &permissions::work_item::DETAIL,
            ),
        )
}

fn candidate_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/candidates/page",
            permission_route(
                get(purchase_payment_request::candidate_list::page),
                &permissions::purchase_payment_request::CREATE,
            ),
        )
        .route(
            "/purchase-payment-requests/candidates/ids",
            permission_route(
                get(purchase_payment_request::candidate_list::ids),
                &permissions::purchase_payment_request::CREATE,
            ),
        )
}

fn command_routes() -> Router<HttpState> {
    draft_routes().merge(lifecycle_routes()).merge(payment_routes())
}

fn draft_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/batch",
            permission_route(
                post(purchase_payment_request::create_batch),
                &permissions::purchase_payment_request::CREATE,
            ),
        )
        .route(
            "/purchase-payment-requests",
            permission_route(
                post(purchase_payment_request::create),
                &permissions::purchase_payment_request::CREATE,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}",
            permission_route(
                put(purchase_payment_request::update),
                &permissions::purchase_payment_request::UPDATE,
            ),
        )
}

fn lifecycle_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/{id}/submit",
            permission_route(
                post(purchase_payment_request::submit),
                &permissions::purchase_payment_request::SUBMIT,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/withdraw",
            permission_route(
                post(purchase_payment_request::withdraw),
                &permissions::purchase_payment_request::WITHDRAW,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/close",
            permission_route(
                post(purchase_payment_request::close),
                &permissions::purchase_payment_request::CLOSE,
            ),
        )
}

fn payment_routes() -> Router<HttpState> {
    Router::new()
        .merge(deposit_routes())
        .route(
            "/purchase-payment-requests/{id}/supplier-credits",
            permission_route(
                get(purchase_payment_request::credit::list),
                &permissions::purchase_payment_request::CREDIT_LIST,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/supplier-credits/use",
            permission_route(
                post(purchase_payment_request::credit::apply),
                &permissions::purchase_payment_request::CREDIT_LIST,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/reveal-account",
            permission_route(
                post(purchase_payment_request::payments::reveal_account),
                &permissions::payable::PAYMENT_RECIPIENT_REVEAL,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/payments/commit",
            permission_route(
                upload::multipart_route(
                    post(purchase_payment_request::payments::pay),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::purchase_payment_request::CREDIT_LIST,
            ),
        )
}

fn deposit_routes() -> Router<HttpState> {
    Router::new().route(
        "/purchase-payment-requests/{id}/deposit-executions",
        permission_route(
            post(purchase_payment_request::deposit::execute),
            &permissions::purchase_payment_request::CREDIT_LIST,
        ),
    )
}

fn material_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-payment-requests/{id}/materials/{file_asset_id}/download",
            permission_route(
                get(purchase_payment_request::materials::material_download),
                &permissions::purchase_payment_request::DETAIL,
            ),
        )
        .route(
            "/purchase-payment-requests/{id}/materials/{file_asset_id}/preview",
            permission_route(
                get(purchase_payment_request::materials::material_preview),
                &permissions::purchase_payment_request::DETAIL,
            ),
        )
}
