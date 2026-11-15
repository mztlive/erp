//! 文件任务的固定功能资格；对象和查询授权由所属业务用例继续执行。
use erp_permissions::{
    Permission, catalog, contract, cost, customer, internal, inventory, payable, purchase_order, receivable,
    sales_inquiry, sales_order, supplier, supplier_fulfillment, supplier_offering,
};

use crate::QualifiedPermissions;

const REQUIREMENTS: &[(&Permission, &[Permission])] = &[
    (&internal::DATA_TRANSFER_PRODUCT_POOL, &[catalog::PRODUCT_SELLABLE_SKU_LIST]),
    (
        &internal::DATA_TRANSFER_SUPPLIER_QUOTES,
        &[
            sales_inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES,
            catalog::PRODUCT_SELLABLE_SKU_LIST,
            supplier_offering::LIST,
        ],
    ),
    (&internal::DATA_TRANSFER_SALES_ORDERS, &[sales_order::LIST]),
    (&internal::DATA_TRANSFER_PURCHASE_ORDERS, &[purchase_order::LIST]),
    (&internal::DATA_TRANSFER_CONTRACTS, &[contract::LIST]),
    (&internal::DATA_TRANSFER_CUSTOMERS, &[customer::LIST]),
    (&internal::DATA_TRANSFER_ALL_CUSTOMERS, &[customer::LIST, customer::ALL_AUTHORIZED_LIST]),
    (&internal::DATA_TRANSFER_STOCK_BALANCE, &[inventory::STOCK_BALANCE_LIST]),
    (&internal::DATA_TRANSFER_STOCK_MOVEMENT, &[inventory::STOCK_MOVEMENT_LIST]),
    (&internal::DATA_TRANSFER_STOCK_ADJUSTMENT, &[inventory::STOCK_ADJUSTMENT_LIST]),
    (&internal::DATA_TRANSFER_STOCK_RESERVATION, &[inventory::STOCK_RESERVATION_LIST]),
    (&internal::DATA_TRANSFER_SUPPLIER_ORDERS, &[supplier_fulfillment::ORDER_LIST]),
    (&internal::DATA_TRANSFER_RECEIVABLES, &[receivable::ACCOUNT_LIST]),
    (&internal::DATA_TRANSFER_RECEIPTS, &[receivable::CUSTOMER_RECEIPT_LIST]),
    (&internal::DATA_TRANSFER_INVOICES, &[receivable::INVOICE_LIST]),
    (&internal::DATA_TRANSFER_PAYABLES, &[payable::ACCOUNT_LIST]),
    (&internal::DATA_TRANSFER_PAYMENTS, &[payable::SUPPLIER_PAYMENT_LIST]),
    (&internal::DATA_TRANSFER_INVOICE_ALLOCATIONS, &[payable::PURCHASE_INVOICE_ALLOCATION_LIST]),
    (&internal::DATA_TRANSFER_INVOICE_REQUESTS, &[receivable::INVOICE_REQUEST_INVOICE_REQUEST_LIST]),
    (&internal::DATA_TRANSFER_PROFIT_LOSS, &[cost::ENTRY_LIST, sales_order::LIST]),
    (&internal::DATA_TRANSFER_CUSTOMER_QUALITY, &[customer::LIST, sales_order::LIST]),
    (&internal::DATA_TRANSFER_PRODUCTS, &[catalog::PRODUCT_PRODUCT_LIST]),
    (&internal::DATA_TRANSFER_SUPPLIERS, &[supplier::LIST]),
    (&internal::DATA_TRANSFER_INQUIRY_PREVIEW, &[sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW]),
    (
        &internal::DATA_TRANSFER_INQUIRY_CREATION,
        &[
            sales_inquiry::COMMANDS_SALES_CREATE,
            sales_inquiry::IMPORT_SALES_IMPORT_PREVIEW,
            catalog::PRODUCT_SELLABLE_SKU_LIST,
        ],
    ),
    (&internal::DATA_TRANSFER_OFFERING_REVISE, &[supplier_offering::REVISE]),
    (&internal::DATA_TRANSFER_OFFERING_AVAILABILITY, &[supplier_offering::UPDATE_AVAILABILITY]),
];

/// 返回任务的原有独立功能资格；任务输入不能追加或删减资格。
pub(super) fn requirements(permission: &Permission) -> Option<&'static [Permission]> {
    REQUIREMENTS.iter().find(|(entry, _)| *entry == permission).map(|(_, required)| *required)
}

/// 仅识别固定功能预检入口，不将业务对象权限转换为无对象操作。
pub(super) fn supports(permission: &Permission) -> bool {
    requirements(permission).is_some()
}

/// 各资格沿原规则独立取证，可以来自不同启用角色。
pub(super) fn qualified(permission: &Permission, qualified: &QualifiedPermissions) -> bool {
    requirements(permission)
        .is_some_and(|required| required.iter().all(|entry| qualified.same_role(std::slice::from_ref(entry))))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_export_preserves_independent_roles_and_all_required_qualifications() {
        let required = [
            ("buyer", vec![sales_inquiry::QUERY_PROCUREMENT_INTERNAL_QUOTES]),
            ("catalog", vec![catalog::PRODUCT_SELLABLE_SKU_LIST]),
            ("supplier", vec![supplier_offering::LIST]),
        ];
        let grants =
            required.iter().map(|(role, permissions)| (*role, permissions.as_slice())).collect::<Vec<_>>();
        let complete = QualifiedPermissions::test_grants(&grants);
        assert!(qualified(&internal::DATA_TRANSFER_SUPPLIER_QUOTES, &complete));
        for missing in 0..required.len() {
            let partial = grants
                .iter()
                .enumerate()
                .filter_map(|(index, grant)| (index != missing).then_some(*grant))
                .collect::<Vec<_>>();
            assert!(!qualified(
                &internal::DATA_TRANSFER_SUPPLIER_QUOTES,
                &QualifiedPermissions::test_grants(&partial)
            ));
        }
    }

    #[test]
    fn customer_export_requires_the_extra_all_authorized_qualification_only_for_that_entry() {
        let listed = [customer::LIST];
        let qualified_list = QualifiedPermissions::test_grants(&[("reader", &listed)]);
        assert!(qualified(&internal::DATA_TRANSFER_CUSTOMERS, &qualified_list));
        assert!(!qualified(&internal::DATA_TRANSFER_ALL_CUSTOMERS, &qualified_list));
        assert!(!qualified(&customer::LIST, &qualified_list));
        assert!(!supports(&customer::LIST));
    }
}
