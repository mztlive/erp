//! 销售和采购询价分别登记路由及权限，挂载于已认证的 /admin。
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use erp_identity::SharedRbacService;

use crate::app_state::AppState;
use crate::core::handler::sales_inquiry;
use crate::core::middleware::with_permission;
use crate::permissions;

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
            permissions::sales_inquiry::INTERNAL_FEE_BASIS_ATTACHMENT,
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
                permissions::sales_inquiry::QUERY_SALES_LIST,
            ),
        )
        .route(
            "/sales-inquiries/{id}",
            with_permission(
                get(sales_inquiry::query::sales_detail),
                rbac,
                permissions::sales_inquiry::QUERY_SALES_DETAIL,
            ),
        )
        .route(
            "/sales-inquiries/template-products",
            with_permission(
                post(sales_inquiry::query::sales_template_products),
                rbac,
                permissions::sales_inquiry::QUERY_SALES_TEMPLATE_PRODUCTS,
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
                permissions::sales_inquiry::COMMANDS_SALES_CREATE,
            ),
        )
        .route(
            "/sales-inquiries/{id}/draft",
            with_permission(
                put(sales_inquiry::commands::sales_save),
                rbac,
                permissions::sales_inquiry::COMMANDS_SALES_SAVE,
            ),
        )
        .route(
            "/sales-inquiries/{id}/submit",
            with_permission(
                post(sales_inquiry::commands::sales_submit),
                rbac,
                permissions::sales_inquiry::COMMANDS_SALES_SUBMIT,
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
                permissions::sales_inquiry::COMMANDS_SALES_ACCEPT,
            ),
        )
        .route(
            "/sales-inquiries/{id}/reinquire",
            with_permission(
                post(sales_inquiry::commands::sales_reinquire),
                rbac,
                permissions::sales_inquiry::COMMANDS_SALES_REINQUIRE,
            ),
        )
        .route(
            "/sales-inquiries/{id}/close",
            with_permission(
                post(sales_inquiry::commands::sales_close),
                rbac,
                permissions::sales_inquiry::COMMANDS_SALES_CLOSE,
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
                permissions::sales_inquiry::IMPORT_CREATION_CREATION_PREVIEW,
            )
            .layer(DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024)),
        )
        .route(
            "/sales-inquiries/imports/create",
            with_permission(
                post(sales_inquiry::import_creation::creation_confirm),
                rbac,
                permissions::sales_inquiry::IMPORT_CREATION_CREATION_CONFIRM,
            ),
        )
        .route(
            "/sales-inquiries/{id}/imports/preview",
            with_permission(
                post(sales_inquiry::import::sales_import_preview),
                rbac,
                permissions::sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            )
            .layer(DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024)),
        )
        .route(
            "/sales-inquiries/{id}/imports/confirm",
            with_permission(
                post(sales_inquiry::import::sales_import_confirm),
                rbac,
                permissions::sales_inquiry::IMPORT_SALES_IMPORT_CONFIRM,
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
                permissions::sales_inquiry::CONVERSION_FINANCIAL_INQUIRY_CANDIDATES,
            ),
        )
        .route(
            "/sales-inquiries/{id}/sales-basis",
            with_permission(
                get(sales_inquiry::conversion::sales_basis),
                rbac,
                permissions::sales_inquiry::CONVERSION_SALES_BASIS,
            ),
        )
        .route(
            "/sales-inquiries/{id}/fee-sales",
            with_permission(
                get(sales_inquiry::conversion::fee_sales),
                rbac,
                permissions::sales_inquiry::CONVERSION_FEE_SALES,
            ),
        )
        .route(
            "/sales-inquiries/{id}/fee-allocations",
            with_permission(
                post(sales_inquiry::conversion::allocate_fees),
                rbac,
                permissions::sales_inquiry::CONVERSION_ALLOCATE_FEES,
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
                permissions::sales_inquiry::RECOVERY_SALES_CREATE_RECOVER,
            ),
        )
        .route(
            "/sales-inquiries/{id}/receipts/{command_key}",
            with_permission(
                get(sales_inquiry::recovery::sales_recover),
                rbac,
                permissions::sales_inquiry::RECOVERY_SALES_RECOVER,
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
                permissions::sales_inquiry::QUERY_PROCUREMENT_LIST,
            ),
        )
        .route(
            "/procurement-inquiries/{id}",
            with_permission(
                get(sales_inquiry::query::procurement_detail),
                rbac,
                permissions::sales_inquiry::QUERY_PROCUREMENT_DETAIL,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/receipts/{command_key}",
            with_permission(
                get(sales_inquiry::recovery::procurement_recover),
                rbac,
                permissions::sales_inquiry::RECOVERY_PROCUREMENT_RECOVER,
            ),
        )
}

fn procurement_work(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/supplement",
            with_permission(
                post(sales_inquiry::commands::procurement_supplement),
                rbac,
                permissions::sales_inquiry::COMMANDS_PROCUREMENT_SUPPLEMENT,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/reassign",
            with_permission(
                post(sales_inquiry::commands::procurement_reassign),
                rbac,
                permissions::sales_inquiry::COMMANDS_PROCUREMENT_REASSIGN,
            ),
        )
}

fn procurement_feedback(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/renew",
            with_permission(
                post(sales_inquiry::commands::procurement_renew),
                rbac,
                permissions::sales_inquiry::COMMANDS_PROCUREMENT_RENEW,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/feedback",
            with_permission(
                put(sales_inquiry::commands::procurement_save_feedback),
                rbac,
                permissions::sales_inquiry::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/publish",
            with_permission(
                post(sales_inquiry::commands::procurement_publish),
                rbac,
                permissions::sales_inquiry::COMMANDS_PROCUREMENT_PUBLISH,
            ),
        )
}

fn procurement_quotes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/internal-quotes/import",
            with_permission(
                post(sales_inquiry::internal::import_quotes),
                rbac,
                permissions::sales_inquiry::INTERNAL_IMPORT_QUOTES,
            )
            .layer(DefaultBodyLimit::max(20 * 1024 * 1024)),
        )
        .route(
            "/procurement-inquiries/{id}/internal-quotes",
            with_permission(
                get(sales_inquiry::query::procurement_internal_quotes),
                rbac,
                permissions::sales_inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/internal-quotes",
            with_permission(
                post(sales_inquiry::internal::save_quote),
                rbac,
                permissions::sales_inquiry::INTERNAL_SAVE_QUOTE,
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
                permissions::sales_inquiry::INTERNAL_FEE_BASES,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/fee-bases",
            with_permission(
                post(sales_inquiry::internal::save_fee_basis),
                rbac,
                permissions::sales_inquiry::INTERNAL_SAVE_FEE_BASIS,
            ),
        )
        .route(
            "/purchase-orders/expense-sources/{id}/inquiry-fees",
            with_permission(
                get(sales_inquiry::conversion::expense_inquiry_candidates),
                rbac,
                permissions::sales_inquiry::CONVERSION_EXPENSE_INQUIRY_CANDIDATES,
            ),
        )
}
