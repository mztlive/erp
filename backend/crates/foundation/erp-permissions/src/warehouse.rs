//! warehouse 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询仓库目录。
pub const DIRECTORY_LIST: Permission = LIST;

/// 回显warehouse目录。
pub const DIRECTORY_SELECTED: Permission = LIST;

/// 查询仓库列表。
pub const LIST: Permission = Permission::new(resources::WAREHOUSE, "list");

/// 创建仓库。
pub const CREATE: Permission = Permission::new(resources::WAREHOUSE, "create");

/// 更新仓库。
pub const UPDATE: Permission = Permission::new(resources::WAREHOUSE, "update");

/// 配置仓库收发经办人。
pub const FULFILLMENT_HANDLERS_UPDATE: Permission = UPDATE;

/// 查询仓库收发经办人选项。
pub const FULFILLMENT_HANDLER_OPTIONS: Permission = UPDATE;

/// 查询仓库修订列表。
pub const REVISION_LIST: Permission = Permission::new(resources::WAREHOUSE_REVISION, "list");

/// 查询仓库SKU预警策略列表。
pub const SKU_POLICY_LIST: Permission = Permission::new(resources::WAREHOUSE_SKU_POLICY, "list");

/// 创建仓库SKU预警策略。
pub const SKU_POLICY_CREATE: Permission = Permission::new(resources::WAREHOUSE_SKU_POLICY, "create");

/// 更新仓库SKU预警策略。
pub const SKU_POLICY_UPDATE: Permission = Permission::new(resources::WAREHOUSE_SKU_POLICY, "update");

/// 删除仓库SKU预警策略。
pub const SKU_POLICY_DELETE: Permission = Permission::new(resources::WAREHOUSE_SKU_POLICY, "delete");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("LIST", PermissionDef { permission: &LIST, group: groups::CATALOG, description: "查询仓库列表" }),
    ("CREATE", PermissionDef { permission: &CREATE, group: groups::CATALOG, description: "创建仓库" }),
    ("UPDATE", PermissionDef { permission: &UPDATE, group: groups::CATALOG, description: "更新仓库" }),
    (
        "REVISION_LIST",
        PermissionDef {
            permission: &REVISION_LIST, group: groups::CATALOG, description: "查询仓库修订列表"
        },
    ),
    (
        "SKU_POLICY_LIST",
        PermissionDef {
            permission: &SKU_POLICY_LIST,
            group: groups::CATALOG,
            description: "查询仓库SKU预警策略列表",
        },
    ),
    (
        "SKU_POLICY_CREATE",
        PermissionDef {
            permission: &SKU_POLICY_CREATE,
            group: groups::CATALOG,
            description: "创建仓库SKU预警策略",
        },
    ),
    (
        "SKU_POLICY_UPDATE",
        PermissionDef {
            permission: &SKU_POLICY_UPDATE,
            group: groups::CATALOG,
            description: "更新仓库SKU预警策略",
        },
    ),
    (
        "SKU_POLICY_DELETE",
        PermissionDef {
            permission: &SKU_POLICY_DELETE,
            group: groups::CATALOG,
            description: "删除仓库SKU预警策略",
        },
    ),
];
