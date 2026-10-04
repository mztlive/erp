//! 域 D16 `fulfillment` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-receipts`、
//! `/admin/deliveries`、`/admin/electronic-deliveries`、`/admin/service-fulfillments`、
//! `/admin/customer-acceptances`；每条路由统一走 JWT + RBAC（`with_permission`），
//! handler 标注 `#[permission_macros::permission]`。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::fulfillment;
use crate::core::middleware::with_permission;
use crate::core::upload;

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
                fulfillment::purchase_receipt::purchase_receipt_list_permission_key(),
            ),
        )
        .route(
            "/purchase-receipts",
            with_permission(
                post(fulfillment::purchase_receipt::purchase_receipt_create),
                rbac,
                fulfillment::purchase_receipt::purchase_receipt_create_permission_key(),
            ),
        )
        .route(
            "/purchase-receipts/{id}",
            with_permission(
                get(fulfillment::purchase_receipt::purchase_receipt_detail),
                rbac,
                fulfillment::purchase_receipt::purchase_receipt_detail_permission_key(),
            ),
        )
        .route(
            "/purchase-receipts/{id}",
            with_permission(
                put(fulfillment::purchase_receipt::purchase_receipt_update),
                rbac,
                fulfillment::purchase_receipt::purchase_receipt_update_permission_key(),
            ),
        )
        .route(
            "/purchase-receipts/{id}/post",
            with_permission(
                post(fulfillment::purchase_receipt::purchase_receipt_post),
                rbac,
                fulfillment::purchase_receipt::purchase_receipt_post_permission_key(),
            ),
        )
        .route(
            "/deliveries",
            with_permission(
                get(fulfillment::delivery::delivery_list),
                rbac,
                fulfillment::delivery::delivery_list_permission_key(),
            ),
        )
        .route(
            "/deliveries",
            with_permission(
                post(fulfillment::delivery::delivery_create),
                rbac,
                fulfillment::delivery::delivery_create_permission_key(),
            ),
        )
        .route(
            "/deliveries/{id}",
            with_permission(
                get(fulfillment::delivery::delivery_detail),
                rbac,
                fulfillment::delivery::delivery_detail_permission_key(),
            ),
        )
        .route(
            "/deliveries/{id}",
            with_permission(
                put(fulfillment::delivery::delivery_update),
                rbac,
                fulfillment::delivery::delivery_update_permission_key(),
            ),
        )
        .route(
            "/deliveries/{id}/post",
            with_permission(
                post(fulfillment::delivery::delivery_post),
                rbac,
                fulfillment::delivery::delivery_post_permission_key(),
            ),
        )
        .route(
            "/electronic-deliveries",
            with_permission(
                get(fulfillment::electronic_delivery::electronic_delivery_list),
                rbac,
                fulfillment::electronic_delivery::electronic_delivery_list_permission_key(),
            ),
        )
        .route(
            "/electronic-deliveries",
            with_permission(
                post(fulfillment::electronic_delivery::electronic_delivery_create),
                rbac,
                fulfillment::electronic_delivery::electronic_delivery_create_permission_key(),
            ),
        )
        .route(
            "/electronic-deliveries/{id}",
            with_permission(
                get(fulfillment::electronic_delivery::electronic_delivery_detail),
                rbac,
                fulfillment::electronic_delivery::electronic_delivery_detail_permission_key(),
            ),
        )
        .route(
            "/electronic-deliveries/{id}/confirm",
            with_permission(
                post(fulfillment::electronic_delivery::electronic_delivery_confirm),
                rbac,
                fulfillment::electronic_delivery::electronic_delivery_confirm_permission_key(),
            ),
        )
        .route(
            "/service-fulfillments",
            with_permission(
                get(fulfillment::service_fulfillment::service_fulfillment_list),
                rbac,
                fulfillment::service_fulfillment::service_fulfillment_list_permission_key(),
            ),
        )
        .route(
            "/service-fulfillments",
            with_permission(
                post(fulfillment::service_fulfillment::service_fulfillment_create),
                rbac,
                fulfillment::service_fulfillment::service_fulfillment_create_permission_key(),
            ),
        )
        .route(
            "/service-fulfillments/{id}",
            with_permission(
                get(fulfillment::service_fulfillment::service_fulfillment_detail),
                rbac,
                fulfillment::service_fulfillment::service_fulfillment_detail_permission_key(),
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
                fulfillment::service_fulfillment::service_fulfillment_confirm_permission_key(),
            ),
        )
        .route(
            "/customer-acceptances",
            with_permission(
                get(fulfillment::customer_acceptance::customer_acceptance_list),
                rbac,
                fulfillment::customer_acceptance::customer_acceptance_list_permission_key(),
            ),
        )
        .route(
            "/customer-acceptances",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_create),
                rbac,
                fulfillment::customer_acceptance::customer_acceptance_create_permission_key(),
            ),
        )
        .route(
            "/customer-acceptances/eligible",
            with_permission(
                get(fulfillment::customer_acceptance::customer_acceptance_eligible),
                rbac,
                fulfillment::customer_acceptance::customer_acceptance_eligible_permission_key(),
            ),
        )
        .route(
            "/customer-acceptances/commit",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_commit),
                rbac,
                fulfillment::customer_acceptance::customer_acceptance_commit_permission_key(),
            ),
        )
        .route(
            "/customer-acceptances/{id}",
            with_permission(
                get(fulfillment::customer_acceptance::customer_acceptance_detail),
                rbac,
                fulfillment::customer_acceptance::customer_acceptance_detail_permission_key(),
            ),
        )
        .route(
            "/customer-acceptances/{id}/post",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_post),
                rbac,
                fulfillment::customer_acceptance::customer_acceptance_post_permission_key(),
            ),
        )
        .route(
            "/customer-acceptances/{id}/reverse",
            with_permission(
                post(fulfillment::customer_acceptance::customer_acceptance_reverse),
                rbac,
                fulfillment::customer_acceptance::customer_acceptance_reverse_permission_key(),
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
            fulfillment::acceptance_evidence::customer_acceptance_evidence_permission_key(),
        ),
    )
}
