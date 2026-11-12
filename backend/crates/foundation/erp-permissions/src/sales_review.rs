//! sales_review 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 读取销售变更原单草稿。
pub const SALES_CHANGE_DRAFT: Permission = Permission::new(resources::SALES_CHANGE_ORDER, "submit");

/// 修改销售变更原单草稿。
pub const SALES_CHANGE_SAVE_DRAFT: Permission = SALES_CHANGE_DRAFT;

/// 查询销售变更单列表。
pub const SALES_CHANGE_ORDER_LIST: Permission = Permission::new(resources::SALES_CHANGE_ORDER, "list");

/// 查询销售变更单详情。
pub const SALES_CHANGE_ORDER_DETAIL: Permission = Permission::new(resources::SALES_CHANGE_ORDER, "detail");

/// 创建销售变更单。
pub const SALES_CHANGE_ORDER_CREATE: Permission = Permission::new(resources::SALES_CHANGE_ORDER, "create");

/// 发起销售变更影响确认。
pub const SALES_CHANGE_ORDER_SUBMIT_IMPACT: Permission = SALES_CHANGE_DRAFT;

/// 作废销售变更单。
pub const SALES_CHANGE_ORDER_VOID: Permission = Permission::new(resources::SALES_CHANGE_ORDER, "delete");

/// 撤回销售变更审批。
pub const SALES_CHANGE_ORDER_CANCEL_APPROVAL: Permission = SALES_CHANGE_DRAFT;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "SALES_CHANGE_DRAFT",
        PermissionDef {
            permission: &SALES_CHANGE_DRAFT,
            group: groups::SALES_REVIEW,
            description: "读取销售变更原单草稿",
        },
    ),
    (
        "SALES_CHANGE_ORDER_LIST",
        PermissionDef {
            permission: &SALES_CHANGE_ORDER_LIST,
            group: groups::SALES_REVIEW,
            description: "查询销售变更单列表",
        },
    ),
    (
        "SALES_CHANGE_ORDER_DETAIL",
        PermissionDef {
            permission: &SALES_CHANGE_ORDER_DETAIL,
            group: groups::SALES_REVIEW,
            description: "查询销售变更单详情",
        },
    ),
    (
        "SALES_CHANGE_ORDER_CREATE",
        PermissionDef {
            permission: &SALES_CHANGE_ORDER_CREATE,
            group: groups::SALES_REVIEW,
            description: "创建销售变更单",
        },
    ),
    (
        "SALES_CHANGE_ORDER_VOID",
        PermissionDef {
            permission: &SALES_CHANGE_ORDER_VOID,
            group: groups::SALES_REVIEW,
            description: "作废销售变更单",
        },
    ),
];
