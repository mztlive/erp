//! Consumer port for business-document registration facts used by support.

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Serialization codes of workflow `DocumentType` consumed by support.
///
/// Support does not depend on `erp-workflow`. Composition adapters map these
/// codes to the workflow catalog; this list is the consumer snapshot of the
/// twenty frozen snake_case variants.
pub const BUSINESS_DOCUMENT_TYPE_CODES: &[&str] = &[
    "sales_order",
    "voucher_sales_order",
    "sales_change_order",
    "purchase_order",
    "purchase_change_order",
    "stock_adjustment",
    "customer_receipt",
    "supplier_payment",
    "customer_refund",
    "supplier_refund",
    "receipt_reversal",
    "payment_reversal",
    "purchase_receipt",
    "delivery",
    "electronic_delivery",
    "service_fulfillment",
    "customer_acceptance",
    "invoice",
    "sales_return_case",
    "purchase_return_order",
];

/// Return whether `object_type` is a frozen business-document type code.
///
/// Matching is exact and fail-closed: whitespace, aliases and case folding are
/// rejected so bulk-job target validation keeps the original DocumentType contract.
pub fn is_business_document_type(object_type: &str) -> bool {
    BUSINESS_DOCUMENT_TYPE_CODES.contains(&object_type)
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
    }
}
