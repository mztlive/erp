//! 销售和采购询价分别登记路由及权限，挂载于已认证的 /admin。
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::sales_inquiry;
use crate::core::middleware::with_permission;

/// 装配询价路由。
/// # 参数
/// `rbac` 为当前权限服务。
/// # 返回
/// 返回销售与采购独立入口。
/// # 错误
/// 不执行 I/O，不返回错误。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .merge(sales_reads(rbac))
        .merge(sales_drafts(rbac))
        .merge(sales_feedback(rbac))
        .merge(sales_imports(rbac))
        .merge(sales_conversion(rbac))
        .merge(sales_recovery(rbac))
        .merge(procurement_reads(rbac))
        .merge(procurement_work(rbac))
        .merge(procurement_feedback(rbac))
        .merge(procurement_quotes(rbac))
        .merge(procurement_fees(rbac))
        .merge(procurement_fee_attachments(rbac))
}

fn procurement_fee_attachments(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new().route(
        "/procurement-inquiries/{id}/fee-bases/{basis_id}/attachments/{asset_id}",
        with_permission(
            get(sales_inquiry::internal::fee_basis_attachment),
            rbac,
            sales_inquiry::internal::fee_basis_attachment_permission_key(),
        ),
    )
}

fn sales_reads(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-inquiries",
            with_permission(
                get(sales_inquiry::query::sales_list),
                rbac,
                sales_inquiry::query::sales_list_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}",
            with_permission(
                get(sales_inquiry::query::sales_detail),
                rbac,
                sales_inquiry::query::sales_detail_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/template-products",
            with_permission(
                post(sales_inquiry::query::sales_template_products),
                rbac,
                sales_inquiry::query::sales_template_products_permission_key(),
            ),
        )
}

fn sales_drafts(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-inquiries",
            with_permission(
                post(sales_inquiry::commands::sales_create),
                rbac,
                sales_inquiry::commands::sales_create_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/draft",
            with_permission(
                put(sales_inquiry::commands::sales_save),
                rbac,
                sales_inquiry::commands::sales_save_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/submit",
            with_permission(
                post(sales_inquiry::commands::sales_submit),
                rbac,
                sales_inquiry::commands::sales_submit_permission_key(),
            ),
        )
}

fn sales_feedback(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-inquiries/{id}/accept",
            with_permission(
                post(sales_inquiry::commands::sales_accept),
                rbac,
                sales_inquiry::commands::sales_accept_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/reinquire",
            with_permission(
                post(sales_inquiry::commands::sales_reinquire),
                rbac,
                sales_inquiry::commands::sales_reinquire_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/close",
            with_permission(
                post(sales_inquiry::commands::sales_close),
                rbac,
                sales_inquiry::commands::sales_close_permission_key(),
            ),
        )
}

fn sales_imports(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-inquiries/imports/preview",
            with_permission(
                post(sales_inquiry::import_creation::creation_preview),
                rbac,
                sales_inquiry::import_creation::creation_preview_permission_key(),
            )
            .layer(DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024)),
        )
        .route(
            "/sales-inquiries/imports/create",
            with_permission(
                post(sales_inquiry::import_creation::creation_confirm),
                rbac,
                sales_inquiry::import_creation::creation_confirm_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/imports/preview",
            with_permission(
                post(sales_inquiry::import::sales_import_preview),
                rbac,
                sales_inquiry::import::sales_import_preview_permission_key(),
            )
            .layer(DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024)),
        )
        .route(
            "/sales-inquiries/{id}/imports/confirm",
            with_permission(
                post(sales_inquiry::import::sales_import_confirm),
                rbac,
                sales_inquiry::import::sales_import_confirm_permission_key(),
            ),
        )
}

fn sales_conversion(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-orders/{id}/inquiry-fees",
            with_permission(
                get(sales_inquiry::conversion::financial_inquiry_candidates),
                rbac,
                sales_inquiry::conversion::financial_inquiry_candidates_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/sales-basis",
            with_permission(
                get(sales_inquiry::conversion::sales_basis),
                rbac,
                sales_inquiry::conversion::sales_basis_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/fee-sales",
            with_permission(
                get(sales_inquiry::conversion::fee_sales),
                rbac,
                sales_inquiry::conversion::fee_sales_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/fee-allocations",
            with_permission(
                post(sales_inquiry::conversion::allocate_fees),
                rbac,
                sales_inquiry::conversion::allocate_fees_permission_key(),
            ),
        )
}

fn sales_recovery(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/sales-inquiries/receipts/{command_key}",
            with_permission(
                get(sales_inquiry::recovery::sales_create_recover),
                rbac,
                sales_inquiry::recovery::sales_create_recover_permission_key(),
            ),
        )
        .route(
            "/sales-inquiries/{id}/receipts/{command_key}",
            with_permission(
                get(sales_inquiry::recovery::sales_recover),
                rbac,
                sales_inquiry::recovery::sales_recover_permission_key(),
            ),
        )
}

fn procurement_reads(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries",
            with_permission(
                get(sales_inquiry::query::procurement_list),
                rbac,
                sales_inquiry::query::procurement_list_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}",
            with_permission(
                get(sales_inquiry::query::procurement_detail),
                rbac,
                sales_inquiry::query::procurement_detail_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}/receipts/{command_key}",
            with_permission(
                get(sales_inquiry::recovery::procurement_recover),
                rbac,
                sales_inquiry::recovery::procurement_recover_permission_key(),
            ),
        )
}

fn procurement_work(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/claim",
            with_permission(
                post(sales_inquiry::commands::procurement_claim),
                rbac,
                sales_inquiry::commands::procurement_claim_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}/supplement",
            with_permission(
                post(sales_inquiry::commands::procurement_supplement),
                rbac,
                sales_inquiry::commands::procurement_supplement_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}/reassign",
            with_permission(
                post(sales_inquiry::commands::procurement_reassign),
                rbac,
                sales_inquiry::commands::procurement_reassign_permission_key(),
            ),
        )
}

fn procurement_feedback(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/feedback",
            with_permission(
                put(sales_inquiry::commands::procurement_save_feedback),
                rbac,
                sales_inquiry::commands::procurement_save_feedback_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}/publish",
            with_permission(
                post(sales_inquiry::commands::procurement_publish),
                rbac,
                sales_inquiry::commands::procurement_publish_permission_key(),
            ),
        )
}

fn procurement_quotes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/internal-quotes",
            with_permission(
                get(sales_inquiry::query::procurement_internal_quotes),
                rbac,
                sales_inquiry::query::procurement_internal_quotes_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}/internal-quotes",
            with_permission(
                post(sales_inquiry::internal::save_quote),
                rbac,
                sales_inquiry::internal::save_quote_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}/internal-quotes/{quote_id}/attachments/{asset_id}",
            with_permission(
                get(sales_inquiry::internal::quote_attachment),
                rbac,
                sales_inquiry::internal::quote_attachment_permission_key(),
            ),
        )
}

fn procurement_fees(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/fee-bases",
            with_permission(
                get(sales_inquiry::internal::fee_bases),
                rbac,
                sales_inquiry::internal::fee_bases_permission_key(),
            ),
        )
        .route(
            "/procurement-inquiries/{id}/fee-bases",
            with_permission(
                post(sales_inquiry::internal::save_fee_basis),
                rbac,
                sales_inquiry::internal::save_fee_basis_permission_key(),
            ),
        )
        .route(
            "/purchase-orders/expense-sources/{id}/inquiry-fees",
            with_permission(
                get(sales_inquiry::conversion::expense_inquiry_candidates),
                rbac,
                sales_inquiry::conversion::expense_inquiry_candidates_permission_key(),
            ),
        )
}
