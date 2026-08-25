//! 域 D18 `receivable` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/receivable-accounts`、
//! `/admin/customer-receipts`、`/admin/invoices`、`/admin/receivable-funds-reviews`；
//! 每条路由统一走 JWT + RBAC（`with_permission`），handler 标注
//! `#[permission_macros::permission]`。

use axum::{
    routing::{get, post},
    Router,
};
use services::iam::SharedRbacService;

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
            "/receivable-funds-reviews",
            with_permission(
                post(receivable::receivable_funds_review_complete),
                rbac,
                receivable::receivable_funds_review_complete_permission_key(),
            ),
        )
        .route(
            "/card-funds-review/receipts",
            with_permission(
                post(receivable::card_funds_receipt_register),
                rbac,
                receivable::card_funds_receipt_register_permission_key(),
            ),
        )
        .route(
            "/card-funds-review/invoices",
            with_permission(
                post(receivable::card_funds_invoice_register),
                rbac,
                receivable::card_funds_invoice_register_permission_key(),
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
