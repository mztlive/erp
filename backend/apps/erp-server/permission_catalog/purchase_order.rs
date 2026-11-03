//! purchase_order 的接口权限目录。
use super::{PermissionDef, groups, purchase_payment_request};

/// 读取原采购变更草稿。
pub const CHANGE_DRAFT_PURCHASE_CHANGE_DRAFT: PermissionDef =
    PURCHASE_CHANGE_SUBMIT.described(groups::PURCHASE_ORDER, "读取原采购变更草稿");

/// 作废采购变更单。
pub const CHANGE_VOID_PURCHASE_CHANGE_VOID: PermissionDef = PermissionDef {
    code: "purchase_change_order:delete",
    group: groups::PURCHASE_ORDER,
    description: "作废采购变更单",
};

/// 下载供应商采购单。
pub const CORRESPONDENCE_PURCHASE_PAPER: PermissionDef =
    DETAIL.described(groups::PURCHASE_ORDER, "下载供应商采购单");

/// 发送采购单邮件给供应商。
pub const CORRESPONDENCE_SEND_PURCHASE_EMAIL: PermissionDef = PermissionDef {
    code: "purchase_order:send_email",
    group: groups::PURCHASE_ORDER,
    description: "发送采购单邮件给供应商",
};

/// 查看采购邮件发送记录。
pub const CORRESPONDENCE_PURCHASE_EMAIL_HISTORY: PermissionDef =
    DETAIL.described(groups::PURCHASE_ORDER, "查看采购邮件发送记录");

/// 保存采购草稿。
pub const DELIVERY_OPTIONS_PURCHASE_DELIVERY_OPTIONS: PermissionDef = SAVE_DRAFT;

/// 查询费用采购来源销售单。
pub const EXPENSE_EXPENSE_SOURCES: PermissionDef =
    CREATE.described(groups::PURCHASE_ORDER, "查询费用采购来源销售单");

/// 读取费用采购来源明细。
pub const EXPENSE_EXPENSE_SOURCE_DETAIL: PermissionDef =
    CREATE.described(groups::PURCHASE_ORDER, "读取费用采购来源明细");

/// 从销售单追加费用采购单。
pub const EXPENSE_EXPENSE_PURCHASE_CREATE: PermissionDef =
    CREATE.described(groups::PURCHASE_ORDER, "从销售单追加费用采购单");

/// 确认采购费用行已发生。
pub const EXPENSE_EXPENSE_OCCURRENCE_CONFIRM: PermissionDef = PermissionDef {
    code: "purchase_order:confirm_expense",
    group: groups::PURCHASE_ORDER,
    description: "确认采购费用行已发生",
};

/// 标记供应商不收运费。
pub const FREIGHT_FREIGHT_WAIVER_CREATE: PermissionDef =
    SAVE_DRAFT.described(groups::PURCHASE_ORDER, "标记供应商不收运费");

/// 查询采购单列表。
pub const LIST: PermissionDef = PermissionDef {
    code: "purchase_order:list",
    group: groups::PURCHASE_ORDER,
    description: "查询采购单列表",
};

/// 查询采购单对象中心。
pub const DETAIL: PermissionDef = PermissionDef {
    code: "purchase_order:detail",
    group: groups::PURCHASE_ORDER,
    description: "查询采购单对象中心",
};

/// 依据采购确认创建采购单。
pub const CREATE: PermissionDef = PermissionDef {
    code: "purchase_order:create",
    group: groups::PURCHASE_ORDER,
    description: "依据采购确认创建采购单",
};

/// 确认供给分配。
pub const CREATE_FROM_SOURCING: PermissionDef = CREATE.described(groups::PURCHASE_ORDER, "确认供给分配");

/// 保存采购草稿。
pub const SAVE_DRAFT: PermissionDef = PermissionDef {
    code: "purchase_order:update",
    group: groups::PURCHASE_ORDER,
    description: "保存采购草稿",
};

/// 作废采购单草稿。
pub const VOID: PermissionDef = PermissionDef {
    code: "purchase_order:delete",
    group: groups::PURCHASE_ORDER,
    description: "作废采购单草稿",
};

/// 提交采购单审批。
pub const SUBMIT: PermissionDef = PermissionDef {
    code: "purchase_order:submit",
    group: groups::PURCHASE_ORDER,
    description: "提交采购单审批",
};

/// 登记维持原采购依据。
pub const MAINTAIN_SOURCE: PermissionDef = PermissionDef {
    code: "purchase_order:maintain_source",
    group: groups::PURCHASE_ORDER,
    description: "登记维持原采购依据",
};

/// 撤回采购单审批。
pub const CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "purchase_order:cancel_approval",
    group: groups::PURCHASE_ORDER,
    description: "撤回采购单审批",
};

/// 查询供给分配依据。
pub const PURCHASE_CREATION_BASIS_LIST: PermissionDef =
    CREATE.described(groups::PURCHASE_ORDER, "查询供给分配依据");

/// 发起采购变更。
pub const PURCHASE_CHANGE_CREATE: PermissionDef = PermissionDef {
    code: "purchase_change_order:create",
    group: groups::PURCHASE_ORDER,
    description: "发起采购变更",
};

/// 提交采购变更审批。
pub const PURCHASE_CHANGE_SUBMIT: PermissionDef = PermissionDef {
    code: "purchase_change_order:submit",
    group: groups::PURCHASE_ORDER,
    description: "提交采购变更审批",
};

/// 采购变更生效。
pub const PURCHASE_CHANGE_EFFECT: PermissionDef = PermissionDef {
    code: "purchase_change_order:post",
    group: groups::PURCHASE_ORDER,
    description: "采购变更生效",
};

/// 撤回采购变更审批。
pub const PURCHASE_CHANGE_CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "purchase_change_order:cancel_approval",
    group: groups::PURCHASE_ORDER,
    description: "撤回采购变更审批",
};

/// 查询采购变更单列表。
pub const PURCHASE_CHANGE_LIST: PermissionDef = PermissionDef {
    code: "purchase_change_order:list",
    group: groups::PURCHASE_ORDER,
    description: "查询采购变更单列表",
};

/// 查询采购变更单详情。
pub const PURCHASE_CHANGE_DETAIL: PermissionDef = PermissionDef {
    code: "purchase_change_order:detail",
    group: groups::PURCHASE_ORDER,
    description: "查询采购变更单详情",
};

/// 查询采购付款安排。
pub const PAYMENT_ARRANGEMENT_PAYMENT_ARRANGEMENT: PermissionDef =
    DETAIL.described(groups::PURCHASE_ORDER, "查询采购付款安排");

/// 安排采购付款跟进。
pub const PAYMENT_ARRANGEMENT_SCHEDULE_PAYMENT_FOLLOW_UP: PermissionDef =
    purchase_payment_request::SUBMIT.described(groups::PURCHASE_PAYMENT_REQUEST, "安排采购付款跟进");

/// 查询采购单对象中心。
pub const PAYMENTS_PURCHASE_PAYMENTS: PermissionDef = DETAIL;

/// 查询采购单对象中心。
pub const PAYMENTS_PURCHASE_PAYMENT_RECEIPT: PermissionDef = DETAIL;

/// 下载采购关联销售合同及凭证。
pub const SALES_MATERIAL_PURCHASE_SALES_MATERIAL_DOWNLOAD: PermissionDef =
    DETAIL.described(groups::PURCHASE_ORDER, "下载采购关联销售合同及凭证");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("CHANGE_DRAFT_PURCHASE_CHANGE_DRAFT", CHANGE_DRAFT_PURCHASE_CHANGE_DRAFT),
    ("CHANGE_VOID_PURCHASE_CHANGE_VOID", CHANGE_VOID_PURCHASE_CHANGE_VOID),
    ("CORRESPONDENCE_PURCHASE_PAPER", CORRESPONDENCE_PURCHASE_PAPER),
    ("CORRESPONDENCE_SEND_PURCHASE_EMAIL", CORRESPONDENCE_SEND_PURCHASE_EMAIL),
    ("CORRESPONDENCE_PURCHASE_EMAIL_HISTORY", CORRESPONDENCE_PURCHASE_EMAIL_HISTORY),
    ("DELIVERY_OPTIONS_PURCHASE_DELIVERY_OPTIONS", DELIVERY_OPTIONS_PURCHASE_DELIVERY_OPTIONS),
    ("EXPENSE_EXPENSE_SOURCES", EXPENSE_EXPENSE_SOURCES),
    ("EXPENSE_EXPENSE_SOURCE_DETAIL", EXPENSE_EXPENSE_SOURCE_DETAIL),
    ("EXPENSE_EXPENSE_PURCHASE_CREATE", EXPENSE_EXPENSE_PURCHASE_CREATE),
    ("EXPENSE_EXPENSE_OCCURRENCE_CONFIRM", EXPENSE_EXPENSE_OCCURRENCE_CONFIRM),
    ("FREIGHT_FREIGHT_WAIVER_CREATE", FREIGHT_FREIGHT_WAIVER_CREATE),
    ("LIST", LIST),
    ("DETAIL", DETAIL),
    ("CREATE", CREATE),
    ("CREATE_FROM_SOURCING", CREATE_FROM_SOURCING),
    ("SAVE_DRAFT", SAVE_DRAFT),
    ("VOID", VOID),
    ("SUBMIT", SUBMIT),
    ("MAINTAIN_SOURCE", MAINTAIN_SOURCE),
    ("CANCEL_APPROVAL", CANCEL_APPROVAL),
    ("PURCHASE_CREATION_BASIS_LIST", PURCHASE_CREATION_BASIS_LIST),
    ("PURCHASE_CHANGE_CREATE", PURCHASE_CHANGE_CREATE),
    ("PURCHASE_CHANGE_SUBMIT", PURCHASE_CHANGE_SUBMIT),
    ("PURCHASE_CHANGE_EFFECT", PURCHASE_CHANGE_EFFECT),
    ("PURCHASE_CHANGE_CANCEL_APPROVAL", PURCHASE_CHANGE_CANCEL_APPROVAL),
    ("PURCHASE_CHANGE_LIST", PURCHASE_CHANGE_LIST),
    ("PURCHASE_CHANGE_DETAIL", PURCHASE_CHANGE_DETAIL),
    ("PAYMENT_ARRANGEMENT_PAYMENT_ARRANGEMENT", PAYMENT_ARRANGEMENT_PAYMENT_ARRANGEMENT),
    ("PAYMENT_ARRANGEMENT_SCHEDULE_PAYMENT_FOLLOW_UP", PAYMENT_ARRANGEMENT_SCHEDULE_PAYMENT_FOLLOW_UP),
    ("PAYMENTS_PURCHASE_PAYMENTS", PAYMENTS_PURCHASE_PAYMENTS),
    ("PAYMENTS_PURCHASE_PAYMENT_RECEIPT", PAYMENTS_PURCHASE_PAYMENT_RECEIPT),
    ("SALES_MATERIAL_PURCHASE_SALES_MATERIAL_DOWNLOAD", SALES_MATERIAL_PURCHASE_SALES_MATERIAL_DOWNLOAD),
];
