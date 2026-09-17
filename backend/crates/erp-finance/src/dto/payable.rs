//! 域 D19 `payable` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数扁平传递；时间一律秒级
//! 时间戳；金额一律十进制字符串；业务日期为 `YYYY-MM-DD`。
//! 契约来源：`erp-client/features/supplier-payables/types.ts`（W12）。
//!
//! 子域拆分（与 `receivable` 同构）：`account`（往来子账）、`payment`（付款单）、
//! `invoice`（进项发票分配）；公开类型名与 serde 形态保持不变。

pub mod account;
pub mod invoice;
pub mod payment;

pub use account::{
    CreatePayableAccountRequest, PageParams, PageView, PayableAccountListParams, PayableAccountListQuery,
    PayableAccountSummaryView, PayableAccountView, PayableEntryView, PaymentRecipientRevealView,
    PaymentRecipientView, PaymentReversalStatus, RevealPaymentRecipientRequest, SortDir,
};
pub use invoice::{
    PurchaseInvoiceAllocationLineRequest, PurchaseInvoiceAllocationListParams,
    PurchaseInvoiceAllocationListQuery, PurchaseInvoiceAllocationView, PurchaseInvoiceRegisteredView,
    RegisterPurchaseInvoiceRequest,
};
pub use payment::{
    CommitSupplierPaymentRequest, CreateSupplierPaymentRequest, PaymentAllocationLineRequest,
    PaymentAllocationView, PaymentExecutionTaskRef, SupplierPaymentBankReceiptView,
    SupplierPaymentListParams, SupplierPaymentListQuery, SupplierPaymentReversalView, SupplierPaymentView,
};

#[cfg(test)]
mod tests {
    use validator::Validate;

    use super::account::{PayableAccountListParams, SortDir, normalize_sort};
    use super::invoice::PurchaseInvoiceAllocationListParams;
    use super::payment::SupplierPaymentListParams;
    use crate::entity::payable::{PayableAccountStatus, PayableSourceType, SupplierPaymentStatus};

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn payable_account_list_params_normalize_filters_and_paging() {
        let params = PayableAccountListParams {
            source_type: Some(PayableSourceType::PurchaseOrder),
            status: Some(PayableAccountStatus::Open),
            page: Some(2),
            page_size: Some(50),
            sort_by: Some("open_total".to_string()),
            sort_dir: Some("asc".to_string()),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.source_type, Some(PayableSourceType::PurchaseOrder));
        assert_eq!(query.status, Some(PayableAccountStatus::Open));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "open_total");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn payment_and_allocation_list_params_normalize() {
        let payment = SupplierPaymentListParams {
            q: Some(" 狮峰 ".to_string()),
            payment_no: Some(" PAY-1 ".to_string()),
            status: Some(SupplierPaymentStatus::Posted),
            ..Default::default()
        };
        let query = payment.normalized().unwrap();
        assert_eq!(query.payment_no.as_deref(), Some("PAY-1"));
        assert_eq!(query.q.as_deref(), Some("狮峰"));
        assert_eq!(query.status, Some(SupplierPaymentStatus::Posted));

        let allocations = PurchaseInvoiceAllocationListParams {
            page: Some(1),
            page_size: Some(25),
            sort_by: Some("created_at".to_string()),
            ..Default::default()
        };
        let query = allocations.normalized().unwrap();
        assert_eq!(query.paging.page_size, 25);
    }

    /// 分配实体转视图时来源展示字段为空，由展示装配补全，避免把分录 ID 当单号。
    #[test]
    fn payment_allocation_view_leaves_source_blank_before_enrichment() {
        use std::str::FromStr;

        use erp_core::common::time::Instant;
        use erp_core::ids::{PayableEntryId, PaymentAllocationId, SupplierPaymentId};
        use erp_core::money::Amount;

        use super::payment::PaymentAllocationView;
        use crate::entity::payable::{AllocationAction, PaymentAllocation, PaymentAllocationData};

        let allocation = PaymentAllocation::new(
            PaymentAllocationId::new("alloc-1"),
            PaymentAllocationData {
                supplier_payment_id: SupplierPaymentId::new("pay-1"),
                payable_entry_id: PayableEntryId::new("pe-1"),
                allocation_seq: 1,
                allocation_action: AllocationAction::Apply,
                allocated_amount: Amount::from_str("10.00").unwrap(),
                allocated_at: Instant::from_unix_secs(1_700_000_000),
                reverses_allocation_id: None,
            },
        )
        .unwrap();
        let view = PaymentAllocationView::from(&allocation);
        assert_eq!(view.payable_entry_id, "pe-1");
        assert!(view.payable_account_id.is_none());
        assert!(view.source_document_no.is_none());
        assert!(view.source_document_id.is_none());
    }

    /// 付款提交必须携带页面已核对的收款账户身份与版本。
    #[test]
    fn payment_commit_requires_expected_recipient_account() {
        use super::payment::CommitSupplierPaymentRequest;

        let without_recipient = serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": "asset-1"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
            "idempotency_key": "k1"
        });
        assert!(serde_json::from_value::<CommitSupplierPaymentRequest>(without_recipient).is_err());

        let blank_recipient = serde_json::from_value::<CommitSupplierPaymentRequest>(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "expected_payee_bank_account_id": " ",
            "expected_payee_bank_account_version": 1,
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": "asset-1"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
            "idempotency_key": "k1"
        }))
        .expect("空白收款账户可完成协议反序列化");
        assert!(blank_recipient.validate().is_err());

        let missing_version = serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "expected_payee_bank_account_id": "bank-1",
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": "asset-1"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10"}],
            "idempotency_key": "k1"
        });
        assert!(serde_json::from_value::<CommitSupplierPaymentRequest>(missing_version).is_err());
    }

    /// 附加付款任务缺省为空，旧客户端不传字段时仍按单任务付款。
    #[test]
    fn payment_commit_defaults_additional_work_items_to_empty() {
        use super::payment::CommitSupplierPaymentRequest;

        let request = serde_json::from_value::<CommitSupplierPaymentRequest>(serde_json::json!({
            "work_item_id": "wi-1",
            "expected_task_version": "3",
            "expected_payee_bank_account_id": "bank-1",
            "expected_payee_bank_account_version": 1,
            "payment": {
                "payment_no": "FK-1",
                "supplier_id": "supplier-1",
                "paid_at": 1,
                "amount": "10.00",
                "bank_reference": null,
                "bank_receipt_asset_id": "asset-1"
            },
            "allocations": [{"payable_entry_id": "pe-1", "allocated_amount": "10.00"}],
            "idempotency_key": "k1"
        }))
        .expect("旧付款命令必须可反序列化");
        assert!(request.additional_work_items.is_empty());
        assert!(request.validate().is_ok());
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params =
            PayableAccountListParams { page: Some(0), page_size: Some(u32::MAX), ..Default::default() };
        assert!(params.validate().is_err());
    }

    /// 请求分配行转换为领域 pending 时保持输入顺序，且正数校验由实体承担。
    #[test]
    fn payment_allocation_line_converts_to_pending_in_input_order() {
        use std::str::FromStr;

        use erp_core::ids::PayableEntryId;
        use erp_core::money::Amount;

        use super::payment::PaymentAllocationLineRequest;
        use crate::entity::payable::PendingPaymentAllocation;

        let lines = [
            PaymentAllocationLineRequest {
                payable_entry_id: PayableEntryId::new("pe-2"),
                allocated_amount: Amount::from_str("20.00").unwrap(),
            },
            PaymentAllocationLineRequest {
                payable_entry_id: PayableEntryId::new("pe-1"),
                allocated_amount: Amount::from_str("10.00").unwrap(),
            },
        ];
        let pending: Vec<PendingPaymentAllocation> = lines
            .iter()
            .map(PaymentAllocationLineRequest::to_pending)
            .collect::<Result<_, _>>()
            .expect("正数分配必须通过");
        assert_eq!(pending[0].payable_entry_id, PayableEntryId::new("pe-2"));
        assert_eq!(pending[0].allocated_amount, Amount::from_str("20.00").unwrap());
        assert_eq!(pending[1].payable_entry_id, PayableEntryId::new("pe-1"));
        assert_eq!(pending[1].allocated_amount, Amount::from_str("10.00").unwrap());
    }

    /// 零/负金额不得通过请求行转换，错误文案保持实体合同。
    #[test]
    fn payment_allocation_line_rejects_zero_or_negative_amount() {
        use std::str::FromStr;

        use erp_core::ids::PayableEntryId;
        use erp_core::money::Amount;

        use super::payment::PaymentAllocationLineRequest;

        let zero = PaymentAllocationLineRequest {
            payable_entry_id: PayableEntryId::new("pe-1"),
            allocated_amount: Amount::from_str("0.00").unwrap(),
        };
        let zero_err = zero.to_pending().unwrap_err();
        assert!(zero_err.to_string().contains("付款金额必须为正数"));

        let negative = PaymentAllocationLineRequest {
            payable_entry_id: PayableEntryId::new("pe-1"),
            allocated_amount: Amount::from_str("-1.00").unwrap(),
        };
        let negative_err = negative.to_pending().unwrap_err();
        assert!(negative_err.to_string().contains("付款金额必须为正数"));
    }
}

#[cfg(test)]
mod reversal_status_wire_tests {
    use super::account::PaymentReversalStatus;

    /// 财务消费方状态映射必须保持审批态的特殊大写 HTTP 编码。
    #[test]
    fn reversal_status_preserves_existing_wire_values() {
        for (status, value) in [
            (PaymentReversalStatus::Draft, "draft"),
            (PaymentReversalStatus::InApproval, "IN_APPROVAL"),
            (PaymentReversalStatus::Posted, "posted"),
            (PaymentReversalStatus::Reversed, "reversed"),
        ] {
            assert_eq!(serde_json::to_value(status).unwrap(), value);
            assert_eq!(
                serde_json::from_value::<PaymentReversalStatus>(serde_json::json!(value)).unwrap(),
                status
            );
        }
    }
}
