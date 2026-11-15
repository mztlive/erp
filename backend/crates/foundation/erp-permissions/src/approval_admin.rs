//! 各审批主体类型的固定管理资格；不向账号授予权限。
use crate::{Permission, resources};

/// `sales_order` 的审批定义管理资格。
pub const SALES_ORDER_DEFINITION: Permission =
    Permission::new(resources::SALES_ORDER, "approval_definition_admin");

/// `sales_order` 的审批运行管理资格。
pub const SALES_ORDER_RUNTIME: Permission = Permission::new(resources::SALES_ORDER, "approval_runtime_admin");

/// `voucher_sales_order` 的审批定义管理资格。
pub const VOUCHER_SALES_ORDER_DEFINITION: Permission =
    Permission::new(resources::VOUCHER_SALES_ORDER, "approval_definition_admin");

/// `voucher_sales_order` 的审批运行管理资格。
pub const VOUCHER_SALES_ORDER_RUNTIME: Permission =
    Permission::new(resources::VOUCHER_SALES_ORDER, "approval_runtime_admin");

/// `sales_change_order` 的审批定义管理资格。
pub const SALES_CHANGE_ORDER_DEFINITION: Permission =
    Permission::new(resources::SALES_CHANGE_ORDER, "approval_definition_admin");

/// `sales_change_order` 的审批运行管理资格。
pub const SALES_CHANGE_ORDER_RUNTIME: Permission =
    Permission::new(resources::SALES_CHANGE_ORDER, "approval_runtime_admin");

/// `purchase_order` 的审批定义管理资格。
pub const PURCHASE_ORDER_DEFINITION: Permission =
    Permission::new(resources::PURCHASE_ORDER, "approval_definition_admin");

/// `purchase_order` 的审批运行管理资格。
pub const PURCHASE_ORDER_RUNTIME: Permission =
    Permission::new(resources::PURCHASE_ORDER, "approval_runtime_admin");

/// `purchase_change_order` 的审批定义管理资格。
pub const PURCHASE_CHANGE_ORDER_DEFINITION: Permission =
    Permission::new(resources::PURCHASE_CHANGE_ORDER, "approval_definition_admin");

/// `purchase_change_order` 的审批运行管理资格。
pub const PURCHASE_CHANGE_ORDER_RUNTIME: Permission =
    Permission::new(resources::PURCHASE_CHANGE_ORDER, "approval_runtime_admin");

/// `stock_adjustment` 的审批定义管理资格。
pub const STOCK_ADJUSTMENT_DEFINITION: Permission =
    Permission::new(resources::STOCK_ADJUSTMENT, "approval_definition_admin");

/// `stock_adjustment` 的审批运行管理资格。
pub const STOCK_ADJUSTMENT_RUNTIME: Permission =
    Permission::new(resources::STOCK_ADJUSTMENT, "approval_runtime_admin");

/// `sales_invoice_request` 的审批定义管理资格。
pub const SALES_INVOICE_REQUEST_DEFINITION: Permission =
    Permission::new(resources::SALES_INVOICE_REQUEST, "approval_definition_admin");

/// `sales_invoice_request` 的审批运行管理资格。
pub const SALES_INVOICE_REQUEST_RUNTIME: Permission =
    Permission::new(resources::SALES_INVOICE_REQUEST, "approval_runtime_admin");

/// `purchase_payment_request` 的审批定义管理资格。
pub const PURCHASE_PAYMENT_REQUEST_DEFINITION: Permission =
    Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "approval_definition_admin");

/// `purchase_payment_request` 的审批运行管理资格。
pub const PURCHASE_PAYMENT_REQUEST_RUNTIME: Permission =
    Permission::new(resources::PURCHASE_PAYMENT_REQUEST, "approval_runtime_admin");

/// `customer_receipt` 的审批定义管理资格。
pub const CUSTOMER_RECEIPT_DEFINITION: Permission =
    Permission::new(resources::CUSTOMER_RECEIPT, "approval_definition_admin");

/// `customer_receipt` 的审批运行管理资格。
pub const CUSTOMER_RECEIPT_RUNTIME: Permission =
    Permission::new(resources::CUSTOMER_RECEIPT, "approval_runtime_admin");

/// `customer_refund` 的审批定义管理资格。
pub const CUSTOMER_REFUND_DEFINITION: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "approval_definition_admin");

/// `customer_refund` 的审批运行管理资格。
pub const CUSTOMER_REFUND_RUNTIME: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "approval_runtime_admin");

/// `supplier_refund` 的审批定义管理资格。
pub const SUPPLIER_REFUND_DEFINITION: Permission =
    Permission::new(resources::SUPPLIER_REFUND, "approval_definition_admin");

/// `supplier_refund` 的审批运行管理资格。
pub const SUPPLIER_REFUND_RUNTIME: Permission =
    Permission::new(resources::SUPPLIER_REFUND, "approval_runtime_admin");

/// `receipt_reversal` 的审批定义管理资格。
pub const RECEIPT_REVERSAL_DEFINITION: Permission =
    Permission::new(resources::RECEIPT_REVERSAL, "approval_definition_admin");

/// `receipt_reversal` 的审批运行管理资格。
pub const RECEIPT_REVERSAL_RUNTIME: Permission =
    Permission::new(resources::RECEIPT_REVERSAL, "approval_runtime_admin");

/// `payment_reversal` 的审批定义管理资格。
pub const PAYMENT_REVERSAL_DEFINITION: Permission =
    Permission::new(resources::PAYMENT_REVERSAL, "approval_definition_admin");

/// `payment_reversal` 的审批运行管理资格。
pub const PAYMENT_REVERSAL_RUNTIME: Permission =
    Permission::new(resources::PAYMENT_REVERSAL, "approval_runtime_admin");

/// 支持审批的主体类型及定义、运行管理资格。
pub const TYPES: &[(&str, Permission, Permission)] = &[
    (resources::SALES_ORDER, SALES_ORDER_DEFINITION, SALES_ORDER_RUNTIME),
    (resources::VOUCHER_SALES_ORDER, VOUCHER_SALES_ORDER_DEFINITION, VOUCHER_SALES_ORDER_RUNTIME),
    (resources::SALES_CHANGE_ORDER, SALES_CHANGE_ORDER_DEFINITION, SALES_CHANGE_ORDER_RUNTIME),
    (resources::PURCHASE_ORDER, PURCHASE_ORDER_DEFINITION, PURCHASE_ORDER_RUNTIME),
    (resources::PURCHASE_CHANGE_ORDER, PURCHASE_CHANGE_ORDER_DEFINITION, PURCHASE_CHANGE_ORDER_RUNTIME),
    (resources::STOCK_ADJUSTMENT, STOCK_ADJUSTMENT_DEFINITION, STOCK_ADJUSTMENT_RUNTIME),
    (resources::SALES_INVOICE_REQUEST, SALES_INVOICE_REQUEST_DEFINITION, SALES_INVOICE_REQUEST_RUNTIME),
    (
        resources::PURCHASE_PAYMENT_REQUEST,
        PURCHASE_PAYMENT_REQUEST_DEFINITION,
        PURCHASE_PAYMENT_REQUEST_RUNTIME,
    ),
    (resources::CUSTOMER_RECEIPT, CUSTOMER_RECEIPT_DEFINITION, CUSTOMER_RECEIPT_RUNTIME),
    (resources::CUSTOMER_REFUND, CUSTOMER_REFUND_DEFINITION, CUSTOMER_REFUND_RUNTIME),
    (resources::SUPPLIER_REFUND, SUPPLIER_REFUND_DEFINITION, SUPPLIER_REFUND_RUNTIME),
    (resources::RECEIPT_REVERSAL, RECEIPT_REVERSAL_DEFINITION, RECEIPT_REVERSAL_RUNTIME),
    (resources::PAYMENT_REVERSAL, PAYMENT_REVERSAL_DEFINITION, PAYMENT_REVERSAL_RUNTIME),
];
