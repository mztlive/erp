//! supplier_offering 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 保存供应商供给条款。
pub const BATCH_REVISE: Permission = REVISE;

/// 更新供应商可供状态。
pub const BATCH_AVAILABILITY: Permission =
    Permission::new(resources::SUPPLIER_OFFERING_AVAILABILITY, "update");

/// 查询供应商配送模板。
pub const DELIVERY_TEMPLATE_LIST: Permission = LIST;

/// 新建供应商配送模板。
pub const DELIVERY_TEMPLATE_CREATE: Permission = REVISE;

/// 保存供应商配送模板。
pub const DELIVERY_TEMPLATE_UPDATE: Permission = REVISE;

/// 删除供应商配送模板。
pub const DELIVERY_TEMPLATE_DELETE: Permission = REVISE;

/// 为供给指定配送模板。
pub const DELIVERY_TEMPLATE_ASSIGN: Permission = REVISE;

/// 查询供应商供给列表。
pub const LIST: Permission = Permission::new(resources::SUPPLIER_OFFERING, "list");

/// 查询供应商供给详情。
pub const DETAIL: Permission = Permission::new(resources::SUPPLIER_OFFERING, "detail");

/// 新增供应商供给。
pub const CREATE: Permission = Permission::new(resources::SUPPLIER_OFFERING, "create");

/// 保存供应商供给条款。
pub const REVISE: Permission = Permission::new(resources::SUPPLIER_OFFERING, "update");

/// 更新供应商可供状态。
pub const UPDATE_AVAILABILITY: Permission = BATCH_AVAILABILITY;

/// 完成供应停止影响核对任务。
pub const COMPLETE_SUPPLY_EXCEPTION_TASK: Permission =
    Permission::new(resources::SUPPLIER_OFFERING, "resolve_supply_exception");

/// 交接供给维护人。
pub const OFFERING_HANDOVER: Permission = REVISE;

/// 查询供给交接候选。
pub const OFFERING_HANDOVER_CANDIDATES: Permission = REVISE;

/// 查询供应商供给详情。
pub const READ_DETAIL: Permission = DETAIL;

/// 查询供应商供给列表。
pub const READ_HISTORY: Permission = LIST;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "BATCH_AVAILABILITY",
        PermissionDef {
            permission: &BATCH_AVAILABILITY,
            group: groups::SUPPLIER_OFFERING,
            description: "更新供应商可供状态",
        },
    ),
    (
        "LIST",
        PermissionDef {
            permission: &LIST,
            group: groups::SUPPLIER_OFFERING,
            description: "查询供应商供给列表",
        },
    ),
    (
        "DETAIL",
        PermissionDef {
            permission: &DETAIL,
            group: groups::SUPPLIER_OFFERING,
            description: "查询供应商供给详情",
        },
    ),
    (
        "CREATE",
        PermissionDef {
            permission: &CREATE,
            group: groups::SUPPLIER_OFFERING,
            description: "新增供应商供给",
        },
    ),
    (
        "REVISE",
        PermissionDef {
            permission: &REVISE,
            group: groups::SUPPLIER_OFFERING,
            description: "保存供应商供给条款",
        },
    ),
    (
        "COMPLETE_SUPPLY_EXCEPTION_TASK",
        PermissionDef {
            permission: &COMPLETE_SUPPLY_EXCEPTION_TASK,
            group: groups::SUPPLIER_OFFERING,
            description: "完成供应停止影响核对任务",
        },
    ),
];
