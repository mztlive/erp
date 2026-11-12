//! receivable 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 读取原回款编辑字段。
pub const DRAFT_READ_CUSTOMER_RECEIPT_DRAFT: Permission = CUSTOMER_RECEIPT_SUBMIT;

/// 修改原财务单据草稿。
pub const DRAFT_UPDATE_CUSTOMER_RECEIPT_UPDATE: Permission = CUSTOMER_RECEIPT_SUBMIT;

/// 保存回款登记草稿。
pub const DRAFT_UPDATE_CUSTOMER_RECEIPT_SAVE_DRAFT: Permission = CUSTOMER_RECEIPT_SUBMIT;

/// 登记发票草稿。
pub const INVOICE_FILES_INVOICE_CREATE_WITH_FILES: Permission = INVOICE_CREATE;

/// 原子登记销项发票并分配。
pub const INVOICE_FILES_INVOICE_COMMIT_WITH_FILES: Permission = INVOICE_POST;

/// 上传附件并完成开票。
pub const INVOICE_FILES_INVOICE_COMPLETE_WITH_FILES: Permission = INVOICE_POST;

/// 查询开票申请列表。
pub const INVOICE_REQUEST_INVOICE_REQUEST_LIST: Permission =
    Permission::new(resources::SALES_INVOICE_REQUEST, "list");

/// 查询开票申请详情。
pub const INVOICE_REQUEST_INVOICE_REQUEST_DETAIL: Permission =
    Permission::new(resources::SALES_INVOICE_REQUEST, "detail");

/// 查询可申请开票额度。
pub const INVOICE_REQUEST_INVOICE_REQUEST_AMOUNTS: Permission = INVOICE_REQUEST_INVOICE_REQUEST_LIST;

/// 提交开票申请。
pub const INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT: Permission =
    Permission::new(resources::SALES_INVOICE_REQUEST, "submit");

/// 保存可编辑开票申请。
pub const INVOICE_REQUEST_UPDATE: Permission = INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT;

/// 撤回开票申请。
pub const INVOICE_REQUEST_INVOICE_REQUEST_CANCEL: Permission =
    Permission::new(resources::SALES_INVOICE_REQUEST, "cancel");

/// 查询应收往来子账列表。
pub const ACCOUNT_LIST: Permission = Permission::new(resources::RECEIVABLE_ACCOUNT, "list");

/// 查询应收往来子账详情。
pub const ACCOUNT_DETAIL: Permission = Permission::new(resources::RECEIVABLE_ACCOUNT, "detail");

/// 建立应收往来子账。
pub const ACCOUNT_CREATE: Permission = Permission::new(resources::RECEIVABLE_ACCOUNT, "create");

/// 查询客户回款单列表。
pub const CUSTOMER_RECEIPT_LIST: Permission = Permission::new(resources::CUSTOMER_RECEIPT, "list");

/// 查询客户回款单详情。
pub const CUSTOMER_RECEIPT_DETAIL: Permission = Permission::new(resources::CUSTOMER_RECEIPT, "detail");

/// 登记客户回款草稿。
pub const CUSTOMER_RECEIPT_CREATE: Permission = Permission::new(resources::CUSTOMER_RECEIPT, "create");

/// 原子创建或提交客户回款审批。
pub const CUSTOMER_RECEIPT_COMMIT: Permission = CUSTOMER_RECEIPT_SUBMIT;

/// 提交客户回款审批。
pub const CUSTOMER_RECEIPT_SUBMIT: Permission = Permission::new(resources::CUSTOMER_RECEIPT, "submit");

/// 撤回客户回款审批。
pub const CUSTOMER_RECEIPT_CANCEL_APPROVAL: Permission =
    Permission::new(resources::CUSTOMER_RECEIPT, "cancel_approval");

/// 客户回款过账并核销。
pub const CUSTOMER_RECEIPT_POST: Permission = Permission::new(resources::CUSTOMER_RECEIPT, "post");

/// 查询发票列表。
pub const INVOICE_LIST: Permission = Permission::new(resources::INVOICE, "list");

/// 查询发票详情。
pub const INVOICE_DETAIL: Permission = Permission::new(resources::INVOICE, "detail");

/// 登记发票草稿。
pub const INVOICE_CREATE: Permission = Permission::new(resources::INVOICE, "create");

/// 原子登记销项发票并分配。
pub const INVOICE_COMMIT: Permission = INVOICE_POST;

/// 发票登记过账并分配。
pub const INVOICE_POST: Permission = Permission::new(resources::INVOICE, "post");

/// 开具红票并红冲。
pub const INVOICE_RED_ISSUE: Permission = Permission::new(resources::INVOICE, "reverse");

/// 保存回款草稿及凭证附件。
pub const RECEIPT_FILES_CUSTOMER_RECEIPT_SAVE_WITH_FILES: Permission = CUSTOMER_RECEIPT_SUBMIT;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "INVOICE_REQUEST_INVOICE_REQUEST_LIST",
        PermissionDef {
            permission: &INVOICE_REQUEST_INVOICE_REQUEST_LIST,
            group: groups::RECEIVABLE,
            description: "查询开票申请列表",
        },
    ),
    (
        "INVOICE_REQUEST_INVOICE_REQUEST_DETAIL",
        PermissionDef {
            permission: &INVOICE_REQUEST_INVOICE_REQUEST_DETAIL,
            group: groups::RECEIVABLE,
            description: "查询开票申请详情",
        },
    ),
    (
        "INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT",
        PermissionDef {
            permission: &INVOICE_REQUEST_INVOICE_REQUEST_SUBMIT,
            group: groups::RECEIVABLE,
            description: "提交开票申请",
        },
    ),
    (
        "INVOICE_REQUEST_INVOICE_REQUEST_CANCEL",
        PermissionDef {
            permission: &INVOICE_REQUEST_INVOICE_REQUEST_CANCEL,
            group: groups::RECEIVABLE,
            description: "撤回开票申请",
        },
    ),
    (
        "ACCOUNT_LIST",
        PermissionDef {
            permission: &ACCOUNT_LIST,
            group: groups::INVOICE,
            description: "查询应收往来子账列表",
        },
    ),
    (
        "ACCOUNT_DETAIL",
        PermissionDef {
            permission: &ACCOUNT_DETAIL,
            group: groups::INVOICE,
            description: "查询应收往来子账详情",
        },
    ),
    (
        "ACCOUNT_CREATE",
        PermissionDef {
            permission: &ACCOUNT_CREATE,
            group: groups::INVOICE,
            description: "建立应收往来子账",
        },
    ),
    (
        "CUSTOMER_RECEIPT_LIST",
        PermissionDef {
            permission: &CUSTOMER_RECEIPT_LIST,
            group: groups::INVOICE,
            description: "查询客户回款单列表",
        },
    ),
    (
        "CUSTOMER_RECEIPT_DETAIL",
        PermissionDef {
            permission: &CUSTOMER_RECEIPT_DETAIL,
            group: groups::INVOICE,
            description: "查询客户回款单详情",
        },
    ),
    (
        "CUSTOMER_RECEIPT_CREATE",
        PermissionDef {
            permission: &CUSTOMER_RECEIPT_CREATE,
            group: groups::INVOICE,
            description: "登记客户回款草稿",
        },
    ),
    (
        "CUSTOMER_RECEIPT_SUBMIT",
        PermissionDef {
            permission: &CUSTOMER_RECEIPT_SUBMIT,
            group: groups::INVOICE,
            description: "提交客户回款审批",
        },
    ),
    (
        "CUSTOMER_RECEIPT_CANCEL_APPROVAL",
        PermissionDef {
            permission: &CUSTOMER_RECEIPT_CANCEL_APPROVAL,
            group: groups::INVOICE,
            description: "撤回客户回款审批",
        },
    ),
    (
        "CUSTOMER_RECEIPT_POST",
        PermissionDef {
            permission: &CUSTOMER_RECEIPT_POST,
            group: groups::INVOICE,
            description: "客户回款过账并核销",
        },
    ),
    (
        "INVOICE_LIST",
        PermissionDef {
            permission: &INVOICE_LIST, group: groups::INVOICE, description: "查询发票列表"
        },
    ),
    (
        "INVOICE_DETAIL",
        PermissionDef {
            permission: &INVOICE_DETAIL, group: groups::INVOICE, description: "查询发票详情"
        },
    ),
    (
        "INVOICE_CREATE",
        PermissionDef {
            permission: &INVOICE_CREATE, group: groups::INVOICE, description: "登记发票草稿"
        },
    ),
    (
        "INVOICE_POST",
        PermissionDef {
            permission: &INVOICE_POST,
            group: groups::INVOICE,
            description: "发票登记过账并分配",
        },
    ),
    (
        "INVOICE_RED_ISSUE",
        PermissionDef {
            permission: &INVOICE_RED_ISSUE,
            group: groups::INVOICE,
            description: "开具红票并红冲",
        },
    ),
];
