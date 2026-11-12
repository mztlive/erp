//! payable 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 上传发票文件并登记过账分配。
pub const INVOICE_FILES_PURCHASE_INVOICE_ALLOCATION_POST_WITH_FILES: Permission =
    PURCHASE_INVOICE_ALLOCATION_POST;

/// 查询应付往来子账列表。
pub const ACCOUNT_LIST: Permission = Permission::new(resources::PAYABLE_ACCOUNT, "list");

/// 查询应付往来子账详情。
pub const ACCOUNT_DETAIL: Permission = Permission::new(resources::PAYABLE_ACCOUNT, "detail");

/// 揭示付款任务收款账号。
pub const PAYMENT_RECIPIENT_REVEAL: Permission = Permission::new(resources::PARTY_BANK_ACCOUNT, "reveal");

/// 建立应付往来子账。
pub const ACCOUNT_CREATE: Permission = Permission::new(resources::PAYABLE_ACCOUNT, "create");

/// 查询供应商付款单列表。
pub const SUPPLIER_PAYMENT_LIST: Permission = Permission::new(resources::SUPPLIER_PAYMENT, "list");

/// 查询供应商付款单详情。
pub const SUPPLIER_PAYMENT_DETAIL: Permission = Permission::new(resources::SUPPLIER_PAYMENT, "detail");

/// 查询可合并的同供应商付款任务。
pub const SUPPLIER_PAYMENT_MERGE_CANDIDATES: Permission = ACCOUNT_DETAIL;

/// 登记供应商付款并过账核销。
pub const SUPPLIER_PAYMENT_COMMIT: Permission = Permission::new(resources::SUPPLIER_PAYMENT, "commit");

/// 预览供应商付款银行回单。
pub const SUPPLIER_PAYMENT_BANK_RECEIPT: Permission = SUPPLIER_PAYMENT_DETAIL;

/// 进项发票登记过账并分配。
pub const PURCHASE_INVOICE_ALLOCATION_POST: Permission =
    Permission::new(resources::PURCHASE_INVOICE_ALLOCATION, "post");

/// 查询进项发票分配列表。
pub const PURCHASE_INVOICE_ALLOCATION_LIST: Permission =
    Permission::new(resources::PURCHASE_INVOICE_ALLOCATION, "list");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "ACCOUNT_LIST",
        PermissionDef {
            permission: &ACCOUNT_LIST,
            group: groups::PAYABLE,
            description: "查询应付往来子账列表",
        },
    ),
    (
        "ACCOUNT_DETAIL",
        PermissionDef {
            permission: &ACCOUNT_DETAIL,
            group: groups::PAYABLE,
            description: "查询应付往来子账详情",
        },
    ),
    (
        "PAYMENT_RECIPIENT_REVEAL",
        PermissionDef {
            permission: &PAYMENT_RECIPIENT_REVEAL,
            group: groups::PAYABLE,
            description: "揭示付款任务收款账号",
        },
    ),
    (
        "ACCOUNT_CREATE",
        PermissionDef {
            permission: &ACCOUNT_CREATE,
            group: groups::PAYABLE,
            description: "建立应付往来子账",
        },
    ),
    (
        "SUPPLIER_PAYMENT_LIST",
        PermissionDef {
            permission: &SUPPLIER_PAYMENT_LIST,
            group: groups::PAYABLE,
            description: "查询供应商付款单列表",
        },
    ),
    (
        "SUPPLIER_PAYMENT_DETAIL",
        PermissionDef {
            permission: &SUPPLIER_PAYMENT_DETAIL,
            group: groups::PAYABLE,
            description: "查询供应商付款单详情",
        },
    ),
    (
        "SUPPLIER_PAYMENT_COMMIT",
        PermissionDef {
            permission: &SUPPLIER_PAYMENT_COMMIT,
            group: groups::PAYABLE,
            description: "登记供应商付款并过账核销",
        },
    ),
    (
        "PURCHASE_INVOICE_ALLOCATION_POST",
        PermissionDef {
            permission: &PURCHASE_INVOICE_ALLOCATION_POST,
            group: groups::PAYABLE,
            description: "进项发票登记过账并分配",
        },
    ),
    (
        "PURCHASE_INVOICE_ALLOCATION_LIST",
        PermissionDef {
            permission: &PURCHASE_INVOICE_ALLOCATION_LIST,
            group: groups::PAYABLE,
            description: "查询进项发票分配列表",
        },
    ),
];
