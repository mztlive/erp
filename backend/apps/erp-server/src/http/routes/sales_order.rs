//! 域 D13 `sales_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-orders`；每条路由
//! 统一走 JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::http::handler::fulfillment::address_arrangement;
use crate::http::handler::sales_order;
use crate::http::middleware::with_permission;
use crate::http::state::HttpState;
use crate::permissions;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/delivery-arrangements",
            with_permission(get(address_arrangement::sales_get), rbac, permissions::sales_order::DETAIL),
        )
        .route(
            "/sales-orders/{id}/delivery-arrangements",
            with_permission(
                put(address_arrangement::sales_save),
                rbac,
                permissions::sales_order::DELIVERY_ARRANGEMENT,
            ),
        )
        .merge(evidence_contract_routes(rbac))
        .merge(read_create_routes(rbac))
        .merge(lifecycle_routes(rbac))
}

/// 查询、方案预填与创建入口，各自保留资源动作权限。
fn read_create_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/timeline",
            with_permission(
                get(sales_order::timeline::sales_timeline),
                rbac,
                permissions::sales_order::TIMELINE_SALES_TIMELINE,
            ),
        )
        .route(
            "/sales-orders/selection-proposals/{id}/basis",
            with_permission(
                get(sales_order::selection::selection_basis),
                rbac,
                permissions::sales_order::SELECTION_SELECTION_BASIS,
            ),
        )
        .route(
            "/sales-orders",
            with_permission(get(sales_order::sales_order_list), rbac, permissions::sales_order::LIST),
        )
        .route(
            "/sales-orders",
            with_permission(post(sales_order::sales_order_create), rbac, permissions::sales_order::CREATE),
        )
        .route(
            "/sales-orders/{id}",
            with_permission(get(sales_order::sales_order_detail), rbac, permissions::sales_order::DETAIL),
        )
}

/// 已有销售单的编辑、提交、撤回、作废和交接入口。
fn lifecycle_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/working-copy",
            with_permission(
                put(sales_order::sales_order_save_working_copy),
                rbac,
                permissions::sales_order::SAVE_WORKING_COPY,
            ),
        )
        .route(
            "/sales-orders/{id}/submit",
            with_permission(post(sales_order::sales_order_submit), rbac, permissions::sales_order::SUBMIT),
        )
        .route(
            "/sales-orders/{id}/cancel-approval",
            with_permission(
                post(sales_order::sales_order_cancel_approval),
                rbac,
                permissions::sales_order::CANCEL_APPROVAL,
            ),
        )
        .route(
            "/sales-orders/{id}/void",
            with_permission(post(sales_order::sales_order_void), rbac, permissions::sales_order::VOID),
        )
        .route(
            "/sales-orders/{id}/handover",
            with_permission(
                post(sales_order::sales_order_handover),
                rbac,
                permissions::sales_order::HANDOVER,
            ),
        )
        .route(
            "/sales-orders/{id}/handover-candidates",
            with_permission(
                get(sales_order::sales_order_handover_candidates),
                rbac,
                permissions::sales_order::HANDOVER_CANDIDATES,
            ),
        )
}

/// 销售凭证下载和补签合同路由。
///
/// # 参数
/// * `rbac` - 当前共享权限服务
/// # 返回
/// 返回逐操作保留权限校验和上传限制的路由集合。
/// # 错误
/// 无；业务错误由各处理器返回。
fn evidence_contract_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/contract-check",
            with_permission(
                get(sales_order::contract_check::sales_order_contract_check),
                rbac,
                permissions::sales_order::CONTRACT_CHECK_SALES_ORDER_CONTRACT_CHECK,
            ),
        )
        .route(
            "/sales-orders/{id}/evidence-files/{asset_id}/download",
            with_permission(
                get(sales_order::evidence::sales_order_evidence_download),
                rbac,
                permissions::sales_order::EVIDENCE_SALES_ORDER_EVIDENCE_DOWNLOAD,
            ),
        )
        .route(
            "/sales-orders/{id}/contract",
            with_permission(
                post(sales_order::sales_order_bind_contract),
                rbac,
                permissions::sales_order::BIND_CONTRACT,
            ),
        )
}
