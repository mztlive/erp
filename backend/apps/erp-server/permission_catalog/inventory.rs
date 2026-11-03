//! inventory 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询库存余额列表。
pub const STOCK_BALANCE_LIST: PermissionDef =
    PermissionDef {
        code: "stock_balance:list", group: groups::INVENTORY, description: "查询库存余额列表"
    };

/// 查询库存余额详情。
pub const STOCK_BALANCE_DETAIL: PermissionDef =
    PermissionDef {
        code: "stock_balance:detail", group: groups::INVENTORY, description: "查询库存余额详情"
    };

/// 查询库存流水台账。
pub const STOCK_MOVEMENT_LIST: PermissionDef =
    PermissionDef {
        code: "stock_movement:list", group: groups::INVENTORY, description: "查询库存流水台账"
    };

/// 查询库存预占列表。
pub const STOCK_RESERVATION_LIST: PermissionDef = PermissionDef {
    code: "stock_reservation:list",
    group: groups::INVENTORY,
    description: "查询库存预占列表",
};

/// 查询库存调整单列表。
pub const STOCK_ADJUSTMENT_LIST: PermissionDef = PermissionDef {
    code: "stock_adjustment:list",
    group: groups::INVENTORY,
    description: "查询库存调整单列表",
};

/// 查询库存调整单详情。
pub const STOCK_ADJUSTMENT_DETAIL: PermissionDef = PermissionDef {
    code: "stock_adjustment:detail",
    group: groups::INVENTORY,
    description: "查询库存调整单详情",
};

/// 创建库存调整单。
pub const STOCK_ADJUSTMENT_CREATE: PermissionDef = PermissionDef {
    code: "stock_adjustment:create",
    group: groups::INVENTORY,
    description: "创建库存调整单",
};

/// 更新库存调整单。
pub const STOCK_ADJUSTMENT_UPDATE: PermissionDef = PermissionDef {
    code: "stock_adjustment:update",
    group: groups::INVENTORY,
    description: "更新库存调整单",
};

/// 提交库存调整审批。
pub const STOCK_ADJUSTMENT_SUBMIT: PermissionDef = PermissionDef {
    code: "stock_adjustment:submit",
    group: groups::INVENTORY,
    description: "提交库存调整审批",
};

/// 查询库存调整提交结果。
pub const STOCK_ADJUSTMENT_SUBMIT_RESULT: PermissionDef =
    STOCK_ADJUSTMENT_DETAIL.described(groups::INVENTORY, "查询库存调整提交结果");

/// 撤回库存调整审批。
pub const STOCK_ADJUSTMENT_CANCEL_APPROVAL: PermissionDef = PermissionDef {
    code: "approval_instance:cancel",
    group: groups::APPROVAL_INSTANCE,
    description: "撤回库存调整审批",
};

/// 过账库存调整。
pub const STOCK_ADJUSTMENT_POST: PermissionDef =
    PermissionDef {
        code: "stock_adjustment:post", group: groups::INVENTORY, description: "过账库存调整"
    };

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("STOCK_BALANCE_LIST", STOCK_BALANCE_LIST),
    ("STOCK_BALANCE_DETAIL", STOCK_BALANCE_DETAIL),
    ("STOCK_MOVEMENT_LIST", STOCK_MOVEMENT_LIST),
    ("STOCK_RESERVATION_LIST", STOCK_RESERVATION_LIST),
    ("STOCK_ADJUSTMENT_LIST", STOCK_ADJUSTMENT_LIST),
    ("STOCK_ADJUSTMENT_DETAIL", STOCK_ADJUSTMENT_DETAIL),
    ("STOCK_ADJUSTMENT_CREATE", STOCK_ADJUSTMENT_CREATE),
    ("STOCK_ADJUSTMENT_UPDATE", STOCK_ADJUSTMENT_UPDATE),
    ("STOCK_ADJUSTMENT_SUBMIT", STOCK_ADJUSTMENT_SUBMIT),
    ("STOCK_ADJUSTMENT_SUBMIT_RESULT", STOCK_ADJUSTMENT_SUBMIT_RESULT),
    ("STOCK_ADJUSTMENT_CANCEL_APPROVAL", STOCK_ADJUSTMENT_CANCEL_APPROVAL),
    ("STOCK_ADJUSTMENT_POST", STOCK_ADJUSTMENT_POST),
];
