//! 域 D18 `receivable` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳（`Instant` 序列化为整数）；
//! 金额一律十进制字符串（`erp_core::money::Amount`）；业务日期为 `YYYY-MM-DD`。
//!
//! 契约来源：`erp-client/features/customer-receivables/types.ts`（W11）、
//! 客户往来页面；与客户端 camelCase/ISO 形态差异
//! 见 P3 PR「契约变更」一节（后端统一 snake_case + 秒级时间戳）。

mod command;
mod facts;
mod invoice;
mod query;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 排序方向。
pub use application_core::SortDir;
/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use application_core::normalize_sort;

pub use self::command::{
    CancelCustomerReceiptApprovalRequest, CommitCustomerReceiptRequest, CreateCustomerReceiptRequest,
    CreateReceivableAccountRequest, PostCustomerReceiptRequest, ReceiptAllocationLineRequest,
    SubmitCustomerReceiptRequest,
};
pub use self::facts::{ReceivableInvoiceFactView, ReceivableReceiptFactView};
pub use self::invoice::{
    CommitInvoiceRequest, CommitRedInvoiceRequest, CreateInvoiceRequest, PostInvoiceRequest,
    SalesInvoiceAllocationLineRequest,
};
pub use self::query::{
    CustomerReceiptListParams, CustomerReceiptListQuery, InvoiceListParams, InvoiceListQuery, InvoiceView,
    PageParams, ReceiptAllocationView, ReceiptOperatorKind, ReceivableAccountListParams,
    ReceivableAccountListQuery, ReceivableAccountSummaryView, ReceivableEntryView,
    SalesInvoiceAllocationView,
};

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::BusinessDate;
    use erp_core::money::Amount;
    use validator::Validate;

    use super::{
        CreateInvoiceRequest, CustomerReceiptListParams, InvoiceListParams, InvoiceView,
        ReceivableAccountListParams, SortDir, normalize_sort,
    };
    use crate::entity::receivable::{
        CustomerReceiptStatus, InvoiceDirection, InvoiceKind, InvoiceStatus, ReceivableAccountStatus,
    };

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) =
            normalize_sort(&Some(" created_at ".to_string()), &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);

        let (field, direction) = normalize_sort(&None, &Some("asc".to_string()), &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn receivable_account_list_params_normalize_filters_and_paging() {
        let params = ReceivableAccountListParams {
            scope_version: None,
            sales_owner_user_ids: None,
            operator_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            q: Some(" SO ".to_string()),
            account_id: None,
            customer_id: None,
            counterparty_party_id: None,
            status: Some(ReceivableAccountStatus::Open),
            sales_order_id: Some(" SO-1 ".to_string()),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("open_total".to_string()),
            sort_dir: Some("asc".to_string()),
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.status, Some(ReceivableAccountStatus::Open));
        assert_eq!(query.q.as_deref(), Some("SO"));
        assert_eq!(query.sales_order_id.as_deref(), Some("SO-1"));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "open_total");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = ReceivableAccountListParams {
            scope_version: None,
            sales_owner_user_ids: None,
            operator_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            q: None,
            account_id: None,
            customer_id: None,
            counterparty_party_id: None,
            status: None,
            sales_order_id: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn receipt_and_invoice_list_params_normalize() {
        let receipt = CustomerReceiptListParams {
            scope_version: None,
            sales_owner_user_ids: None,
            operator_user_ids: None,
            operator_kind: None,
            org_unit_ids: None,
            include_descendants: None,
            q: None,
            receipt_no: Some(" RC-1 ".to_string()),
            counterparty_party_id: None,
            status: Some(CustomerReceiptStatus::Posted),
            sales_order_id: None,
            receivable_account_id: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = receipt.normalized().unwrap();
        assert_eq!(query.receipt_no.as_deref(), Some("RC-1"));
        assert_eq!(query.status, Some(CustomerReceiptStatus::Posted));

        let invoice = InvoiceListParams {
            scope_version: None,
            sales_owner_user_ids: None,
            procurement_owner_user_ids: None,
            operator_user_ids: None,
            org_unit_ids: None,
            include_descendants: None,
            q: None,
            invoice_direction: Some(InvoiceDirection::Sales),
            invoice_kind: None,
            party_id: None,
            invoice_no: None,
            status: None,
            sales_order_id: None,
            receivable_account_id: None,
            page: None,
            page_size: Some(99),
            sort_by: None,
            sort_dir: None,
        };
        let query = invoice.normalized().unwrap();
        assert_eq!(query.invoice_direction, Some(InvoiceDirection::Sales));
        assert_eq!(query.paging.page_size, 99);
    }

    #[test]
    fn receipt_operator_requires_explicit_kind_and_org_needs_scope() {
        let missing_kind = CustomerReceiptListParams {
            scope_version: None,
            sales_owner_user_ids: None,
            operator_user_ids: serde_json::from_value(serde_json::json!("u-1")).unwrap(),
            operator_kind: None,
            org_unit_ids: None,
            include_descendants: None,
            q: None,
            receipt_no: None,
            counterparty_party_id: None,
            status: None,
            sales_order_id: None,
            receivable_account_id: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        assert!(missing_kind.normalized().is_err());

        let missing_org =
            CustomerReceiptListParams { include_descendants: Some(true), ..missing_kind.clone() };
        assert!(missing_org.normalized().is_err());

        assert!(
            serde_json::from_value::<CustomerReceiptListParams>(serde_json::json!({"owner": "someone"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<ReceivableAccountListParams>(
                serde_json::json!({"owner_user_ids": "someone"})
            )
            .is_err()
        );
    }

    /// 发票创建请求拒绝定义 ID / 审批人；视图不暴露审批区。
    #[test]
    fn invoice_create_and_view_have_no_approval_surface() {
        let valid = serde_json::json!({
            "invoice_direction": "sales",
            "invoice_kind": "blue",
            "party_id": "p-1",
            "invoice_no": "001",
            "invoice_date": "2026-08-06",
            "gross_amount": "100.00",
            "net_amount": "88.50",
            "tax_amount": "11.50"
        });
        assert!(serde_json::from_value::<CreateInvoiceRequest>(valid).is_ok());
        let forged = serde_json::json!({
            "invoice_direction": "sales",
            "invoice_kind": "blue",
            "party_id": "p-1",
            "invoice_no": "001",
            "invoice_date": "2026-08-06",
            "gross_amount": "100.00",
            "net_amount": "88.50",
            "tax_amount": "11.50",
            "definition_id": "forged",
            "assignee": "forged"
        });
        assert!(serde_json::from_value::<CreateInvoiceRequest>(forged).is_err());

        let view = InvoiceView {
            sales_invoice_request_id: None,
            id: "inv-1".into(),
            invoice_direction: InvoiceDirection::Sales,
            invoice_kind: InvoiceKind::Blue,
            party_id: "p-1".into(),
            invoice_code: None,
            invoice_no: "001".into(),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).expect("日期合法"),
            gross_amount: Amount::from_str("100").expect("金额合法"),
            net_amount: Amount::from_str("88.50").expect("金额合法"),
            tax_amount: Amount::from_str("11.50").expect("金额合法"),
            rounding_adjustment_amount: Amount::from_str("0").expect("金额合法"),
            rounding_reason: None,
            original_invoice_id: None,
            status: InvoiceStatus::Draft,
            version: 1,
            created_at: 1,
            allocated_total: Amount::from_str("0").expect("金额合法"),
            unallocated_amount: Amount::from_str("100").expect("金额合法"),
            allocations: Vec::new(),
        };
        let value = serde_json::to_value(&view).expect("视图可序列化");
        let object = value.as_object().expect("视图为对象");
        assert!(!object.contains_key("approval"));
        assert!(!object.contains_key("definition_id"));
        assert!(!object.contains_key("assignee"));
    }

    #[test]
    fn submit_and_cancel_requests_reject_client_assignee_choice() {
        use super::{CancelCustomerReceiptApprovalRequest, SubmitCustomerReceiptRequest};

        assert!(
            serde_json::from_value::<SubmitCustomerReceiptRequest>(serde_json::json!({
                "expected_version": 1,
                "idempotency_key": "k1",
                "allocations": [{"receivable_entry_id": "re-1", "allocated_amount": "10"}],
                "definition_id": "forged"
            }))
            .is_err()
        );
        let submit: SubmitCustomerReceiptRequest = serde_json::from_value(serde_json::json!({
            "expected_version": 1,
            "idempotency_key": "k1",
            "allocations": [{"receivable_entry_id": "re-1", "allocated_amount": "10"}]
        }))
        .unwrap();
        assert_eq!(submit.expected_version, 1);
        assert!(
            serde_json::from_value::<CancelCustomerReceiptApprovalRequest>(serde_json::json!({
                "expected_version": 1,
                "reason": "改金额",
                "idempotency_key": "k2",
                "assignee": "forged"
            }))
            .is_err()
        );
    }
}

pub mod invoice_request;
pub use invoice_request::*;
