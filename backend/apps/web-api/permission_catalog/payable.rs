//! payable 的接口权限目录。
use super::{PermissionDef, groups};

/// 上传发票文件并登记过账分配。
pub const INVOICE_FILES_PURCHASE_INVOICE_ALLOCATION_POST_WITH_FILES: PermissionDef =
    PURCHASE_INVOICE_ALLOCATION_POST.described(groups::PAYABLE, "上传发票文件并登记过账分配");

/// 查询应付往来子账列表。
pub const ACCOUNT_LIST: PermissionDef = PermissionDef {
    code: "payable_account:list",
    group: groups::PAYABLE,
    description: "查询应付往来子账列表",
};

/// 查询应付往来子账详情。
pub const ACCOUNT_DETAIL: PermissionDef = PermissionDef {
    code: "payable_account:detail",
    group: groups::PAYABLE,
    description: "查询应付往来子账详情",
};

/// 揭示付款任务收款账号。
pub const PAYMENT_RECIPIENT_REVEAL: PermissionDef = PermissionDef {
    code: "party_bank_account:reveal",
    group: groups::PAYABLE,
    description: "揭示付款任务收款账号",
};

/// 建立应付往来子账。
pub const ACCOUNT_CREATE: PermissionDef =
    PermissionDef {
        code: "payable_account:create", group: groups::PAYABLE, description: "建立应付往来子账"
    };

/// 查询供应商付款单列表。
pub const SUPPLIER_PAYMENT_LIST: PermissionDef = PermissionDef {
    code: "supplier_payment:list",
    group: groups::PAYABLE,
    description: "查询供应商付款单列表",
};

/// 查询供应商付款单详情。
pub const SUPPLIER_PAYMENT_DETAIL: PermissionDef = PermissionDef {
    code: "supplier_payment:detail",
    group: groups::PAYABLE,
    description: "查询供应商付款单详情",
};

/// 查询可合并的同供应商付款任务。
pub const SUPPLIER_PAYMENT_MERGE_CANDIDATES: PermissionDef =
    ACCOUNT_DETAIL.described(groups::PAYABLE, "查询可合并的同供应商付款任务");

/// 登记供应商付款并过账核销。
pub const SUPPLIER_PAYMENT_COMMIT: PermissionDef = PermissionDef {
    code: "supplier_payment:commit",
    group: groups::PAYABLE,
    description: "登记供应商付款并过账核销",
};

/// 预览供应商付款银行回单。
pub const SUPPLIER_PAYMENT_BANK_RECEIPT: PermissionDef =
    SUPPLIER_PAYMENT_DETAIL.described(groups::PAYABLE, "预览供应商付款银行回单");

/// 进项发票登记过账并分配。
pub const PURCHASE_INVOICE_ALLOCATION_POST: PermissionDef = PermissionDef {
    code: "purchase_invoice_allocation:post",
    group: groups::PAYABLE,
    description: "进项发票登记过账并分配",
};

/// 查询进项发票分配列表。
pub const PURCHASE_INVOICE_ALLOCATION_LIST: PermissionDef = PermissionDef {
    code: "purchase_invoice_allocation:list",
    group: groups::PAYABLE,
    description: "查询进项发票分配列表",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "INVOICE_FILES_PURCHASE_INVOICE_ALLOCATION_POST_WITH_FILES",
        INVOICE_FILES_PURCHASE_INVOICE_ALLOCATION_POST_WITH_FILES,
    ),
    ("ACCOUNT_LIST", ACCOUNT_LIST),
    ("ACCOUNT_DETAIL", ACCOUNT_DETAIL),
    ("PAYMENT_RECIPIENT_REVEAL", PAYMENT_RECIPIENT_REVEAL),
    ("ACCOUNT_CREATE", ACCOUNT_CREATE),
    ("SUPPLIER_PAYMENT_LIST", SUPPLIER_PAYMENT_LIST),
    ("SUPPLIER_PAYMENT_DETAIL", SUPPLIER_PAYMENT_DETAIL),
    ("SUPPLIER_PAYMENT_MERGE_CANDIDATES", SUPPLIER_PAYMENT_MERGE_CANDIDATES),
    ("SUPPLIER_PAYMENT_COMMIT", SUPPLIER_PAYMENT_COMMIT),
    ("SUPPLIER_PAYMENT_BANK_RECEIPT", SUPPLIER_PAYMENT_BANK_RECEIPT),
    ("PURCHASE_INVOICE_ALLOCATION_POST", PURCHASE_INVOICE_ALLOCATION_POST),
    ("PURCHASE_INVOICE_ALLOCATION_LIST", PURCHASE_INVOICE_ALLOCATION_LIST),
];
