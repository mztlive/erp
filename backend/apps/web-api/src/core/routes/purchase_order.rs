//! 域 D15 `purchase_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-orders`、
//! `/admin/purchase-change-orders`；每条路由统一走
//! JWT + RBAC（`with_permission`），handler 标注 `#[permission_macros::permission]`。

use axum::Router;
use axum::routing::{get, post};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::purchase_order;
use crate::core::middleware::with_permission;

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
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
fn purchase_sales_material_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/purchase-orders/{id}/sales-materials/{asset_id}/download",
        with_permission(
            get(purchase_order::sales_material::purchase_sales_material_download),
            rbac,
            purchase_order::sales_material::purchase_sales_material_download_permission_key(),
        ),
    )
}

/// 采购单列表、创建与详情入口。
fn purchase_document_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-orders",
            with_permission(
                get(purchase_order::purchase_order_list),
                rbac,
                purchase_order::purchase_order_list_permission_key(),
            ),
        )
        .route(
            "/purchase-orders",
            with_permission(
                post(purchase_order::purchase_order_create),
                rbac,
                purchase_order::purchase_order_create_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/from-sourcing",
            with_permission(
                post(purchase_order::purchase_order_create_from_sourcing),
                rbac,
                purchase_order::purchase_order_create_from_sourcing_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}",
            with_permission(
                get(purchase_order::purchase_order_detail),
                rbac,
                purchase_order::purchase_order_detail_permission_key(),
            ),
        )
}

/// 采购单详情下的付款记录、交付安排范围和原单草稿保存入口。
fn purchase_detail_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/payments",
            with_permission(
                get(purchase_order::payments::purchase_payments),
                rbac,
                purchase_order::payments::purchase_payments_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/payments/{payment_id}/receipt",
            with_permission(
                get(purchase_order::payments::purchase_payment_receipt),
                rbac,
                purchase_order::payments::purchase_payment_receipt_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/delivery-options",
            with_permission(
                get(purchase_order::delivery_options::purchase_delivery_options),
                rbac,
                purchase_order::delivery_options::purchase_delivery_options_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/draft",
            with_permission(
                post(purchase_order::purchase_order_save_draft),
                rbac,
                purchase_order::purchase_order_save_draft_permission_key(),
            ),
        )
}

/// 采购单审批、作废、建单依据及发起变更入口。
fn purchase_workflow_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/void",
            with_permission(
                post(purchase_order::purchase_order_void),
                rbac,
                purchase_order::purchase_order_void_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/submit",
            with_permission(
                post(purchase_order::purchase_order_submit),
                rbac,
                purchase_order::purchase_order_submit_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/source-update/maintain",
            with_permission(
                post(purchase_order::purchase_order_maintain_source),
                rbac,
                purchase_order::purchase_order_maintain_source_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/cancel-approval",
            with_permission(
                post(purchase_order::purchase_order_cancel_approval),
                rbac,
                purchase_order::purchase_order_cancel_approval_permission_key(),
            ),
        )
        .route(
            "/purchase-creation-bases",
            with_permission(
                get(purchase_order::purchase_creation_basis_list),
                rbac,
                purchase_order::purchase_creation_basis_list_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/changes",
            with_permission(
                post(purchase_order::purchase_change_create),
                rbac,
                purchase_order::purchase_change_create_permission_key(),
            ),
        )
}

/// 采购变更详情和既有审批命令入口。
fn purchase_change_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-change-orders",
            with_permission(
                get(purchase_order::purchase_change_list),
                rbac,
                purchase_order::purchase_change_list_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}",
            with_permission(
                get(purchase_order::purchase_change_detail),
                rbac,
                purchase_order::purchase_change_detail_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}/submit",
            with_permission(
                post(purchase_order::purchase_change_submit),
                rbac,
                purchase_order::purchase_change_submit_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}/cancel-approval",
            with_permission(
                post(purchase_order::purchase_change_cancel_approval),
                rbac,
                purchase_order::purchase_change_cancel_approval_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}/effect",
            with_permission(
                post(purchase_order::purchase_change_effect),
                rbac,
                purchase_order::purchase_change_effect_permission_key(),
            ),
        )
}

/// 采购改单草稿读取（复用提交权限）与作废入口。
fn purchase_change_draft_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-change-orders/{id}/draft",
            with_permission(
                get(purchase_order::change_draft::purchase_change_draft),
                rbac,
                purchase_order::change_draft::purchase_change_draft_permission_key(),
            ),
        )
        .route(
            "/purchase-change-orders/{id}/void",
            with_permission(
                post(purchase_order::change_void::purchase_change_void),
                rbac,
                purchase_order::change_void::purchase_change_void_permission_key(),
            ),
        )
}

fn purchase_correspondence_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .merge(expense_source_routes(rbac))
        .route(
            "/purchase-orders/{id}/emails",
            with_permission(
                get(purchase_order::correspondence::purchase_email_history),
                rbac,
                purchase_order::correspondence::purchase_email_history_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/paper",
            with_permission(
                get(purchase_order::correspondence::purchase_paper),
                rbac,
                purchase_order::correspondence::purchase_paper_permission_key(),
            ),
        )
        .route(
            "/sales-orders/{id}/expense-purchases",
            with_permission(
                post(purchase_order::expense::expense_purchase_create),
                rbac,
                purchase_order::expense::expense_purchase_create_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/expenses/{expense_line_id}/occurrence",
            with_permission(
                post(purchase_order::expense::expense_occurrence_confirm),
                rbac,
                purchase_order::expense::expense_occurrence_confirm_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/freight-waivers",
            with_permission(
                post(purchase_order::freight::freight_waiver_create),
                rbac,
                purchase_order::freight::freight_waiver_create_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/email",
            with_permission(
                post(purchase_order::correspondence::send_purchase_email),
                rbac,
                purchase_order::correspondence::send_purchase_email_permission_key(),
            ),
        )
}

fn expense_source_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-orders/expense-sources",
            with_permission(
                get(purchase_order::expense::expense_sources),
                rbac,
                purchase_order::expense::expense_sources_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/expense-sources/{id}",
            with_permission(
                get(purchase_order::expense::expense_source_detail),
                rbac,
                purchase_order::expense::expense_source_detail_permission_key(),
            ),
        )
}

/// 采购付款安排查询与当前负责人跟进日期登记。
fn payment_arrangement_routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/payment-arrangement",
            with_permission(
                get(purchase_order::payment_arrangement::payment_arrangement),
                rbac,
                purchase_order::payment_arrangement::payment_arrangement_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/{id}/payment-follow-up",
            with_permission(
                post(purchase_order::payment_arrangement::schedule_payment_follow_up),
                rbac,
                purchase_order::payment_arrangement::schedule_payment_follow_up_permission_key(),
            ),
        )
}
