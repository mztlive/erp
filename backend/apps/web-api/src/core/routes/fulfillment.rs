//! 域 D16 `fulfillment` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-receipts`、
//! `/admin/deliveries`、`/admin/electronic-deliveries`、`/admin/service-fulfillments`、
//! `/admin/customer-acceptances`；每条路由统一走 JWT + RBAC（`with_permission`），
//! 权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::fulfillment;
use crate::core::middleware::with_permission;
use crate::core::upload;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .merge(acceptance_evidence_routes(rbac))
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
fn acceptance_evidence_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/customer-acceptances/{id}/evidence",
        with_permission(
            get(fulfillment::acceptance_evidence::customer_acceptance_evidence),
            rbac,
            permissions::fulfillment::ACCEPTANCE_EVIDENCE_CUSTOMER_ACCEPTANCE_EVIDENCE,
        ),
    )
}
