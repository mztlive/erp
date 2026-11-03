//! 域 D15 `purchase_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-orders`、
//! `/admin/purchase-change-orders`；每条路由统一走
//! JWT + RBAC（`with_permission`），权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::http::handler::purchase_order;
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
    purchase_document_routes(rbac)
        .merge(purchase_detail_routes(rbac))
        .merge(payment_arrangement_routes(rbac))
        .merge(purchase_workflow_routes(rbac))
        .merge(purchase_change_routes(rbac))
        .merge(purchase_change_draft_routes(rbac))
        .merge(purchase_sales_material_routes(rbac))
        .merge(purchase_correspondence_routes(rbac))
}

/// 采购关联销售材料下载始终使用采购对象详情资格及其准确销售版本。
fn purchase_sales_material_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new().route(
        "/purchase-orders/{id}/sales-materials/{asset_id}/download",
        with_permission(
            get(purchase_order::sales_material::purchase_sales_material_download),
            rbac,
            permissions::purchase_order::SALES_MATERIAL_PURCHASE_SALES_MATERIAL_DOWNLOAD,
        ),
    )
}

/// 采购单列表、创建与详情入口。
fn purchase_document_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders",
            with_permission(
                get(purchase_order::purchase_order_list),
                rbac,
                permissions::purchase_order::LIST,
            ),
        )
        .route(
            "/purchase-orders",
            with_permission(
                post(purchase_order::purchase_order_create),
                rbac,
                permissions::purchase_order::CREATE,
            ),
        )
        .route(
            "/purchase-orders/from-sourcing",
            with_permission(
                post(purchase_order::purchase_order_create_from_sourcing),
                rbac,
                permissions::purchase_order::CREATE_FROM_SOURCING,
            ),
        )
        .route(
            "/purchase-orders/{id}",
            with_permission(
                get(purchase_order::purchase_order_detail),
                rbac,
                permissions::purchase_order::DETAIL,
            ),
        )
}

/// 采购单详情下的付款记录、交付安排范围和原单草稿保存入口。
fn purchase_detail_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/payments",
            with_permission(
                get(purchase_order::payments::purchase_payments),
                rbac,
                permissions::purchase_order::PAYMENTS_PURCHASE_PAYMENTS,
            ),
        )
        .route(
            "/purchase-orders/{id}/payments/{payment_id}/receipt",
            with_permission(
                get(purchase_order::payments::purchase_payment_receipt),
                rbac,
                permissions::purchase_order::PAYMENTS_PURCHASE_PAYMENT_RECEIPT,
            ),
        )
        .route(
            "/purchase-orders/{id}/delivery-options",
            with_permission(
                get(purchase_order::delivery_options::purchase_delivery_options),
                rbac,
                permissions::purchase_order::DELIVERY_OPTIONS_PURCHASE_DELIVERY_OPTIONS,
            ),
        )
        .route(
            "/purchase-orders/{id}/draft",
            with_permission(
                post(purchase_order::purchase_order_save_draft),
                rbac,
                permissions::purchase_order::SAVE_DRAFT,
            ),
        )
}

/// 采购单审批、作废、建单依据及发起变更入口。
fn purchase_workflow_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/void",
            with_permission(
                post(purchase_order::purchase_order_void),
                rbac,
                permissions::purchase_order::VOID,
            ),
        )
        .route(
            "/purchase-orders/{id}/submit",
            with_permission(
                post(purchase_order::purchase_order_submit),
                rbac,
                permissions::purchase_order::SUBMIT,
            ),
        )
        .route(
            "/purchase-orders/{id}/source-update/maintain",
            with_permission(
                post(purchase_order::purchase_order_maintain_source),
                rbac,
                permissions::purchase_order::MAINTAIN_SOURCE,
            ),
        )
        .route(
            "/purchase-orders/{id}/cancel-approval",
            with_permission(
                post(purchase_order::purchase_order_cancel_approval),
                rbac,
                permissions::purchase_order::CANCEL_APPROVAL,
            ),
        )
        .route(
            "/purchase-creation-bases",
            with_permission(
                get(purchase_order::purchase_creation_basis_list),
                rbac,
                permissions::purchase_order::PURCHASE_CREATION_BASIS_LIST,
            ),
        )
        .route(
            "/purchase-orders/{id}/changes",
            with_permission(
                post(purchase_order::purchase_change_create),
                rbac,
                permissions::purchase_order::PURCHASE_CHANGE_CREATE,
            ),
        )
}

/// 采购变更详情和既有审批命令入口。
fn purchase_change_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-change-orders",
            with_permission(
                get(purchase_order::purchase_change_list),
                rbac,
                permissions::purchase_order::PURCHASE_CHANGE_LIST,
            ),
        )
        .route(
            "/purchase-change-orders/{id}",
            with_permission(
                get(purchase_order::purchase_change_detail),
                rbac,
                permissions::purchase_order::PURCHASE_CHANGE_DETAIL,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/submit",
            with_permission(
                post(purchase_order::purchase_change_submit),
                rbac,
                permissions::purchase_order::PURCHASE_CHANGE_SUBMIT,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/cancel-approval",
            with_permission(
                post(purchase_order::purchase_change_cancel_approval),
                rbac,
                permissions::purchase_order::PURCHASE_CHANGE_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/effect",
            with_permission(
                post(purchase_order::purchase_change_effect),
                rbac,
                permissions::purchase_order::PURCHASE_CHANGE_EFFECT,
            ),
        )
}

/// 采购改单草稿读取（复用提交权限）与作废入口。
fn purchase_change_draft_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-change-orders/{id}/draft",
            with_permission(
                post(purchase_order::change_draft::purchase_change_save_draft),
                rbac,
                permissions::purchase_order::CHANGE_DRAFT_SAVE,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/draft",
            with_permission(
                get(purchase_order::change_draft::purchase_change_draft),
                rbac,
                permissions::purchase_order::CHANGE_DRAFT_PURCHASE_CHANGE_DRAFT,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/void",
            with_permission(
                post(purchase_order::change_void::purchase_change_void),
                rbac,
                permissions::purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID,
            ),
        )
}

fn purchase_correspondence_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .merge(expense_source_routes(rbac))
        .route(
            "/purchase-orders/{id}/emails",
            with_permission(
                get(purchase_order::correspondence::purchase_email_history),
                rbac,
                permissions::purchase_order::CORRESPONDENCE_PURCHASE_EMAIL_HISTORY,
            ),
        )
        .route(
            "/purchase-orders/{id}/paper",
            with_permission(
                get(purchase_order::correspondence::purchase_paper),
                rbac,
                permissions::purchase_order::CORRESPONDENCE_PURCHASE_PAPER,
            ),
        )
        .route(
            "/sales-orders/{id}/expense-purchases",
            with_permission(
                post(purchase_order::expense::expense_purchase_create),
                rbac,
                permissions::purchase_order::EXPENSE_EXPENSE_PURCHASE_CREATE,
            ),
        )
        .route(
            "/purchase-orders/{id}/expenses/{expense_line_id}/occurrence",
            with_permission(
                post(purchase_order::expense::expense_occurrence_confirm),
                rbac,
                permissions::purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM,
            ),
        )
        .route(
            "/purchase-orders/{id}/freight-waivers",
            with_permission(
                post(purchase_order::freight::freight_waiver_create),
                rbac,
                permissions::purchase_order::FREIGHT_FREIGHT_WAIVER_CREATE,
            ),
        )
        .route(
            "/purchase-orders/{id}/email",
            with_permission(
                post(purchase_order::correspondence::send_purchase_email),
                rbac,
                permissions::purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL,
            ),
        )
}

fn expense_source_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/expense-sources",
            with_permission(
                get(purchase_order::expense::expense_sources),
                rbac,
                permissions::purchase_order::EXPENSE_EXPENSE_SOURCES,
            ),
        )
        .route(
            "/purchase-orders/expense-sources/{id}",
            with_permission(
                get(purchase_order::expense::expense_source_detail),
                rbac,
                permissions::purchase_order::EXPENSE_EXPENSE_SOURCE_DETAIL,
            ),
        )
}

/// 采购付款安排查询与当前负责人跟进日期登记。
fn payment_arrangement_routes(rbac: &SharedRbacService) -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/payment-arrangement",
            with_permission(
                get(purchase_order::payment_arrangement::payment_arrangement),
                rbac,
                permissions::purchase_order::PAYMENT_ARRANGEMENT_PAYMENT_ARRANGEMENT,
            ),
        )
        .route(
            "/purchase-orders/{id}/payment-follow-up",
            with_permission(
                post(purchase_order::payment_arrangement::schedule_payment_follow_up),
                rbac,
                permissions::purchase_order::PAYMENT_ARRANGEMENT_SCHEDULE_PAYMENT_FOLLOW_UP,
            ),
        )
}
