//! warehouse 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询仓库目录。
pub const DIRECTORY_LIST: PermissionDef = LIST.described(groups::WAREHOUSE, "查询仓库目录");

/// 回显warehouse目录。
pub const DIRECTORY_SELECTED: PermissionDef = LIST.described(groups::WAREHOUSE, "回显warehouse目录");

/// 查询仓库列表。
pub const LIST: PermissionDef =
    PermissionDef { code: "warehouse:list", group: groups::CATALOG, description: "查询仓库列表" };

/// 创建仓库。
pub const CREATE: PermissionDef =
    PermissionDef { code: "warehouse:create", group: groups::CATALOG, description: "创建仓库" };

/// 更新仓库。
pub const UPDATE: PermissionDef =
    PermissionDef { code: "warehouse:update", group: groups::CATALOG, description: "更新仓库" };

/// 配置仓库收发经办人。
pub const FULFILLMENT_HANDLERS_UPDATE: PermissionDef =
    UPDATE.described(groups::CATALOG, "配置仓库收发经办人");

/// 查询仓库收发经办人选项。
pub const FULFILLMENT_HANDLER_OPTIONS: PermissionDef =
    UPDATE.described(groups::CATALOG, "查询仓库收发经办人选项");

/// 查询仓库修订列表。
pub const REVISION_LIST: PermissionDef = PermissionDef {
    code: "warehouse_revision:list",
    group: groups::CATALOG,
    description: "查询仓库修订列表",
};

/// 查询仓库SKU预警策略列表。
pub const SKU_POLICY_LIST: PermissionDef = PermissionDef {
    code: "warehouse_sku_policy:list",
    group: groups::CATALOG,
    description: "查询仓库SKU预警策略列表",
};

/// 创建仓库SKU预警策略。
pub const SKU_POLICY_CREATE: PermissionDef = PermissionDef {
    code: "warehouse_sku_policy:create",
    group: groups::CATALOG,
    description: "创建仓库SKU预警策略",
};

/// 更新仓库SKU预警策略。
pub const SKU_POLICY_UPDATE: PermissionDef = PermissionDef {
    code: "warehouse_sku_policy:update",
    group: groups::CATALOG,
    description: "更新仓库SKU预警策略",
};

/// 删除仓库SKU预警策略。
pub const SKU_POLICY_DELETE: PermissionDef = PermissionDef {
    code: "warehouse_sku_policy:delete",
    group: groups::CATALOG,
    description: "删除仓库SKU预警策略",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("DIRECTORY_LIST", DIRECTORY_LIST),
    ("DIRECTORY_SELECTED", DIRECTORY_SELECTED),
    ("LIST", LIST),
    ("CREATE", CREATE),
    ("UPDATE", UPDATE),
    ("FULFILLMENT_HANDLERS_UPDATE", FULFILLMENT_HANDLERS_UPDATE),
    ("FULFILLMENT_HANDLER_OPTIONS", FULFILLMENT_HANDLER_OPTIONS),
    ("REVISION_LIST", REVISION_LIST),
    ("SKU_POLICY_LIST", SKU_POLICY_LIST),
    ("SKU_POLICY_CREATE", SKU_POLICY_CREATE),
    ("SKU_POLICY_UPDATE", SKU_POLICY_UPDATE),
    ("SKU_POLICY_DELETE", SKU_POLICY_DELETE),
];
