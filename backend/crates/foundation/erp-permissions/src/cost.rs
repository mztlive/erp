//! cost 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询销售单附加费用。
pub const EXTRA_COST_SALES_EXTRA_COST_LIST: Permission = ENTRY_LIST;

/// 登记销售单附加费用。
pub const EXTRA_COST_SALES_EXTRA_COST_REGISTER: Permission = ENTRY_CREATE;

/// 冲减销售单附加费用。
pub const EXTRA_COST_SALES_EXTRA_COST_REDUCE: Permission = ENTRY_CREATE;

/// 查询成本事实列表。
pub const ENTRY_LIST: Permission = Permission::new(resources::COST_ENTRY, "list");

/// 查询成本事实详情。
pub const ENTRY_DETAIL: Permission = Permission::new(resources::COST_ENTRY, "detail");

/// 手工登记成本事实。
pub const ENTRY_CREATE: Permission = Permission::new(resources::COST_ENTRY, "create");

/// 查询成本分配列表。
pub const ALLOCATION_LIST: Permission = Permission::new(resources::COST_ALLOCATION, "list");

/// 查询成本事实列表。
pub const PROFIT_LOSS_PERIOD_BASIS: Permission = ENTRY_LIST;

/// 查询成本事实列表。
pub const PROFIT_LOSS_VIEW: Permission = ENTRY_LIST;

/// 查询成本事实列表。
pub const PROFIT_LOSS_EXPORT: Permission = ENTRY_LIST;

/// 查询历史归属候选。
pub const PROFIT_LOSS_HISTORY_DIRECTORY: Permission = ENTRY_LIST;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "ENTRY_LIST",
        PermissionDef {
            permission: &ENTRY_LIST, group: groups::COST, description: "查询成本事实列表"
        },
    ),
    (
        "ENTRY_DETAIL",
        PermissionDef {
            permission: &ENTRY_DETAIL, group: groups::COST, description: "查询成本事实详情"
        },
    ),
    (
        "ENTRY_CREATE",
        PermissionDef {
            permission: &ENTRY_CREATE, group: groups::COST, description: "手工登记成本事实"
        },
    ),
    (
        "ALLOCATION_LIST",
        PermissionDef {
            permission: &ALLOCATION_LIST, group: groups::COST, description: "查询成本分配列表"
        },
    ),
];
