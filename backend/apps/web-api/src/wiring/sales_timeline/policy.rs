use erp_identity::Permission;
use erp_read_models::Result;
use erp_workflow::DocumentType;

/// 组合已有详情入口的读取策略；退货单不属于审批来源适配器。
#[derive(Debug, PartialEq, Eq)]
pub(super) enum ReadPolicy {
    Invoice,
    PaymentRequest,
    SalesReturn,
    PurchaseReturn,
    Fulfillment,
    ApprovalSource,
}

impl ReadPolicy {
    pub(super) fn for_document(kind: DocumentType) -> Self {
        use DocumentType::*;
        match kind {
            Invoice => Self::Invoice,
            PurchasePaymentRequest => Self::PaymentRequest,
            SalesReturnCase => Self::SalesReturn,
            PurchaseReturnOrder => Self::PurchaseReturn,
            Delivery | ElectronicDelivery | ServiceFulfillment | PurchaseReceipt | CustomerAcceptance => {
                Self::Fulfillment
            },
            _ => Self::ApprovalSource,
        }
    }

    pub(super) fn permission(&self, kind: DocumentType) -> Result<Permission> {
        let resource = if kind == DocumentType::VoucherSalesOrder { "sales_order" } else { kind.as_str() };
        // 这两个详情入口沿用 list 权限，权限目录中没有对应的 detail 动作。
        let action = match kind {
            DocumentType::ElectronicDelivery | DocumentType::ServiceFulfillment => "list",
            _ => "detail",
        };
        Ok(Permission::parse(format!("{resource}:{action}"))?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn return_documents_use_their_own_detail_policy() {
        for (kind, expected, grant) in [
            (DocumentType::SalesReturnCase, ReadPolicy::SalesReturn, "sales_return_case:detail"),
            (DocumentType::PurchaseReturnOrder, ReadPolicy::PurchaseReturn, "purchase_return_order:detail"),
        ] {
            let policy = ReadPolicy::for_document(kind);
            assert_eq!(policy, expected);
            let permission = policy.permission(kind).unwrap();
            assert!(Permission::parse(grant).unwrap().covers(&permission));
            assert!(!Permission::parse("sales_order:detail").unwrap().covers(&permission));
        }
    }

    #[test]
    fn fulfillment_accepts_the_exact_registered_read_grant() {
        for (kind, grant) in [
            (DocumentType::ElectronicDelivery, "electronic_delivery:list"),
            (DocumentType::ServiceFulfillment, "service_fulfillment:list"),
            (DocumentType::Delivery, "delivery:detail"),
            (DocumentType::PurchaseReceipt, "purchase_receipt:detail"),
            (DocumentType::CustomerAcceptance, "customer_acceptance:detail"),
        ] {
            let policy = ReadPolicy::for_document(kind);
            assert_eq!(policy, ReadPolicy::Fulfillment);
            let permission = policy.permission(kind).unwrap();
            assert!(Permission::parse(grant).unwrap().covers(&permission));
            assert!(!Permission::parse("sales_order:detail").unwrap().covers(&permission));
        }
    }
}
