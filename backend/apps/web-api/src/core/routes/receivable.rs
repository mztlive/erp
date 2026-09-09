//! 域 D18 `receivable` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/receivable-accounts`、
//! `/admin/customer-receipts`、`/admin/invoices`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use erp_identity::SharedRbacService;

use crate::{
    app_state::AppState,
    core::{handler::receivable, middleware::with_permission},
};

/// 返回本域管理端路由集合。
///
/// # 参数
/// * `rbac` - 共享 Casbin RBAC 服务
///
/// # 返回
/// 返回挂载了权限校验层的路由集合。
pub fn routes(rbac: &SharedRbacService) -> Router<AppState> {
    Router::new()
        .route(
            "/receivable-accounts",
            with_permission(
                get(receivable::receivable_account_list),
                rbac,
                receivable::receivable_account_list_permission_key(),
            ),
        )
        .route(
            "/receivable-accounts",
            with_permission(
                post(receivable::receivable_account_create),
                rbac,
                receivable::receivable_account_create_permission_key(),
            ),
        )
        .route(
            "/receivable-accounts/{id}",
            with_permission(
                get(receivable::receivable_account_detail),
                rbac,
                receivable::receivable_account_detail_permission_key(),
            ),
        )
        .route(
            "/customer-receipts",
            with_permission(
                get(receivable::customer_receipt_list),
                rbac,
                receivable::customer_receipt_list_permission_key(),
            ),
        )
        .route(
            "/customer-receipts",
            with_permission(
                post(receivable::customer_receipt_create),
                rbac,
                receivable::customer_receipt_create_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}",
            with_permission(
                get(receivable::customer_receipt_detail),
                rbac,
                receivable::customer_receipt_detail_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/commit",
            with_permission(
                post(receivable::customer_receipt_commit),
                rbac,
                receivable::customer_receipt_commit_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}/submit",
            with_permission(
                post(receivable::customer_receipt_submit),
                rbac,
                receivable::customer_receipt_submit_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}/cancel-approval",
            with_permission(
                post(receivable::customer_receipt_cancel_approval),
                rbac,
                receivable::customer_receipt_cancel_approval_permission_key(),
            ),
        )
        .route(
            "/customer-receipts/{id}/post",
            with_permission(
                post(receivable::customer_receipt_post),
                rbac,
                receivable::customer_receipt_post_permission_key(),
            ),
        )
        .route(
            "/invoices",
            with_permission(
                get(receivable::invoice_list),
                rbac,
                receivable::invoice_list_permission_key(),
            ),
        )
        .route(
            "/invoices",
            with_permission(
                post(receivable::invoice_create),
                rbac,
                receivable::invoice_create_permission_key(),
            ),
        )
        .route(
            "/invoices/{id}",
            with_permission(
                get(receivable::invoice_detail),
                rbac,
                receivable::invoice_detail_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests",
            with_permission(
                get(receivable::invoice_request::invoice_request_list),
                rbac,
                receivable::invoice_request::invoice_request_list_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/submit",
            with_permission(
                post(receivable::invoice_request::invoice_request_submit),
                rbac,
                receivable::invoice_request::invoice_request_submit_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/{id}",
            with_permission(
                get(receivable::invoice_request::invoice_request_detail),
                rbac,
                receivable::invoice_request::invoice_request_detail_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/{id}/cancel",
            with_permission(
                post(receivable::invoice_request::invoice_request_cancel),
                rbac,
                receivable::invoice_request::invoice_request_cancel_permission_key(),
            ),
        )
        .route(
            "/sales-invoice-requests/amounts/{id}",
            with_permission(
                get(receivable::invoice_request::invoice_request_amounts),
                rbac,
                receivable::invoice_request::invoice_request_amounts_permission_key(),
            ),
        )
        .route(
            "/invoices/commit",
            with_permission(
                post(receivable::invoice_commit),
                rbac,
                receivable::invoice_commit_permission_key(),
            ),
        )
        .route(
            "/invoices/{id}/post",
            with_permission(
                post(receivable::invoice_post),
                rbac,
                receivable::invoice_post_permission_key(),
            ),
        )
        .route(
            "/invoices/{id}/red-issue",
            with_permission(
                post(receivable::invoice_red_issue),
                rbac,
                receivable::invoice_red_issue_permission_key(),
            ),
        )
}

#[cfg(test)]
mod tests {
    /// 客户回款路由暴露提交与撤回，不再把过账当客户端旁路入口。
    #[test]
    fn customer_receipt_routes_expose_submit_and_cancel() {
        let production = include_str!("receivable.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产路由必须存在");
        assert!(production.contains("/customer-receipts/{id}/submit"));
        assert!(production.contains("/customer-receipts/{id}/cancel-approval"));
        assert!(production.contains("customer_receipt_submit"));
        assert!(production.contains("customer_receipt_cancel_approval"));
        assert!(!production.contains("PENDING_REVIEW"));
    }
}

#[cfg(test)]
mod retired_review_routes_tests {
    /// 复核接口撤销后，正常回款、发票及应收接口必须继续保留。
    #[test]
    fn funds_review_routes_are_removed_and_finance_routes_remain() {
        let source = include_str!("receivable.rs")
            .split("#[cfg(test)]")
            .next()
            .unwrap();
        assert!(!source.contains("/receivable-funds-reviews"));
        assert!(!source.contains("/card-funds-review/"));
        for path in [
            "/receivable-accounts",
            "/customer-receipts/commit",
            "/customer-receipts/{id}/submit",
            "/invoices/commit",
            "/invoices/{id}/red-issue",
        ] {
            assert!(source.contains(path));
        }
    }
}
