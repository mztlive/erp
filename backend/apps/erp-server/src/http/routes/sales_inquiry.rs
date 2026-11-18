//! 销售和采购询价分别登记路由及权限，挂载于已认证的 /admin。
use axum::Router;
use axum::extract::DefaultBodyLimit;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::sales_inquiry;
use crate::http::routes::permission_route;
use crate::http::state::HttpState;

/// 装配询价路由。
/// 返回销售与采购独立入口。
pub fn routes() -> Router<HttpState> {
    Router::new()
        .merge(sales_reads())
        .merge(sales_drafts())
        .merge(sales_feedback())
        .merge(sales_imports())
        .merge(sales_conversion())
        .merge(sales_recovery())
        .merge(procurement_reads())
        .merge(procurement_work())
        .merge(procurement_feedback())
        .merge(procurement_quotes())
        .merge(procurement_fees())
        .merge(procurement_fee_attachments())
}

fn procurement_fee_attachments() -> Router<HttpState> {
    Router::new().route(
        "/procurement-inquiries/{id}/fee-bases/{basis_id}/attachments/{asset_id}",
        permission_route(
            get(sales_inquiry::internal::fee_basis_attachment),
            &permissions::sales_inquiry::INTERNAL_FEE_BASES,
        ),
    )
}

fn sales_reads() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-inquiries",
            permission_route(
                get(sales_inquiry::query::sales_list),
                &permissions::sales_inquiry::QUERY_SALES_LIST,
            ),
        )
        .route(
            "/sales-inquiries/{id}",
            permission_route(
                get(sales_inquiry::query::sales_detail),
                &permissions::sales_inquiry::QUERY_SALES_DETAIL,
            ),
        )
        .route(
            "/sales-inquiries/template-products",
            permission_route(
                post(sales_inquiry::query::sales_template_products),
                &permissions::sales_inquiry::COMMANDS_SALES_CREATE,
            ),
        )
}

fn sales_drafts() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-inquiries",
            permission_route(
                post(sales_inquiry::commands::sales_create),
                &permissions::sales_inquiry::COMMANDS_SALES_CREATE,
            ),
        )
        .route(
            "/sales-inquiries/{id}/draft",
            permission_route(
                put(sales_inquiry::commands::sales_save),
                &permissions::sales_inquiry::COMMANDS_SALES_SAVE,
            ),
        )
        .route(
            "/sales-inquiries/{id}/submit",
            permission_route(
                post(sales_inquiry::commands::sales_submit),
                &permissions::sales_inquiry::COMMANDS_SALES_SUBMIT,
            ),
        )
}

fn sales_feedback() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-inquiries/{id}/accept",
            permission_route(
                post(sales_inquiry::commands::sales_accept),
                &permissions::sales_inquiry::COMMANDS_SALES_ACCEPT,
            ),
        )
        .route(
            "/sales-inquiries/{id}/reinquire",
            permission_route(
                post(sales_inquiry::commands::sales_reinquire),
                &permissions::sales_inquiry::COMMANDS_SALES_REINQUIRE,
            ),
        )
        .route(
            "/sales-inquiries/{id}/close",
            permission_route(
                post(sales_inquiry::commands::sales_close),
                &permissions::sales_inquiry::COMMANDS_SALES_CLOSE,
            ),
        )
}

fn sales_imports() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-inquiries/imports/preview",
            permission_route(
                post(sales_inquiry::import_creation::creation_preview),
                &permissions::sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            )
            .layer(DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024)),
        )
        .route(
            "/sales-inquiries/imports/create",
            permission_route(
                post(sales_inquiry::import_creation::creation_confirm),
                &permissions::sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            ),
        )
        .route(
            "/sales-inquiries/{id}/imports/preview",
            permission_route(
                post(sales_inquiry::import::sales_import_preview),
                &permissions::sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            )
            .layer(DefaultBodyLimit::max(5 * 1024 * 1024 + 64 * 1024)),
        )
        .route(
            "/sales-inquiries/{id}/imports/confirm",
            permission_route(
                post(sales_inquiry::import::sales_import_confirm),
                &permissions::sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            ),
        )
}

fn sales_conversion() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-orders/{id}/inquiry-fees",
            permission_route(
                get(sales_inquiry::conversion::financial_inquiry_candidates),
                &permissions::cost::ENTRY_CREATE,
            ),
        )
        .route(
            "/sales-inquiries/{id}/sales-basis",
            permission_route(
                get(sales_inquiry::conversion::sales_basis),
                &permissions::sales_inquiry::CONVERSION_SALES_BASIS,
            ),
        )
        .route(
            "/sales-inquiries/{id}/fee-sales",
            permission_route(
                get(sales_inquiry::conversion::fee_sales),
                &permissions::sales_inquiry::QUERY_SALES_DETAIL,
            ),
        )
        .route(
            "/sales-inquiries/{id}/fee-allocations",
            permission_route(
                post(sales_inquiry::conversion::allocate_fees),
                &permissions::sales_inquiry::CONVERSION_SALES_BASIS,
            ),
        )
}

fn sales_recovery() -> Router<HttpState> {
    Router::new()
        .route(
            "/sales-inquiries/receipts/{command_key}",
            permission_route(
                get(sales_inquiry::recovery::sales_create_recover),
                &permissions::sales_inquiry::RECOVERY_SALES_RECOVER,
            ),
        )
        .route(
            "/sales-inquiries/{id}/receipts/{command_key}",
            permission_route(
                get(sales_inquiry::recovery::sales_recover),
                &permissions::sales_inquiry::RECOVERY_SALES_RECOVER,
            ),
        )
}

fn procurement_reads() -> Router<HttpState> {
    Router::new()
        .route(
            "/procurement-inquiries",
            permission_route(
                get(sales_inquiry::query::procurement_list),
                &permissions::sales_inquiry::QUERY_PROCUREMENT_LIST,
            ),
        )
        .route(
            "/procurement-inquiries/{id}",
            permission_route(
                get(sales_inquiry::query::procurement_detail),
                &permissions::sales_inquiry::QUERY_PROCUREMENT_DETAIL,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/receipts/{command_key}",
            permission_route(
                get(sales_inquiry::recovery::procurement_recover),
                &permissions::sales_inquiry::RECOVERY_PROCUREMENT_RECOVER,
            ),
        )
}

fn procurement_work() -> Router<HttpState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/supplement",
            permission_route(
                post(sales_inquiry::commands::procurement_supplement),
                &permissions::sales_inquiry::COMMANDS_PROCUREMENT_SUPPLEMENT,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/reassign",
            permission_route(
                post(sales_inquiry::commands::procurement_reassign),
                &permissions::sales_inquiry::COMMANDS_PROCUREMENT_REASSIGN,
            ),
        )
}

fn procurement_feedback() -> Router<HttpState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/renew",
            permission_route(
                post(sales_inquiry::commands::procurement_renew),
                &permissions::sales_inquiry::COMMANDS_PROCUREMENT_RENEW,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/feedback",
            permission_route(
                put(sales_inquiry::commands::procurement_save_feedback),
                &permissions::sales_inquiry::COMMANDS_PROCUREMENT_SAVE_FEEDBACK,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/publish",
            permission_route(
                post(sales_inquiry::commands::procurement_publish),
                &permissions::sales_inquiry::COMMANDS_PROCUREMENT_RENEW,
            ),
        )
}

fn procurement_quotes() -> Router<HttpState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/internal-quotes/import",
            permission_route(
                post(sales_inquiry::internal::import_quotes),
                &permissions::sales_inquiry::INTERNAL_FEE_BASES,
            )
            .layer(DefaultBodyLimit::max(20 * 1024 * 1024)),
        )
        .route(
            "/procurement-inquiries/{id}/internal-quotes",
            permission_route(
                get(sales_inquiry::query::procurement_internal_quotes),
                &permissions::sales_inquiry::INTERNAL_FEE_BASES,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/internal-quotes",
            permission_route(
                post(sales_inquiry::internal::save_quote),
                &permissions::sales_inquiry::INTERNAL_FEE_BASES,
            ),
        )
}

fn procurement_fees() -> Router<HttpState> {
    Router::new()
        .route(
            "/procurement-inquiries/{id}/fee-bases",
            permission_route(
                get(sales_inquiry::internal::fee_bases),
                &permissions::sales_inquiry::INTERNAL_FEE_BASES,
            ),
        )
        .route(
            "/procurement-inquiries/{id}/fee-bases",
            permission_route(
                post(sales_inquiry::internal::save_fee_basis),
                &permissions::sales_inquiry::INTERNAL_FEE_BASES,
            ),
        )
        .route(
            "/purchase-orders/expense-sources/{id}/inquiry-fees",
            permission_route(
                get(sales_inquiry::conversion::expense_inquiry_candidates),
                &permissions::purchase_order::CREATE,
            ),
        )
}
