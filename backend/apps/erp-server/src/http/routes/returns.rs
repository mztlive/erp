//! 域 D21 `returns` 管理端路由。
//!
//! 经 `admin.rs` 的 `/admin` nest 后，最终路径为 `/admin/sales-return-cases`、
//! `/admin/purchase-return-orders`、`/admin/customer-refunds`、
//! `/admin/supplier-refunds`、`/admin/receipt-reversals`、`/admin/payment-reversals`；
//! 每条路由统一走 JWT 认证；业务授权由用例执行，目录由 `permission_route` 提供。

use axum::Router;
use axum::routing::{get, post, put};
use erp_permissions as permissions;

use crate::http::handler::returns;
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
    Router::new()
        .merge(sales_return_routes())
        .merge(purchase_return_routes())
        .merge(customer_refund_routes())
        .merge(customer_refund_command_routes())
        .merge(supplier_refund_routes())
        .merge(receipt_reversal_routes())
        .merge(payment_reversal_routes())
}

/// 销售退货与草稿维护路由。
fn sales_return_routes() -> Router<HttpState> {
    Router::new()
        .merge(draft_update_routes())
        .route(
            "/sales-return-cases",
            permission_route(
                get(returns::sales_return::sales_return_case_list),
                &permissions::returns::SALES_RETURN_SALES_RETURN_CASE_LIST,
            ),
        )
        .route(
            "/sales-return-cases",
            permission_route(
                post(returns::sales_return::sales_return_case_create),
                &permissions::returns::SALES_RETURN_SALES_RETURN_CASE_CREATE,
            ),
        )
        .route(
            "/sales-return-cases/{id}",
            permission_route(
                get(returns::sales_return::sales_return_case_detail),
                &permissions::returns::SALES_RETURN_SALES_RETURN_CASE_DETAIL,
            ),
        )
}

/// 采购退货查询与确认路由。
fn purchase_return_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/purchase-orders/{id}/return-finance/sync",
            permission_route(
                post(returns::purchase_return::purchase_return_finance_sync),
                &permissions::returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE,
            ),
        )
        .route(
            "/purchase-orders/{id}/direct-returns",
            permission_route(
                post(returns::purchase_return::purchase_direct_return_confirm),
                &permissions::returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE,
            ),
        )
        .route(
            "/purchase-return-orders",
            permission_route(
                get(returns::purchase_return::purchase_return_order_list),
                &permissions::returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_LIST,
            ),
        )
        .route(
            "/purchase-return-orders",
            permission_route(
                post(returns::purchase_return::purchase_return_order_create),
                &permissions::returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE,
            ),
        )
        .route(
            "/purchase-return-orders/{id}",
            permission_route(
                get(returns::purchase_return::purchase_return_order_detail),
                &permissions::returns::PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL,
            ),
        )
}

/// 客户退款查询、创建与确认路由。
fn customer_refund_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-refunds",
            permission_route(
                get(returns::customer_refund::customer_refund_list),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_LIST,
            ),
        )
        .route(
            "/customer-refunds/commit",
            permission_route(
                post(returns::customer_refund::customer_refund_commit),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
            ),
        )
        .route(
            "/customer-refunds",
            permission_route(
                post(returns::customer_refund::customer_refund_create),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_CREATE,
            ),
        )
        .route(
            "/customer-refunds/{id}",
            permission_route(
                get(returns::customer_refund::customer_refund_detail),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL,
            ),
        )
}

/// 客户退款提交、撤回与记账路由。
fn customer_refund_command_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-refunds/{id}/submit",
            permission_route(
                post(returns::customer_refund::customer_refund_submit),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
            ),
        )
        .route(
            "/customer-refunds/{id}/cancel-approval",
            permission_route(
                post(returns::customer_refund::customer_refund_cancel_approval),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/customer-refunds/{id}/post",
            permission_route(
                post(returns::customer_refund::customer_refund_post),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_POST,
            ),
        )
}

/// 供应商退款读取与办理路由。
fn supplier_refund_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/supplier-refunds",
            permission_route(
                post(returns::supplier_refund::supplier_refund_create),
                &permissions::returns::SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE,
            ),
        )
        .route(
            "/supplier-refunds/commit",
            permission_route(
                post(returns::supplier_refund::supplier_refund_commit),
                &permissions::returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            ),
        )
        .route(
            "/supplier-refunds/{id}",
            permission_route(
                get(returns::supplier_refund::supplier_refund_detail),
                &permissions::returns::SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL,
            ),
        )
        .route(
            "/supplier-refunds/{id}/submit",
            permission_route(
                post(returns::supplier_refund::supplier_refund_submit),
                &permissions::returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            ),
        )
        .route(
            "/supplier-refunds/{id}/cancel-approval",
            permission_route(
                post(returns::supplier_refund::supplier_refund_cancel_approval),
                &permissions::returns::SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/supplier-refunds/{id}/post",
            permission_route(
                post(returns::supplier_refund::supplier_refund_post),
                &permissions::returns::SUPPLIER_REFUND_SUPPLIER_REFUND_POST,
            ),
        )
}

/// 收款冲销读取与办理路由。
fn receipt_reversal_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/receipt-reversals",
            permission_route(
                post(returns::receipt_reversal::receipt_reversal_create),
                &permissions::returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE,
            ),
        )
        .route(
            "/receipt-reversals/commit",
            permission_route(
                post(returns::receipt_reversal::receipt_reversal_commit),
                &permissions::returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            ),
        )
        .route(
            "/receipt-reversals/{id}",
            permission_route(
                get(returns::receipt_reversal::receipt_reversal_detail),
                &permissions::returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL,
            ),
        )
        .route(
            "/receipt-reversals/{id}/submit",
            permission_route(
                post(returns::receipt_reversal::receipt_reversal_submit),
                &permissions::returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            ),
        )
        .route(
            "/receipt-reversals/{id}/cancel-approval",
            permission_route(
                post(returns::receipt_reversal::receipt_reversal_cancel_approval),
                &permissions::returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/receipt-reversals/{id}/post",
            permission_route(
                post(returns::receipt_reversal::receipt_reversal_post),
                &permissions::returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_POST,
            ),
        )
}

/// 付款冲销读取与办理路由。
fn payment_reversal_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/payment-reversals",
            permission_route(
                post(returns::payment_reversal::payment_reversal_create),
                &permissions::returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE,
            ),
        )
        .route(
            "/payment-reversals/commit",
            permission_route(
                post(returns::payment_reversal::payment_reversal_commit),
                &permissions::returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
            ),
        )
        .route(
            "/payment-reversals/{id}",
            permission_route(
                get(returns::payment_reversal::payment_reversal_detail),
                &permissions::returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL,
            ),
        )
        .route(
            "/payment-reversals/{id}/submit",
            permission_route(
                post(returns::payment_reversal::payment_reversal_submit),
                &permissions::returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
            ),
        )
        .route(
            "/payment-reversals/{id}/cancel-approval",
            permission_route(
                post(returns::payment_reversal::payment_reversal_cancel_approval),
                &permissions::returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL,
            ),
        )
        .route(
            "/payment-reversals/{id}/post",
            permission_route(
                post(returns::payment_reversal::payment_reversal_post),
                &permissions::returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_POST,
            ),
        )
}

/// 退换退款单据草稿修改路由。
///
/// # 参数
/// 无。
/// # 返回
/// 返回逐操作声明权限目录信息并保留上传限制的路由集合。
/// # 错误
/// 无；业务错误由各处理器返回。
fn draft_update_routes() -> Router<HttpState> {
    Router::new()
        .route(
            "/customer-refunds/{id}",
            permission_route(
                put(returns::draft_update::customer_refund_update),
                &permissions::returns::CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
            ),
        )
        .route(
            "/supplier-refunds/{id}",
            permission_route(
                put(returns::draft_update::supplier_refund_update),
                &permissions::returns::SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            ),
        )
        .route(
            "/receipt-reversals/{id}",
            permission_route(
                put(returns::draft_update::receipt_reversal_update),
                &permissions::returns::RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            ),
        )
        .route(
            "/payment-reversals/{id}",
            permission_route(
                put(returns::draft_update::payment_reversal_update),
                &permissions::returns::PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
            ),
        )
}
