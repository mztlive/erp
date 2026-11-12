//! inventory 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询库存余额列表。
pub const STOCK_BALANCE_LIST: Permission = Permission::new(resources::STOCK_BALANCE, "list");

/// 查询库存余额详情。
pub const STOCK_BALANCE_DETAIL: Permission = Permission::new(resources::STOCK_BALANCE, "detail");

/// 查询库存流水台账。
pub const STOCK_MOVEMENT_LIST: Permission = Permission::new(resources::STOCK_MOVEMENT, "list");

/// 查询库存预占列表。
pub const STOCK_RESERVATION_LIST: Permission = Permission::new(resources::STOCK_RESERVATION, "list");

/// 查询库存调整单列表。
pub const STOCK_ADJUSTMENT_LIST: Permission = Permission::new(resources::STOCK_ADJUSTMENT, "list");

/// 查询库存调整单详情。
pub const STOCK_ADJUSTMENT_DETAIL: Permission = Permission::new(resources::STOCK_ADJUSTMENT, "detail");

/// 创建库存调整单。
pub const STOCK_ADJUSTMENT_CREATE: Permission = Permission::new(resources::STOCK_ADJUSTMENT, "create");

/// 更新库存调整单。
pub const STOCK_ADJUSTMENT_UPDATE: Permission = Permission::new(resources::STOCK_ADJUSTMENT, "update");

/// 提交库存调整审批。
pub const STOCK_ADJUSTMENT_SUBMIT: Permission = Permission::new(resources::STOCK_ADJUSTMENT, "submit");

/// 查询库存调整提交结果。
pub const STOCK_ADJUSTMENT_SUBMIT_RESULT: Permission = STOCK_ADJUSTMENT_DETAIL;

/// 撤回库存调整审批。
pub const STOCK_ADJUSTMENT_CANCEL_APPROVAL: Permission =
    Permission::new(resources::APPROVAL_INSTANCE, "cancel");

/// 过账库存调整。
pub const STOCK_ADJUSTMENT_POST: Permission = Permission::new(resources::STOCK_ADJUSTMENT, "post");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "STOCK_BALANCE_LIST",
        PermissionDef {
            permission: &STOCK_BALANCE_LIST,
            group: groups::INVENTORY,
            description: "查询库存余额列表",
        },
    ),
    (
        "STOCK_BALANCE_DETAIL",
        PermissionDef {
            permission: &STOCK_BALANCE_DETAIL,
            group: groups::INVENTORY,
            description: "查询库存余额详情",
        },
    ),
    (
        "STOCK_MOVEMENT_LIST",
        PermissionDef {
            permission: &STOCK_MOVEMENT_LIST,
            group: groups::INVENTORY,
            description: "查询库存流水台账",
        },
    ),
    (
        "STOCK_RESERVATION_LIST",
        PermissionDef {
            permission: &STOCK_RESERVATION_LIST,
            group: groups::INVENTORY,
            description: "查询库存预占列表",
        },
    ),
    (
        "STOCK_ADJUSTMENT_LIST",
        PermissionDef {
            permission: &STOCK_ADJUSTMENT_LIST,
            group: groups::INVENTORY,
            description: "查询库存调整单列表",
        },
    ),
    (
        "STOCK_ADJUSTMENT_DETAIL",
        PermissionDef {
            permission: &STOCK_ADJUSTMENT_DETAIL,
            group: groups::INVENTORY,
            description: "查询库存调整单详情",
        },
    ),
    (
        "STOCK_ADJUSTMENT_CREATE",
        PermissionDef {
            permission: &STOCK_ADJUSTMENT_CREATE,
            group: groups::INVENTORY,
            description: "创建库存调整单",
        },
    ),
    (
        "STOCK_ADJUSTMENT_UPDATE",
        PermissionDef {
            permission: &STOCK_ADJUSTMENT_UPDATE,
            group: groups::INVENTORY,
            description: "更新库存调整单",
        },
    ),
    (
        "STOCK_ADJUSTMENT_SUBMIT",
        PermissionDef {
            permission: &STOCK_ADJUSTMENT_SUBMIT,
            group: groups::INVENTORY,
            description: "提交库存调整审批",
        },
    ),
    (
        "STOCK_ADJUSTMENT_CANCEL_APPROVAL",
        PermissionDef {
            permission: &STOCK_ADJUSTMENT_CANCEL_APPROVAL,
            group: groups::APPROVAL_INSTANCE,
            description: "撤回库存调整审批",
        },
    ),
    (
        "STOCK_ADJUSTMENT_POST",
        PermissionDef {
            permission: &STOCK_ADJUSTMENT_POST,
            group: groups::INVENTORY,
            description: "过账库存调整",
        },
    ),
];
