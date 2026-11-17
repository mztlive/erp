//! 域 D16 `fulfillment` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-receipts`、
//! `/admin/deliveries`、`/admin/electronic-deliveries`、`/admin/service-fulfillments`、
//! `/admin/customer-acceptances`；每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，
//! 权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::fulfillment;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;
use crate::http::upload;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回声明公共权限目录信息的路由集合。
///
/// # 错误
/// 无；业务错误由各处理器返回。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(address_arrangement_routes())
        .merge(purchase_receipt_routes())
        .merge(delivery_routes())
        .merge(electronic_routes())
        .merge(service_routes())
        .merge(acceptance_query_routes())
        .merge(acceptance_command_routes())
        .merge(acceptance_evidence_routes())
}

fn address_arrangement_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/deliveries/{id}/delivery-arrangements",
            permission_route(
                get(fulfillment::address_arrangement::delivery_get),
                &permissions::fulfillment::DELIVERY_DELIVERY_UPDATE,
            ),
        )
        .route(
            "/deliveries/{id}/delivery-arrangements",
            permission_route(
                put(fulfillment::address_arrangement::delivery_save),
                &permissions::fulfillment::DELIVERY_DELIVERY_UPDATE,
            ),
        )
}

fn purchase_receipt_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-receipts",
            permission_route(
                get(fulfillment::purchase_receipt::purchase_receipt_list),
                &permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST,
            ),
        )
        .route(
            "/purchase-receipts",
            permission_route(
                post(fulfillment::purchase_receipt::purchase_receipt_create),
                &permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE,
            ),
        )
        .route(
            "/purchase-receipts/{id}",
            permission_route(
                get(fulfillment::purchase_receipt::purchase_receipt_detail),
                &permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/purchase-receipts/{id}",
            permission_route(
                put(fulfillment::purchase_receipt::purchase_receipt_update),
                &permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE,
            ),
        )
        .route(
            "/purchase-receipts/{id}/post",
            permission_route(
                post(fulfillment::purchase_receipt::purchase_receipt_post),
                &permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST,
            ),
        )
}

fn delivery_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/deliveries",
            permission_route(
                get(fulfillment::delivery::delivery_list),
                &permissions::fulfillment::DELIVERY_DELIVERY_LIST,
            ),
        )
        .route(
            "/deliveries",
            permission_route(
                post(fulfillment::delivery::delivery_create),
                &permissions::fulfillment::DELIVERY_DELIVERY_CREATE,
            ),
        )
        .route(
            "/deliveries/{id}",
            permission_route(
                get(fulfillment::delivery::delivery_detail),
                &permissions::fulfillment::DELIVERY_DELIVERY_DETAIL,
            ),
        )
        .route(
            "/deliveries/{id}",
            permission_route(
                put(fulfillment::delivery::delivery_update),
                &permissions::fulfillment::DELIVERY_DELIVERY_UPDATE,
            ),
        )
        .route(
            "/deliveries/{id}/post",
            permission_route(
                post(fulfillment::delivery::delivery_post),
                &permissions::fulfillment::DELIVERY_DELIVERY_POST,
            ),
        )
}

fn electronic_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/electronic-deliveries",
            permission_route(
                get(fulfillment::electronic_delivery::electronic_delivery_list),
                &permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            ),
        )
        .route(
            "/electronic-deliveries",
            permission_route(
                post(fulfillment::electronic_delivery::electronic_delivery_create),
                &permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE,
            ),
        )
        .route(
            "/electronic-deliveries/{id}",
            permission_route(
                get(fulfillment::electronic_delivery::electronic_delivery_detail),
                &permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            ),
        )
        .route(
            "/electronic-deliveries/{id}/confirm",
            permission_route(
                post(fulfillment::electronic_delivery::electronic_delivery_confirm),
                &permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
            ),
        )
}

fn service_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/service-fulfillments",
            permission_route(
                get(fulfillment::service_fulfillment::service_fulfillment_list),
                &permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            ),
        )
        .route(
            "/service-fulfillments",
            permission_route(
                post(fulfillment::service_fulfillment::service_fulfillment_create),
                &permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE,
            ),
        )
        .route(
            "/service-fulfillments/{id}",
            permission_route(
                get(fulfillment::service_fulfillment::service_fulfillment_detail),
                &permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            ),
        )
        .route(
            "/service-fulfillments/{id}/confirm",
            permission_route(
                upload::multipart_route(
                    post(fulfillment::service_fulfillment::service_fulfillment_confirm),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                &permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
            ),
        )
}

fn acceptance_query_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-acceptances",
            permission_route(
                get(fulfillment::customer_acceptance::customer_acceptance_list),
                &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
            ),
        )
        .route(
            "/customer-acceptances",
            permission_route(
                post(fulfillment::customer_acceptance::customer_acceptance_create),
                &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
            ),
        )
        .route(
            "/customer-acceptances/eligible",
            permission_route(
                get(fulfillment::customer_acceptance::customer_acceptance_eligible),
                &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
            ),
        )
}

fn acceptance_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-acceptances/commit",
            permission_route(
                post(fulfillment::customer_acceptance::customer_acceptance_commit),
                &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST,
            ),
        )
        .route(
            "/customer-acceptances/{id}",
            permission_route(
                get(fulfillment::customer_acceptance::customer_acceptance_detail),
                &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL,
            ),
        )
        .route(
            "/customer-acceptances/{id}/post",
            permission_route(
                post(fulfillment::customer_acceptance::customer_acceptance_post),
                &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST,
            ),
        )
        .route(
            "/customer-acceptances/{id}/reverse",
            permission_route(
                post(fulfillment::customer_acceptance::customer_acceptance_reverse),
                &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE,
            ),
        )
}

/// 客户签收凭证读取路由。
///
/// # 参数
/// 无。
/// # 返回
/// 返回逐操作声明权限目录信息并保留上传限制的路由集合。
/// # 错误
/// 无；业务错误由各处理器返回。
fn acceptance_evidence_routes() -> Router<HttpState> {
    Router::new().route(
        "/customer-acceptances/{id}/evidence",
        permission_route(
            get(fulfillment::acceptance_evidence::customer_acceptance_evidence),
            &permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL,
        ),
    )
}
