//! 域 D15 `purchase_order` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/purchase-orders`、
//! `/admin/purchase-change-orders`；每条路由统一走
//! JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供，权限由显式目录统一定义。

use axum::Router;
use axum::routing::{get, post};
use erp_permissions as permissions;

use crate::http::handler::purchase_order;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 返回本域管理端路由集合。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回声明公共权限目录信息的路由集合。
///
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes() -> Router<HttpState> {
    purchase_document_routes()
        .merge(purchase_detail_routes())
        .merge(payment_arrangement_routes())
        .merge(purchase_workflow_routes())
        .merge(purchase_change_routes())
        .merge(purchase_change_draft_routes())
        .merge(purchase_sales_material_routes())
        .merge(purchase_correspondence_routes())
}

/// 采购关联销售材料下载始终使用采购对象详情资格及其准确销售版本。
fn purchase_sales_material_routes() -> Router<HttpState> {
    Router::new().route(
        "/purchase-orders/{id}/sales-materials/{asset_id}/download",
        permission_route(
            get(purchase_order::sales_material::purchase_sales_material_download),
            &permissions::purchase_order::DETAIL,
        ),
    )
}

/// 采购单列表、创建与详情入口。
fn purchase_document_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders",
            permission_route(get(purchase_order::purchase_order_list), &permissions::purchase_order::LIST),
        )
        .route(
            "/purchase-orders",
            permission_route(
                post(purchase_order::purchase_order_create),
                &permissions::purchase_order::CREATE,
            ),
        )
        .route(
            "/purchase-orders/from-sourcing",
            permission_route(
                post(purchase_order::purchase_order_create_from_sourcing),
                &permissions::purchase_order::CREATE,
            ),
        )
        .route(
            "/purchase-orders/{id}",
            permission_route(
                get(purchase_order::purchase_order_detail),
                &permissions::purchase_order::DETAIL,
            ),
        )
}

/// 采购单详情下的付款记录、交付安排范围和原单草稿保存入口。
fn purchase_detail_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/payments",
            permission_route(
                get(purchase_order::payments::purchase_payments),
                &permissions::purchase_order::DETAIL,
            ),
        )
        .route(
            "/purchase-orders/{id}/payments/{payment_id}/receipt",
            permission_route(
                get(purchase_order::payments::purchase_payment_receipt),
                &permissions::purchase_order::DETAIL,
            ),
        )
        .route(
            "/purchase-orders/{id}/delivery-options",
            permission_route(
                get(purchase_order::delivery_options::purchase_delivery_options),
                &permissions::purchase_order::SAVE_DRAFT,
            ),
        )
        .route(
            "/purchase-orders/{id}/draft",
            permission_route(
                post(purchase_order::purchase_order_save_draft),
                &permissions::purchase_order::SAVE_DRAFT,
            ),
        )
}

/// 采购单审批、作废、建单依据及发起变更入口。
fn purchase_workflow_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/void",
            permission_route(post(purchase_order::purchase_order_void), &permissions::purchase_order::VOID),
        )
        .route(
            "/purchase-orders/{id}/submit",
            permission_route(
                post(purchase_order::purchase_order_submit),
                &permissions::purchase_order::SUBMIT,
            ),
        )
        .route(
            "/purchase-orders/{id}/source-update/maintain",
            permission_route(
                post(purchase_order::purchase_order_maintain_source),
                &permissions::purchase_order::MAINTAIN_SOURCE,
            ),
        )
        .route(
            "/purchase-orders/{id}/cancel-approval",
            permission_route(
                post(purchase_order::purchase_order_cancel_approval),
                &permissions::purchase_order::CANCEL_APPROVAL,
            ),
        )
        .route(
            "/purchase-creation-bases",
            permission_route(
                get(purchase_order::purchase_creation_basis_list),
                &permissions::purchase_order::CREATE,
            ),
        )
        .route(
            "/purchase-orders/{id}/changes",
            permission_route(
                post(purchase_order::purchase_change_create),
                &permissions::purchase_order::PURCHASE_CHANGE_CREATE,
            ),
        )
}

/// 采购变更详情和既有审批命令入口。
fn purchase_change_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-change-orders",
            permission_route(
                get(purchase_order::purchase_change_list),
                &permissions::purchase_order::PURCHASE_CHANGE_LIST,
            ),
        )
        .route(
            "/purchase-change-orders/{id}",
            permission_route(
                get(purchase_order::purchase_change_detail),
                &permissions::purchase_order::PURCHASE_CHANGE_DETAIL,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/submit",
            permission_route(
                post(purchase_order::purchase_change_submit),
                &permissions::purchase_order::PURCHASE_CHANGE_SUBMIT,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/cancel-approval",
            permission_route(
                post(purchase_order::purchase_change_cancel_approval),
                &permissions::purchase_order::PURCHASE_CHANGE_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/effect",
            permission_route(
                post(purchase_order::purchase_change_effect),
                &permissions::purchase_order::PURCHASE_CHANGE_EFFECT,
            ),
        )
}

/// 采购改单草稿读取（复用提交权限）与作废入口。
fn purchase_change_draft_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-change-orders/{id}/draft",
            permission_route(
                post(purchase_order::change_draft::purchase_change_save_draft),
                &permissions::purchase_order::PURCHASE_CHANGE_SUBMIT,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/draft",
            permission_route(
                get(purchase_order::change_draft::purchase_change_draft),
                &permissions::purchase_order::PURCHASE_CHANGE_SUBMIT,
            ),
        )
        .route(
            "/purchase-change-orders/{id}/void",
            permission_route(
                post(purchase_order::change_void::purchase_change_void),
                &permissions::purchase_order::CHANGE_VOID_PURCHASE_CHANGE_VOID,
            ),
        )
}

fn purchase_correspondence_routes() -> Router<HttpState> {
    Router::new()
        .merge(expense_source_routes())
        .route(
            "/purchase-orders/{id}/emails",
            permission_route(
                get(purchase_order::correspondence::purchase_email_history),
                &permissions::purchase_order::DETAIL,
            ),
        )
        .route(
            "/purchase-orders/{id}/paper",
            permission_route(
                get(purchase_order::correspondence::purchase_paper),
                &permissions::purchase_order::DETAIL,
            ),
        )
        .route(
            "/sales-orders/{id}/expense-purchases",
            permission_route(
                post(purchase_order::expense::expense_purchase_create),
                &permissions::purchase_order::CREATE,
            ),
        )
        .route(
            "/purchase-orders/{id}/expenses/{expense_line_id}/occurrence",
            permission_route(
                post(purchase_order::expense::expense_occurrence_confirm),
                &permissions::purchase_order::EXPENSE_EXPENSE_OCCURRENCE_CONFIRM,
            ),
        )
        .route(
            "/purchase-orders/{id}/freight-waivers",
            permission_route(
                post(purchase_order::freight::freight_waiver_create),
                &permissions::purchase_order::SAVE_DRAFT,
            ),
        )
        .route(
            "/purchase-orders/{id}/email",
            permission_route(
                post(purchase_order::correspondence::send_purchase_email),
                &permissions::purchase_order::CORRESPONDENCE_SEND_PURCHASE_EMAIL,
            ),
        )
}

fn expense_source_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/expense-sources",
            permission_route(
                get(purchase_order::expense::expense_sources),
                &permissions::purchase_order::CREATE,
            ),
        )
        .route(
            "/purchase-orders/expense-sources/{id}",
            permission_route(
                get(purchase_order::expense::expense_source_detail),
                &permissions::purchase_order::CREATE,
            ),
        )
}

/// 采购付款安排查询与当前负责人跟进日期登记。
fn payment_arrangement_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/payment-arrangement",
            permission_route(
                get(purchase_order::payment_arrangement::payment_arrangement),
                &permissions::purchase_order::DETAIL,
            ),
        )
        .route(
            "/purchase-orders/{id}/payment-follow-up",
            permission_route(
                post(purchase_order::payment_arrangement::schedule_payment_follow_up),
                &permissions::purchase_payment_request::SUBMIT,
            ),
        )
}
