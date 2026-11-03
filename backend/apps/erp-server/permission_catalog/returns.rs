//! returns 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询客户退款列表。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_LIST: PermissionDef =
    PermissionDef {
        code: "customer_refund:list", group: groups::RETURNS, description: "查询客户退款列表"
    };

/// 查询客户退款详情。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL: PermissionDef =
    PermissionDef {
        code: "customer_refund:detail", group: groups::RETURNS, description: "查询客户退款详情"
    };

/// 登记客户退款草稿。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_CREATE: PermissionDef =
    PermissionDef {
        code: "customer_refund:create", group: groups::RETURNS, description: "登记客户退款草稿"
    };

/// 一次创建并提交客户退款审批。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_COMMIT: PermissionDef =
    CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT.described(groups::RETURNS, "一次创建并提交客户退款审批");

/// 提交客户退款审批。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT: PermissionDef =
    PermissionDef {
        code: "customer_refund:submit", group: groups::RETURNS, description: "提交客户退款审批"
    };

/// 撤回客户退款审批。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "customer_refund:cancel_approval",
    group: groups::RETURNS,
    description: "撤回客户退款审批",
};

/// 客户退款过账。
pub const CUSTOMER_REFUND_CUSTOMER_REFUND_POST: PermissionDef =
    PermissionDef { code: "customer_refund:post", group: groups::RETURNS, description: "客户退款过账" };

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_CUSTOMER_REFUND_UPDATE: PermissionDef =
    CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT.described(groups::RETURNS, "修改原财务单据草稿");

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_SUPPLIER_REFUND_UPDATE: PermissionDef =
    SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT.described(groups::RETURNS, "修改原财务单据草稿");

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_RECEIPT_REVERSAL_UPDATE: PermissionDef =
    RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT.described(groups::RETURNS, "修改原财务单据草稿");

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_PAYMENT_REVERSAL_UPDATE: PermissionDef =
    PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT.described(groups::RETURNS, "修改原财务单据草稿");

/// 查询付款冲正详情。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL: PermissionDef = PermissionDef {
    code: "payment_reversal:detail",
    group: groups::RETURNS,
    description: "查询付款冲正详情",
};

/// 登记付款冲正草稿。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE: PermissionDef = PermissionDef {
    code: "payment_reversal:create",
    group: groups::RETURNS,
    description: "登记付款冲正草稿",
};

/// 一次创建并提交付款冲正审批。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_COMMIT: PermissionDef =
    PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT.described(groups::RETURNS, "一次创建并提交付款冲正审批");

/// 提交付款冲正审批。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT: PermissionDef = PermissionDef {
    code: "payment_reversal:submit",
    group: groups::RETURNS,
    description: "提交付款冲正审批",
};

/// 撤回付款冲正审批。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "payment_reversal:cancel_approval",
    group: groups::RETURNS,
    description: "撤回付款冲正审批",
};

/// 付款冲正过账。
pub const PAYMENT_REVERSAL_PAYMENT_REVERSAL_POST: PermissionDef =
    PermissionDef {
        code: "payment_reversal:post", group: groups::RETURNS, description: "付款冲正过账"
    };

/// 查询采购退货单列表。
pub const PURCHASE_RETURN_PURCHASE_RETURN_ORDER_LIST: PermissionDef = PermissionDef {
    code: "purchase_return_order:list",
    group: groups::RETURNS,
    description: "查询采购退货单列表",
};

/// 查询采购退货单详情。
pub const PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL: PermissionDef = PermissionDef {
    code: "purchase_return_order:detail",
    group: groups::RETURNS,
    description: "查询采购退货单详情",
};

/// 建立采购退货单。
pub const PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE: PermissionDef = PermissionDef {
    code: "purchase_return_order:create",
    group: groups::RETURNS,
    description: "建立采购退货单",
};

/// 建立采购退货单。
pub const PURCHASE_RETURN_PURCHASE_DIRECT_RETURN_CONFIRM: PermissionDef =
    PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE;

/// 建立采购退货单。
pub const PURCHASE_RETURN_PURCHASE_RETURN_FINANCE_SYNC: PermissionDef =
    PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE;

/// 查询回款冲正详情。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL: PermissionDef = PermissionDef {
    code: "receipt_reversal:detail",
    group: groups::RETURNS,
    description: "查询回款冲正详情",
};

/// 登记回款冲正草稿。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE: PermissionDef = PermissionDef {
    code: "receipt_reversal:create",
    group: groups::RETURNS,
    description: "登记回款冲正草稿",
};

/// 一次创建并提交回款冲正审批。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_COMMIT: PermissionDef =
    RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT.described(groups::RETURNS, "一次创建并提交回款冲正审批");

/// 提交回款冲正审批。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT: PermissionDef = PermissionDef {
    code: "receipt_reversal:submit",
    group: groups::RETURNS,
    description: "提交回款冲正审批",
};

/// 撤回回款冲正审批。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "receipt_reversal:cancel_approval",
    group: groups::RETURNS,
    description: "撤回回款冲正审批",
};

/// 回款冲正过账。
pub const RECEIPT_REVERSAL_RECEIPT_REVERSAL_POST: PermissionDef =
    PermissionDef {
        code: "receipt_reversal:post", group: groups::RETURNS, description: "回款冲正过账"
    };

/// 查询销售退货处理单列表。
pub const SALES_RETURN_SALES_RETURN_CASE_LIST: PermissionDef = PermissionDef {
    code: "sales_return_case:list",
    group: groups::RETURNS,
    description: "查询销售退货处理单列表",
};

/// 查询销售退货处理单详情。
pub const SALES_RETURN_SALES_RETURN_CASE_DETAIL: PermissionDef = PermissionDef {
    code: "sales_return_case:detail",
    group: groups::RETURNS,
    description: "查询销售退货处理单详情",
};

/// 建立销售退货处理单。
pub const SALES_RETURN_SALES_RETURN_CASE_CREATE: PermissionDef = PermissionDef {
    code: "sales_return_case:create",
    group: groups::RETURNS,
    description: "建立销售退货处理单",
};

/// 查询供应商退款详情。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL: PermissionDef = PermissionDef {
    code: "supplier_refund:detail",
    group: groups::RETURNS,
    description: "查询供应商退款详情",
};

/// 登记供应商退款草稿。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE: PermissionDef = PermissionDef {
    code: "supplier_refund:create",
    group: groups::RETURNS,
    description: "登记供应商退款草稿",
};

/// 一次创建并提交供应商退款审批。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_COMMIT: PermissionDef =
    SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT.described(groups::RETURNS, "一次创建并提交供应商退款审批");

/// 提交供应商退款审批。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT: PermissionDef = PermissionDef {
    code: "supplier_refund:submit",
    group: groups::RETURNS,
    description: "提交供应商退款审批",
};

/// 撤回供应商退款审批。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "supplier_refund:cancel_approval",
    group: groups::RETURNS,
    description: "撤回供应商退款审批",
};

/// 供应商退款过账。
pub const SUPPLIER_REFUND_SUPPLIER_REFUND_POST: PermissionDef =
    PermissionDef {
        code: "supplier_refund:post", group: groups::RETURNS, description: "供应商退款过账"
    };

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("CUSTOMER_REFUND_CUSTOMER_REFUND_LIST", CUSTOMER_REFUND_CUSTOMER_REFUND_LIST),
    ("CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL", CUSTOMER_REFUND_CUSTOMER_REFUND_DETAIL),
    ("CUSTOMER_REFUND_CUSTOMER_REFUND_CREATE", CUSTOMER_REFUND_CUSTOMER_REFUND_CREATE),
    ("CUSTOMER_REFUND_CUSTOMER_REFUND_COMMIT", CUSTOMER_REFUND_CUSTOMER_REFUND_COMMIT),
    ("CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT", CUSTOMER_REFUND_CUSTOMER_REFUND_SUBMIT),
    ("CUSTOMER_REFUND_CUSTOMER_REFUND_CANCEL_APPROVAL", CUSTOMER_REFUND_CUSTOMER_REFUND_CANCEL_APPROVAL),
    ("CUSTOMER_REFUND_CUSTOMER_REFUND_POST", CUSTOMER_REFUND_CUSTOMER_REFUND_POST),
    ("DRAFT_UPDATE_CUSTOMER_REFUND_UPDATE", DRAFT_UPDATE_CUSTOMER_REFUND_UPDATE),
    ("DRAFT_UPDATE_SUPPLIER_REFUND_UPDATE", DRAFT_UPDATE_SUPPLIER_REFUND_UPDATE),
    ("DRAFT_UPDATE_RECEIPT_REVERSAL_UPDATE", DRAFT_UPDATE_RECEIPT_REVERSAL_UPDATE),
    ("DRAFT_UPDATE_PAYMENT_REVERSAL_UPDATE", DRAFT_UPDATE_PAYMENT_REVERSAL_UPDATE),
    ("PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL", PAYMENT_REVERSAL_PAYMENT_REVERSAL_DETAIL),
    ("PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE", PAYMENT_REVERSAL_PAYMENT_REVERSAL_CREATE),
    ("PAYMENT_REVERSAL_PAYMENT_REVERSAL_COMMIT", PAYMENT_REVERSAL_PAYMENT_REVERSAL_COMMIT),
    ("PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT", PAYMENT_REVERSAL_PAYMENT_REVERSAL_SUBMIT),
    ("PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL", PAYMENT_REVERSAL_PAYMENT_REVERSAL_CANCEL_APPROVAL),
    ("PAYMENT_REVERSAL_PAYMENT_REVERSAL_POST", PAYMENT_REVERSAL_PAYMENT_REVERSAL_POST),
    ("PURCHASE_RETURN_PURCHASE_RETURN_ORDER_LIST", PURCHASE_RETURN_PURCHASE_RETURN_ORDER_LIST),
    ("PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL", PURCHASE_RETURN_PURCHASE_RETURN_ORDER_DETAIL),
    ("PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE", PURCHASE_RETURN_PURCHASE_RETURN_ORDER_CREATE),
    ("PURCHASE_RETURN_PURCHASE_DIRECT_RETURN_CONFIRM", PURCHASE_RETURN_PURCHASE_DIRECT_RETURN_CONFIRM),
    ("PURCHASE_RETURN_PURCHASE_RETURN_FINANCE_SYNC", PURCHASE_RETURN_PURCHASE_RETURN_FINANCE_SYNC),
    ("RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL", RECEIPT_REVERSAL_RECEIPT_REVERSAL_DETAIL),
    ("RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE", RECEIPT_REVERSAL_RECEIPT_REVERSAL_CREATE),
    ("RECEIPT_REVERSAL_RECEIPT_REVERSAL_COMMIT", RECEIPT_REVERSAL_RECEIPT_REVERSAL_COMMIT),
    ("RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT", RECEIPT_REVERSAL_RECEIPT_REVERSAL_SUBMIT),
    ("RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL", RECEIPT_REVERSAL_RECEIPT_REVERSAL_CANCEL_APPROVAL),
    ("RECEIPT_REVERSAL_RECEIPT_REVERSAL_POST", RECEIPT_REVERSAL_RECEIPT_REVERSAL_POST),
    ("SALES_RETURN_SALES_RETURN_CASE_LIST", SALES_RETURN_SALES_RETURN_CASE_LIST),
    ("SALES_RETURN_SALES_RETURN_CASE_DETAIL", SALES_RETURN_SALES_RETURN_CASE_DETAIL),
    ("SALES_RETURN_SALES_RETURN_CASE_CREATE", SALES_RETURN_SALES_RETURN_CASE_CREATE),
    ("SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL", SUPPLIER_REFUND_SUPPLIER_REFUND_DETAIL),
    ("SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE", SUPPLIER_REFUND_SUPPLIER_REFUND_CREATE),
    ("SUPPLIER_REFUND_SUPPLIER_REFUND_COMMIT", SUPPLIER_REFUND_SUPPLIER_REFUND_COMMIT),
    ("SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT", SUPPLIER_REFUND_SUPPLIER_REFUND_SUBMIT),
    ("SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL", SUPPLIER_REFUND_SUPPLIER_REFUND_CANCEL_APPROVAL),
    ("SUPPLIER_REFUND_SUPPLIER_REFUND_POST", SUPPLIER_REFUND_SUPPLIER_REFUND_POST),
];
