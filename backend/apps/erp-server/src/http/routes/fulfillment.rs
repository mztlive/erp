//! 域 D16 `fulfillment` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-receipts`、
//! `/admin/deliveries`、`/admin/electronic-deliveries`、`/admin/service-fulfillments`、
//! `/admin/customer-acceptances`；每条路由统一走 JWT + RBAC（`with_permission`），
//! 权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::AuthorizationService;

use crate::http::handler::fulfillment;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::http::upload;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
///
/// # 错误
/// 无；业务错误由各处理器返回。
pub fn routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .merge(address_arrangement_routes(rbac))
        .merge(purchase_receipt_routes(rbac))
        .merge(delivery_routes(rbac))
        .merge(electronic_routes(rbac))
        .merge(service_routes(rbac))
        .merge(acceptance_query_routes(rbac))
        .merge(acceptance_command_routes(rbac))
        .merge(acceptance_evidence_routes(rbac))
}

fn address_arrangement_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/deliveries/{id}/delivery-arrangements",
            with_permission(
                get(fulfillment::address_arrangement::delivery_get),
                rbac,
                permissions::fulfillment::DELIVERY_DELIVERY_UPDATE,
            ),
        )
        .route(
            "/deliveries/{id}/delivery-arrangements",
            with_permission(
                put(fulfillment::address_arrangement::delivery_save),
                rbac,
                permissions::fulfillment::DELIVERY_DELIVERY_UPDATE,
            ),
        )
}

fn purchase_receipt_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-receipts",
            with_permission(
                get(fulfillment::purchase_receipt::purchase_receipt_list),
                rbac,
                permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_LIST,
            ),
        )
        .route(
            "/purchase-receipts",
            with_permission(
                post(fulfillment::purchase_receipt::purchase_receipt_create),
                rbac,
                permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_CREATE,
            ),
        )
        .route(
            "/purchase-receipts/{id}",
            with_permission(
                get(fulfillment::purchase_receipt::purchase_receipt_detail),
                rbac,
                permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_DETAIL,
            ),
        )
        .route(
            "/purchase-receipts/{id}",
            with_permission(
                put(fulfillment::purchase_receipt::purchase_receipt_update),
                rbac,
                permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_UPDATE,
            ),
        )
        .route(
            "/purchase-receipts/{id}/post",
            with_permission(
                post(fulfillment::purchase_receipt::purchase_receipt_post),
                rbac,
                permissions::fulfillment::PURCHASE_RECEIPT_PURCHASE_RECEIPT_POST,
            ),
        )
}

fn delivery_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/deliveries",
            with_permission(
                get(fulfillment::delivery::delivery_list),
                rbac,
                permissions::fulfillment::DELIVERY_DELIVERY_LIST,
            ),
        )
        .route(
            "/deliveries",
            with_permission(
                post(fulfillment::delivery::delivery_create),
                rbac,
                permissions::fulfillment::DELIVERY_DELIVERY_CREATE,
            ),
        )
        .route(
            "/deliveries/{id}",
            with_permission(
                get(fulfillment::delivery::delivery_detail),
                rbac,
                permissions::fulfillment::DELIVERY_DELIVERY_DETAIL,
            ),
        )
        .route(
            "/deliveries/{id}",
            with_permission(
                put(fulfillment::delivery::delivery_update),
                rbac,
                permissions::fulfillment::DELIVERY_DELIVERY_UPDATE,
            ),
        )
        .route(
            "/deliveries/{id}/post",
            with_permission(
                post(fulfillment::delivery::delivery_post),
                rbac,
                permissions::fulfillment::DELIVERY_DELIVERY_POST,
            ),
        )
}

fn electronic_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/electronic-deliveries",
            with_permission(
                get(fulfillment::electronic_delivery::electronic_delivery_list),
                rbac,
                permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_LIST,
            ),
        )
        .route(
            "/electronic-deliveries",
            with_permission(
                post(fulfillment::electronic_delivery::electronic_delivery_create),
                rbac,
                permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CREATE,
            ),
        )
        .route(
            "/electronic-deliveries/{id}",
            with_permission(
                get(fulfillment::electronic_delivery::electronic_delivery_detail),
                rbac,
                permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_DETAIL,
            ),
        )
        .route(
            "/electronic-deliveries/{id}/confirm",
            with_permission(
                post(fulfillment::electronic_delivery::electronic_delivery_confirm),
                rbac,
                permissions::fulfillment::ELECTRONIC_DELIVERY_ELECTRONIC_DELIVERY_CONFIRM,
            ),
        )
}

fn service_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/service-fulfillments",
            with_permission(
                get(fulfillment::service_fulfillment::service_fulfillment_list),
                rbac,
                permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_LIST,
            ),
        )
        .route(
            "/service-fulfillments",
            with_permission(
                post(fulfillment::service_fulfillment::service_fulfillment_create),
                rbac,
                permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CREATE,
            ),
        )
        .route(
            "/service-fulfillments/{id}",
            with_permission(
                get(fulfillment::service_fulfillment::service_fulfillment_detail),
                rbac,
                permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_DETAIL,
            ),
        )
        .route(
            "/service-fulfillments/{id}/confirm",
            with_permission(
                upload::multipart_route(
                    post(fulfillment::service_fulfillment::service_fulfillment_confirm),
                    upload::MAX_MULTIPART_REQUEST_BYTES,
                ),
                rbac,
                permissions::fulfillment::SERVICE_FULFILLMENT_SERVICE_FULFILLMENT_CONFIRM,
            ),
        )
}

fn acceptance_query_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-acceptances",
            with_permission(
                get(fulfillment::customer_acceptance::customer_acceptance_list),
                rbac,
                permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_LIST,
            ),
        )
        .route(
            "/customer-acceptances",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_create),
                rbac,
                permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_CREATE,
            ),
        )
        .route(
            "/customer-acceptances/eligible",
            with_permission(
                get(fulfillment::customer_acceptance::customer_acceptance_eligible),
                rbac,
                permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_ELIGIBLE,
            ),
        )
}

fn acceptance_command_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-acceptances/commit",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_commit),
                rbac,
                permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_COMMIT,
            ),
        )
        .route(
            "/customer-acceptances/{id}",
            with_permission(
                get(fulfillment::customer_acceptance::customer_acceptance_detail),
                rbac,
                permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_DETAIL,
            ),
        )
        .route(
            "/customer-acceptances/{id}/post",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_post),
                rbac,
                permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_POST,
            ),
        )
        .route(
            "/customer-acceptances/{id}/reverse",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_reverse),
                rbac,
                permissions::fulfillment::CUSTOMER_ACCEPTANCE_CUSTOMER_ACCEPTANCE_REVERSE,
            ),
        )
}

/// 客户签收凭证读取路由。
///
/// # 参数
/// * `rbac` - 当前共享权限服务
/// # 返回
/// 返回逐操作保留权限校验和上传限制的路由集合。
/// # 错误
/// 无；业务错误由各处理器返回。
fn acceptance_evidence_routes(rbac: &AuthorizationService) -> Router<HttpState> {
    Router::new().route(
        "/customer-acceptances/{id}/evidence",
        with_permission(
            get(fulfillment::acceptance_evidence::customer_acceptance_evidence),
            rbac,
            permissions::fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
        ),
    )
}
