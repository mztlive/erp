//! Consumer port for business-document registration facts used by support.

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Serialization codes of workflow `DocumentType` consumed by support.
///
/// Support does not depend on `erp-workflow`. Composition adapters map these
/// codes to the workflow catalog; this list is the consumer snapshot of the
/// twenty frozen snake_case variants. Entries are sorted for binary search;
/// keep them sorted when adding new codes.
pub const BUSINESS_DOCUMENT_TYPE_CODES: &[&str] = &[
    "customer_acceptance",
    "customer_receipt",
    "customer_refund",
    "delivery",
    "electronic_delivery",
    "invoice",
    "payment_reversal",
    "purchase_change_order",
    "purchase_order",
    "purchase_receipt",
    "purchase_return_order",
    "receipt_reversal",
    "sales_change_order",
    "sales_order",
    "sales_return_case",
    "service_fulfillment",
    "stock_adjustment",
    "supplier_payment",
    "supplier_refund",
    "voucher_sales_order",
];

/// Return whether `object_type` is a frozen business-document type code.
///
/// Matching is exact and fail-closed: whitespace, aliases and case folding are
/// rejected so bulk-job target validation keeps the original DocumentType contract.
pub fn is_business_document_type(object_type: &str) -> bool {
    BUSINESS_DOCUMENT_TYPE_CODES.binary_search(&object_type).is_ok()
}

/// Port support uses to read registered business-document ids without workflow types.
#[async_trait]
pub trait BusinessDocumentPort: Send + Sync {
    /// Ensure a single business document id is registered.
    async fn ensure_registered(&self, document_id: &str, executor: &mut dyn Executor) -> Result<()>;

    /// Load registered document ids from the given set.
    async fn find_registered_ids(&self, ids: &[String], executor: &mut dyn Executor) -> Result<Vec<String>>;
}

/// Fail-closed document port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedBusinessDocumentPort;

#[async_trait]
impl BusinessDocumentPort for FailClosedBusinessDocumentPort {
    async fn ensure_registered(&self, _document_id: &str, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("业务单据端口未接线".to_string()))
    }

    async fn find_registered_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(Error::Internal("业务单据端口未接线".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::{BUSINESS_DOCUMENT_TYPE_CODES, is_business_document_type};

    #[test]
    fn business_document_type_matching_is_exact_and_fail_closed() {
        assert!(is_business_document_type("sales_order"));
        assert!(is_business_document_type("payment_reversal"));
        assert!(!is_business_document_type(" Sales_order "));
        assert!(!is_business_document_type("SALES_ORDER"));
        assert!(!is_business_document_type("unknown"));
        assert_eq!(BUSINESS_DOCUMENT_TYPE_CODES.len(), 20);
        let mut sorted = BUSINESS_DOCUMENT_TYPE_CODES.to_vec();
        sorted.sort_unstable();
        assert_eq!(sorted, BUSINESS_DOCUMENT_TYPE_CODES, "类型码须保持有序以支持二分查找");
    }
}
