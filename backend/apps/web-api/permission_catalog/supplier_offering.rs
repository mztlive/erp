//! supplier_offering 的接口权限目录。
use super::{PermissionDef, groups};

/// 保存供应商供给条款。
pub const BATCH_REVISE: PermissionDef = REVISE;

/// 更新供应商可供状态。
pub const BATCH_AVAILABILITY: PermissionDef = PermissionDef {
    code: "supplier_offering_availability:update",
    group: groups::SUPPLIER_OFFERING,
    description: "更新供应商可供状态",
};

/// 查询供应商配送模板。
pub const DELIVERY_TEMPLATE_LIST: PermissionDef =
    LIST.described(groups::SUPPLIER_OFFERING, "查询供应商配送模板");

/// 新建供应商配送模板。
pub const DELIVERY_TEMPLATE_CREATE: PermissionDef =
    REVISE.described(groups::SUPPLIER_OFFERING, "新建供应商配送模板");

/// 保存供应商配送模板。
pub const DELIVERY_TEMPLATE_UPDATE: PermissionDef =
    REVISE.described(groups::SUPPLIER_OFFERING, "保存供应商配送模板");

/// 删除供应商配送模板。
pub const DELIVERY_TEMPLATE_DELETE: PermissionDef =
    REVISE.described(groups::SUPPLIER_OFFERING, "删除供应商配送模板");

/// 为供给指定配送模板。
pub const DELIVERY_TEMPLATE_ASSIGN: PermissionDef =
    REVISE.described(groups::SUPPLIER_OFFERING, "为供给指定配送模板");

/// 查询供应商供给列表。
pub const LIST: PermissionDef = PermissionDef {
    code: "supplier_offering:list",
    group: groups::SUPPLIER_OFFERING,
    description: "查询供应商供给列表",
};

/// 新增供应商供给。
pub const CREATE: PermissionDef = PermissionDef {
    code: "supplier_offering:create",
    group: groups::SUPPLIER_OFFERING,
    description: "新增供应商供给",
};

/// 保存供应商供给条款。
pub const REVISE: PermissionDef = PermissionDef {
    code: "supplier_offering:update",
    group: groups::SUPPLIER_OFFERING,
    description: "保存供应商供给条款",
};

/// 更新供应商可供状态。
pub const UPDATE_AVAILABILITY: PermissionDef = BATCH_AVAILABILITY;

/// 完成供应停止影响核对任务。
pub const COMPLETE_SUPPLY_EXCEPTION_TASK: PermissionDef = PermissionDef {
    code: "supplier_offering:resolve_supply_exception",
    group: groups::SUPPLIER_OFFERING,
    description: "完成供应停止影响核对任务",
};

/// 交接供给维护人。
pub const OFFERING_HANDOVER: PermissionDef = REVISE.described(groups::SUPPLIER_OFFERING, "交接供给维护人");

/// 查询供给交接候选。
pub const OFFERING_HANDOVER_CANDIDATES: PermissionDef =
    REVISE.described(groups::SUPPLIER_OFFERING, "查询供给交接候选");

/// 查询供应商供给列表。
pub const READ_DETAIL: PermissionDef = LIST;

/// 查询供应商供给列表。
pub const READ_HISTORY: PermissionDef = LIST;

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("BATCH_REVISE", BATCH_REVISE),
    ("BATCH_AVAILABILITY", BATCH_AVAILABILITY),
    ("DELIVERY_TEMPLATE_LIST", DELIVERY_TEMPLATE_LIST),
    ("DELIVERY_TEMPLATE_CREATE", DELIVERY_TEMPLATE_CREATE),
    ("DELIVERY_TEMPLATE_UPDATE", DELIVERY_TEMPLATE_UPDATE),
    ("DELIVERY_TEMPLATE_DELETE", DELIVERY_TEMPLATE_DELETE),
    ("DELIVERY_TEMPLATE_ASSIGN", DELIVERY_TEMPLATE_ASSIGN),
    ("LIST", LIST),
    ("CREATE", CREATE),
    ("REVISE", REVISE),
    ("UPDATE_AVAILABILITY", UPDATE_AVAILABILITY),
    ("COMPLETE_SUPPLY_EXCEPTION_TASK", COMPLETE_SUPPLY_EXCEPTION_TASK),
    ("OFFERING_HANDOVER", OFFERING_HANDOVER),
    ("OFFERING_HANDOVER_CANDIDATES", OFFERING_HANDOVER_CANDIDATES),
    ("READ_DETAIL", READ_DETAIL),
    ("READ_HISTORY", READ_HISTORY),
];
