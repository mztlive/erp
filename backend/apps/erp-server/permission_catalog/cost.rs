//! cost 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询销售单附加费用。
pub const EXTRA_COST_SALES_EXTRA_COST_LIST: PermissionDef =
    ENTRY_LIST.described(groups::COST, "查询销售单附加费用");

/// 登记销售单附加费用。
pub const EXTRA_COST_SALES_EXTRA_COST_REGISTER: PermissionDef =
    ENTRY_CREATE.described(groups::COST, "登记销售单附加费用");

/// 冲减销售单附加费用。
pub const EXTRA_COST_SALES_EXTRA_COST_REDUCE: PermissionDef =
    ENTRY_CREATE.described(groups::COST, "冲减销售单附加费用");

/// 查询成本事实列表。
pub const ENTRY_LIST: PermissionDef =
    PermissionDef { code: "cost_entry:list", group: groups::COST, description: "查询成本事实列表" };

/// 查询成本事实详情。
pub const ENTRY_DETAIL: PermissionDef =
    PermissionDef { code: "cost_entry:detail", group: groups::COST, description: "查询成本事实详情" };

/// 手工登记成本事实。
pub const ENTRY_CREATE: PermissionDef =
    PermissionDef { code: "cost_entry:create", group: groups::COST, description: "手工登记成本事实" };

/// 查询成本分配列表。
pub const ALLOCATION_LIST: PermissionDef =
    PermissionDef {
        code: "cost_allocation:list", group: groups::COST, description: "查询成本分配列表"
    };

/// 查询成本事实列表。
pub const PROFIT_LOSS_PERIOD_BASIS: PermissionDef = ENTRY_LIST;

/// 查询成本事实列表。
pub const PROFIT_LOSS_VIEW: PermissionDef = ENTRY_LIST;

/// 查询成本事实列表。
pub const PROFIT_LOSS_EXPORT: PermissionDef = ENTRY_LIST;

/// 查询历史归属候选。
pub const PROFIT_LOSS_HISTORY_DIRECTORY: PermissionDef =
    ENTRY_LIST.described(groups::COST_ENTRY, "查询历史归属候选");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("EXTRA_COST_SALES_EXTRA_COST_LIST", EXTRA_COST_SALES_EXTRA_COST_LIST),
    ("EXTRA_COST_SALES_EXTRA_COST_REGISTER", EXTRA_COST_SALES_EXTRA_COST_REGISTER),
    ("EXTRA_COST_SALES_EXTRA_COST_REDUCE", EXTRA_COST_SALES_EXTRA_COST_REDUCE),
    ("ENTRY_LIST", ENTRY_LIST),
    ("ENTRY_DETAIL", ENTRY_DETAIL),
    ("ENTRY_CREATE", ENTRY_CREATE),
    ("ALLOCATION_LIST", ALLOCATION_LIST),
    ("PROFIT_LOSS_PERIOD_BASIS", PROFIT_LOSS_PERIOD_BASIS),
    ("PROFIT_LOSS_VIEW", PROFIT_LOSS_VIEW),
    ("PROFIT_LOSS_EXPORT", PROFIT_LOSS_EXPORT),
    ("PROFIT_LOSS_HISTORY_DIRECTORY", PROFIT_LOSS_HISTORY_DIRECTORY),
];
