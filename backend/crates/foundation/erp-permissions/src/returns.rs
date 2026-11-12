//! returns 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询客户退款列表。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_LIST: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "list");

/// 查询客户退款详情。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "detail");

/// 登记客户退款草稿。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_CREATE: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "create");

/// 一次创建并提交客户退款审批。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_COMMIT: Permission = CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT;

/// 提交客户退款审批。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "submit");

/// 撤回客户退款审批。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_CANCEL_APPROVAL: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "cancel_approval");

/// 客户退款过账。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_POST: Permission =
    Permission::new(resources::CUSTOMER_REFUND, "post");

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_CUSTOMER_REFUND_UPDATE: Permission = CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT;

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_SUPPLIER_REFUND_UPDATE: Permission = SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT;

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_RECEIPT_REVERSAL_UPDATE: Permission = RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT;

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_PAYMENT_REVERSAL_UPDATE: Permission = PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT;

/// 查询付款冲正详情。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL: Permission =
    Permission::new(resources::PAYMENT_REVERSAL, "detail");

/// 登记付款冲正草稿。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE: Permission =
    Permission::new(resources::PAYMENT_REVERSAL, "create");

/// 一次创建并提交付款冲正审批。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_COMMIT: Permission = PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT;

/// 提交付款冲正审批。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT: Permission =
    Permission::new(resources::PAYMENT_REVERSAL, "submit");

/// 撤回付款冲正审批。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL: Permission =
    Permission::new(resources::PAYMENT_REVERSAL, "cancel_approval");

/// 付款冲正过账。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_POST: Permission =
    Permission::new(resources::PAYMENT_REVERSAL, "post");

/// 查询采购退货单列表。
pub const PURCHASE_RETURN_PURCHASE_RETURN_ORDER_LIST: Permission =
    Permission::new(resources::PURCHASE_RETURN_ORDER, "list");

/// 查询采购退货单详情。
pub const PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL: Permission =
    Permission::new(resources::PURCHASE_RETURN_ORDER, "detail");

/// 建立采购退货单。
pub const PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE: Permission =
    Permission::new(resources::PURCHASE_RETURN_ORDER, "create");

/// 建立采购退货单。
pub const PURCHASE_RETURN_PURCHASE_DIRECT_RETURN_CONFIRM: Permission =
    PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE;

/// 建立采购退货单。
pub const PURCHASE_RETURN_PURCHASE_RETURN_FINANCE_SYNC: Permission =
    PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE;

/// 查询回款冲正详情。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL: Permission =
    Permission::new(resources::RECEIPT_REVERSAL, "detail");

/// 登记回款冲正草稿。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE: Permission =
    Permission::new(resources::RECEIPT_REVERSAL, "create");

/// 一次创建并提交回款冲正审批。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_COMMIT: Permission = RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT;

/// 提交回款冲正审批。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT: Permission =
    Permission::new(resources::RECEIPT_REVERSAL, "submit");

/// 撤回回款冲正审批。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL: Permission =
    Permission::new(resources::RECEIPT_REVERSAL, "cancel_approval");

/// 回款冲正过账。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_POST: Permission =
    Permission::new(resources::RECEIPT_REVERSAL, "post");

/// 查询销售退货处理单列表。
pub const SALES_RETURN_SALES_RETURN_CASE_LIST: Permission =
    Permission::new(resources::SALES_RETURN_CASE, "list");

/// 查询销售退货处理单详情。
pub const SALES_RETURN_SALES_RETURN_CASE_DETAIL: Permission =
    Permission::new(resources::SALES_RETURN_CASE, "detail");

/// 建立销售退货处理单。
pub const SALES_RETURN_SALES_RETURN_CASE_CREATE: Permission =
    Permission::new(resources::SALES_RETURN_CASE, "create");

/// 查询供应商退款详情。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL: Permission =
    Permission::new(resources::SUPPLIER_REFUND, "detail");

/// 登记供应商退款草稿。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE: Permission =
    Permission::new(resources::SUPPLIER_REFUND, "create");

/// 一次创建并提交供应商退款审批。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_COMMIT: Permission = SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT;

/// 提交供应商退款审批。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT: Permission =
    Permission::new(resources::SUPPLIER_REFUND, "submit");

/// 撤回供应商退款审批。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL: Permission =
    Permission::new(resources::SUPPLIER_REFUND, "cancel_approval");

/// 供应商退款过账。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_POST: Permission =
    Permission::new(resources::SUPPLIER_REFUND, "post");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "CUSTOMER_REFUND_CUSTOMER_REFUND_LIST",
        PermissionDef {
            permission: &CUSTOMER_REFUND_CUSTOMER_REFUND_LIST,
            group: groups::RETURNS,
            description: "查询客户退款列表",
        },
    ),
    (
        "CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL",
        PermissionDef {
            permission: &CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL,
            group: groups::RETURNS,
            description: "查询客户退款详情",
        },
    ),
    (
        "CUSTOMER_REFUND_CUSTOMER_REFUND_CREATE",
        PermissionDef {
            permission: &CUSTOMER_REFUND_CUSTOMER_REFUND_CREATE,
            group: groups::RETURNS,
            description: "登记客户退款草稿",
        },
    ),
    (
        "CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT",
        PermissionDef {
            permission: &CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT,
            group: groups::RETURNS,
            description: "提交客户退款审批",
        },
    ),
    (
        "CUSTOMER_REFUND_CUSTOMER_REFUND_CANCEL_APPROVAL",
        PermissionDef {
            permission: &CUSTOMER_REFUND_CUSTOMER_REFUND_CANCEL_APPROVAL,
            group: groups::RETURNS,
            description: "撤回客户退款审批",
        },
    ),
    (
        "CUSTOMER_REFUND_CUSTOMER_REFUND_POST",
        PermissionDef {
            permission: &CUSTOMER_REFUND_CUSTOMER_REFUND_POST,
            group: groups::RETURNS,
            description: "客户退款过账",
        },
    ),
    (
        "PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL",
        PermissionDef {
            permission: &PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL,
            group: groups::RETURNS,
            description: "查询付款冲正详情",
        },
    ),
    (
        "PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE",
        PermissionDef {
            permission: &PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE,
            group: groups::RETURNS,
            description: "登记付款冲正草稿",
        },
    ),
    (
        "PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT",
        PermissionDef {
            permission: &PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT,
            group: groups::RETURNS,
            description: "提交付款冲正审批",
        },
    ),
    (
        "PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL",
        PermissionDef {
            permission: &PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL,
            group: groups::RETURNS,
            description: "撤回付款冲正审批",
        },
    ),
    (
        "PAYMENT_REVERSAL_PAYMENT_REVERSAL_POST",
        PermissionDef {
            permission: &PAYMENT_REVERSAL_PAYMENT_REVERSAL_POST,
            group: groups::RETURNS,
            description: "付款冲正过账",
        },
    ),
    (
        "PURCHASE_RETURN_PURCHASE_RETURN_ORDER_LIST",
        PermissionDef {
            permission: &PURCHASE_RETURN_PURCHASE_RETURN_ORDER_LIST,
            group: groups::RETURNS,
            description: "查询采购退货单列表",
        },
    ),
    (
        "PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL",
        PermissionDef {
            permission: &PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL,
            group: groups::RETURNS,
            description: "查询采购退货单详情",
        },
    ),
    (
        "PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE",
        PermissionDef {
            permission: &PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE,
            group: groups::RETURNS,
            description: "建立采购退货单",
        },
    ),
    (
        "RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL",
        PermissionDef {
            permission: &RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL,
            group: groups::RETURNS,
            description: "查询回款冲正详情",
        },
    ),
    (
        "RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE",
        PermissionDef {
            permission: &RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE,
            group: groups::RETURNS,
            description: "登记回款冲正草稿",
        },
    ),
    (
        "RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT",
        PermissionDef {
            permission: &RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT,
            group: groups::RETURNS,
            description: "提交回款冲正审批",
        },
    ),
    (
        "RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL",
        PermissionDef {
            permission: &RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL,
            group: groups::RETURNS,
            description: "撤回回款冲正审批",
        },
    ),
    (
        "RECEIPT_REVERSAL_RECEIPT_REVERSAL_POST",
        PermissionDef {
            permission: &RECEIPT_REVERSAL_RECEIPT_REVERSAL_POST,
            group: groups::RETURNS,
            description: "回款冲正过账",
        },
    ),
    (
        "SALES_RETURN_SALES_RETURN_CASE_LIST",
        PermissionDef {
            permission: &SALES_RETURN_SALES_RETURN_CASE_LIST,
            group: groups::RETURNS,
            description: "查询销售退货处理单列表",
        },
    ),
    (
        "SALES_RETURN_SALES_RETURN_CASE_DETAIL",
        PermissionDef {
            permission: &SALES_RETURN_SALES_RETURN_CASE_DETAIL,
            group: groups::RETURNS,
            description: "查询销售退货处理单详情",
        },
    ),
    (
        "SALES_RETURN_SALES_RETURN_CASE_CREATE",
        PermissionDef {
            permission: &SALES_RETURN_SALES_RETURN_CASE_CREATE,
            group: groups::RETURNS,
            description: "建立销售退货处理单",
        },
    ),
    (
        "SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL",
        PermissionDef {
            permission: &SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL,
            group: groups::RETURNS,
            description: "查询供应商退款详情",
        },
    ),
    (
        "SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE",
        PermissionDef {
            permission: &SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE,
            group: groups::RETURNS,
            description: "登记供应商退款草稿",
        },
    ),
    (
        "SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT",
        PermissionDef {
            permission: &SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT,
            group: groups::RETURNS,
            description: "提交供应商退款审批",
        },
    ),
    (
        "SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL",
        PermissionDef {
            permission: &SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL,
            group: groups::RETURNS,
            description: "撤回供应商退款审批",
        },
    ),
    (
        "SUPPLIER_REFUND_SUPPLIER_REFUND_POST",
        PermissionDef {
            permission: &SUPPLIER_REFUND_SUPPLIER_REFUND_POST,
            group: groups::RETURNS,
            description: "供应商退款过账",
        },
    ),
];
