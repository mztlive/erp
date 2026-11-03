//! sales_review 的接口权限目录。
use super::{PermissionDef, groups};

/// 读取销售变更原单草稿。
pub const SALES_CHANGE_DRAFT: PermissionDef = PermissionDef {
    code: "sales_change_order:submit",
    group: groups::SALES_REVIEW,
    description: "读取销售变更原单草稿",
};

/// 修改销售变更原单草稿。
pub const SALES_CHANGE_SAVE_DRAFT: PermissionDef =
    SALES_CHANGE_DRAFT.described(groups::SALES_REVIEW, "修改销售变更原单草稿");

/// 查询销售变更单列表。
pub const SALES_CHANGE_ORDER_LIST: PermissionDef = PermissionDef {
    code: "sales_change_order:list",
    group: groups::SALES_REVIEW,
    description: "查询销售变更单列表",
};

/// 查询销售变更单详情。
pub const SALES_CHANGE_ORDER_DETAIL: PermissionDef = PermissionDef {
    code: "sales_change_order:detail",
    group: groups::SALES_REVIEW,
    description: "查询销售变更单详情",
};

/// 创建销售变更单。
pub const SALES_CHANGE_ORDER_CREATE: PermissionDef = PermissionDef {
    code: "sales_change_order:create",
    group: groups::SALES_REVIEW,
    description: "创建销售变更单",
};

/// 发起销售变更影响确认。
pub const SALES_CHANGE_ORDER_SUBMIT_IMPACT: PermissionDef =
    SALES_CHANGE_DRAFT.described(groups::SALES_REVIEW, "发起销售变更影响确认");

/// 作废销售变更单。
pub const SALES_CHANGE_ORDER_VOID: PermissionDef = PermissionDef {
    code: "sales_change_order:delete",
    group: groups::SALES_REVIEW,
    description: "作废销售变更单",
};

/// 撤回销售变更审批。
pub const SALES_CHANGE_ORDER_CANCEL_APPROVAL: PermissionDef =
    SALES_CHANGE_DRAFT.described(groups::SALES_REVIEW, "撤回销售变更审批");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("SALES_CHANGE_DRAFT", SALES_CHANGE_DRAFT),
    ("SALES_CHANGE_SAVE_DRAFT", SALES_CHANGE_SAVE_DRAFT),
    ("SALES_CHANGE_ORDER_LIST", SALES_CHANGE_ORDER_LIST),
    ("SALES_CHANGE_ORDER_DETAIL", SALES_CHANGE_ORDER_DETAIL),
    ("SALES_CHANGE_ORDER_CREATE", SALES_CHANGE_ORDER_CREATE),
    ("SALES_CHANGE_ORDER_SUBMIT_IMPACT", SALES_CHANGE_ORDER_SUBMIT_IMPACT),
    ("SALES_CHANGE_ORDER_VOID", SALES_CHANGE_ORDER_VOID),
    ("SALES_CHANGE_ORDER_CANCEL_APPROVAL", SALES_CHANGE_ORDER_CANCEL_APPROVAL),
];
