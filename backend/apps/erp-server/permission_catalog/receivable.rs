//! receivable 的接口权限目录。
use super::{PermissionDef, groups};

/// 读取原回款编辑字段。
pub const DRAFT_READ_CUSTOMER_RECEIPT_DRAFT: PermissionDef =
    CUSTOMER_RECEIPT_SUBMIT.described(groups::INVOICE, "读取原回款编辑字段");

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_CUSTOMER_RECEIPT_UPDATE: PermissionDef =
    CUSTOMER_RECEIPT_SUBMIT.described(groups::INVOICE, "修改原财务单据草稿");

/// 保存回款登记草稿。
pub const DRAFT_UPDATE_CUSTOMER_RECEIPT_SAVE_DRAFT: PermissionDef =
    CUSTOMER_RECEIPT_SUBMIT.described(groups::INVOICE, "保存回款登记草稿");

/// 登记发票草稿。
pub const INVOICE_FILES_INVOICE_CREATE_WITH_FILES: PermissionDef = INVOICE_CREATE;

/// 原子登记销项发票并分配。
pub const INVOICE_FILES_INVOICE_COMMIT_WITH_FILES: PermissionDef =
    INVOICE_POST.described(groups::INVOICE, "原子登记销项发票并分配");

/// 上传附件并完成开票。
pub const INVOICE_FILES_INVOICE_COMPLETE_WITH_FILES: PermissionDef =
    INVOICE_POST.described(groups::INVOICE, "上传附件并完成开票");

/// 查询开票申请列表。
pub const INVOICE_REQUEST_INVOICE_REQUEST_LIST: PermissionDef = PermissionDef {
    code: "sales_invoice_request:list",
    group: groups::RECEIVABLE,
    description: "查询开票申请列表",
};

/// 查询开票申请详情。
pub const INVOICE_REQUEST_INVOICE_REQUEST_DETAIL: PermissionDef = PermissionDef {
    code: "sales_invoice_request:detail",
    group: groups::RECEIVABLE,
    description: "查询开票申请详情",
};

/// 查询可申请开票额度。
pub const INVOICE_REQUEST_INVOICE_REQUEST_AMOUNTS: PermissionDef =
    INVOICE_REQUEST_INVOICE_REQUEST_LIST.described(groups::RECEIVABLE, "查询可申请开票额度");

/// 提交开票申请。
pub const INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT: PermissionDef = PermissionDef {
    code: "sales_invoice_request:submit",
    group: groups::RECEIVABLE,
    description: "提交开票申请",
};

/// 撤回开票申请。
pub const INVOICE_REQUEST_INVOICE_REQUEST_CANCEL: PermissionDef = PermissionDef {
    code: "sales_invoice_request:cancel",
    group: groups::RECEIVABLE,
    description: "撤回开票申请",
};

/// 查询应收往来子账列表。
pub const ACCOUNT_LIST: PermissionDef = PermissionDef {
    code: "receivable_account:list",
    group: groups::INVOICE,
    description: "查询应收往来子账列表",
};

/// 查询应收往来子账详情。
pub const ACCOUNT_DETAIL: PermissionDef = PermissionDef {
    code: "receivable_account:detail",
    group: groups::INVOICE,
    description: "查询应收往来子账详情",
};

/// 建立应收往来子账。
pub const ACCOUNT_CREATE: PermissionDef = PermissionDef {
    code: "receivable_account:create",
    group: groups::INVOICE,
    description: "建立应收往来子账",
};

/// 查询客户回款单列表。
pub const CUSTOMER_RECEIPT_LIST: PermissionDef = PermissionDef {
    code: "customer_receipt:list",
    group: groups::INVOICE,
    description: "查询客户回款单列表",
};

/// 查询客户回款单详情。
pub const CUSTOMER_RECEIPT_DETAIL: PermissionDef = PermissionDef {
    code: "customer_receipt:detail",
    group: groups::INVOICE,
    description: "查询客户回款单详情",
};

/// 登记客户回款草稿。
pub const CUSTOMER_RECEIPT_CREATE: PermissionDef = PermissionDef {
    code: "customer_receipt:create",
    group: groups::INVOICE,
    description: "登记客户回款草稿",
};

/// 原子创建或提交客户回款审批。
pub const CUSTOMER_RECEIPT_COMMIT: PermissionDef =
    CUSTOMER_RECEIPT_SUBMIT.described(groups::INVOICE, "原子创建或提交客户回款审批");

/// 提交客户回款审批。
pub const CUSTOMER_RECEIPT_SUBMIT: PermissionDef = PermissionDef {
    code: "customer_receipt:submit",
    group: groups::INVOICE,
    description: "提交客户回款审批",
};

/// 撤回客户回款审批。
pub const CUSTOMER_RECEIPT_CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "customer_receipt:cancel_approval",
    group: groups::INVOICE,
    description: "撤回客户回款审批",
};

/// 客户回款过账并核销。
pub const CUSTOMER_RECEIPT_POST: PermissionDef = PermissionDef {
    code: "customer_receipt:post",
    group: groups::INVOICE,
    description: "客户回款过账并核销",
};

/// 查询发票列表。
pub const INVOICE_LIST: PermissionDef =
    PermissionDef { code: "invoice:list", group: groups::INVOICE, description: "查询发票列表" };

/// 查询发票详情。
pub const INVOICE_DETAIL: PermissionDef =
    PermissionDef { code: "invoice:detail", group: groups::INVOICE, description: "查询发票详情" };

/// 登记发票草稿。
pub const INVOICE_CREATE: PermissionDef =
    PermissionDef { code: "invoice:create", group: groups::INVOICE, description: "登记发票草稿" };

/// 原子登记销项发票并分配。
pub const INVOICE_COMMIT: PermissionDef = INVOICE_POST.described(groups::INVOICE, "原子登记销项发票并分配");

/// 发票登记过账并分配。
pub const INVOICE_POST: PermissionDef =
    PermissionDef {
        code: "invoice:post", group: groups::INVOICE, description: "发票登记过账并分配"
    };

/// 开具红票并红冲。
pub const INVOICE_RED_ISSUE: PermissionDef =
    PermissionDef { code: "invoice:reverse", group: groups::INVOICE, description: "开具红票并红冲" };

/// 保存回款草稿及凭证附件。
pub const RECEIPT_FILES_CUSTOMER_RECEIPT_SAVE_WITH_FILES: PermissionDef =
    CUSTOMER_RECEIPT_SUBMIT.described(groups::INVOICE, "保存回款草稿及凭证附件");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("DRAFT_READ_CUSTOMER_RECEIPT_DRAFT", DRAFT_READ_CUSTOMER_RECEIPT_DRAFT),
    ("DRAFT_UPDATE_CUSTOMER_RECEIPT_UPDATE", DRAFT_UPDATE_CUSTOMER_RECEIPT_UPDATE),
    ("DRAFT_UPDATE_CUSTOMER_RECEIPT_SAVE_DRAFT", DRAFT_UPDATE_CUSTOMER_RECEIPT_SAVE_DRAFT),
    ("INVOICE_FILES_INVOICE_CREATE_WITH_FILES", INVOICE_FILES_INVOICE_CREATE_WITH_FILES),
    ("INVOICE_FILES_INVOICE_COMMIT_WITH_FILES", INVOICE_FILES_INVOICE_COMMIT_WITH_FILES),
    ("INVOICE_FILES_INVOICE_COMPLETE_WITH_FILES", INVOICE_FILES_INVOICE_COMPLETE_WITH_FILES),
    ("INVOICE_REQUEST_INVOICE_REQUEST_LIST", INVOICE_REQUEST_INVOICE_REQUEST_LIST),
    ("INVOICE_REQUEST_INVOICE_REQUEST_DETAIL", INVOICE_REQUEST_INVOICE_REQUEST_DETAIL),
    ("INVOICE_REQUEST_INVOICE_REQUEST_AMOUNTS", INVOICE_REQUEST_INVOICE_REQUEST_AMOUNTS),
    ("INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT", INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT),
    ("INVOICE_REQUEST_INVOICE_REQUEST_CANCEL", INVOICE_REQUEST_INVOICE_REQUEST_CANCEL),
    ("ACCOUNT_LIST", ACCOUNT_LIST),
    ("ACCOUNT_DETAIL", ACCOUNT_DETAIL),
    ("ACCOUNT_CREATE", ACCOUNT_CREATE),
    ("CUSTOMER_RECEIPT_LIST", CUSTOMER_RECEIPT_LIST),
    ("CUSTOMER_RECEIPT_DETAIL", CUSTOMER_RECEIPT_DETAIL),
    ("CUSTOMER_RECEIPT_CREATE", CUSTOMER_RECEIPT_CREATE),
    ("CUSTOMER_RECEIPT_COMMIT", CUSTOMER_RECEIPT_COMMIT),
    ("CUSTOMER_RECEIPT_SUBMIT", CUSTOMER_RECEIPT_SUBMIT),
    ("CUSTOMER_RECEIPT_CANCEL_APPROVAL", CUSTOMER_RECEIPT_CANCEL_APPROVAL),
    ("CUSTOMER_RECEIPT_POST", CUSTOMER_RECEIPT_POST),
    ("INVOICE_LIST", INVOICE_LIST),
    ("INVOICE_DETAIL", INVOICE_DETAIL),
    ("INVOICE_CREATE", INVOICE_CREATE),
    ("INVOICE_COMMIT", INVOICE_COMMIT),
    ("INVOICE_POST", INVOICE_POST),
    ("INVOICE_RED_ISSUE", INVOICE_RED_ISSUE),
    ("RECEIPT_FILES_CUSTOMER_RECEIPT_SAVE_WITH_FILES", RECEIPT_FILES_CUSTOMER_RECEIPT_SAVE_WITH_FILES),
];
