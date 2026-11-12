//! source_registry 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询来源系统列表。
pub const SOURCE_SYSTEM_LIST: Permission = Permission::new(resources::SOURCE_SYSTEM, "list");

/// 创建来源系统。
pub const SOURCE_SYSTEM_CREATE: Permission = Permission::new(resources::SOURCE_SYSTEM, "create");

/// 更新来源系统。
pub const SOURCE_SYSTEM_UPDATE: Permission = Permission::new(resources::SOURCE_SYSTEM, "update");

/// 查询外部身份映射列表。
pub const EXTERNAL_IDENTITY_MAP_LIST: Permission = Permission::new(resources::EXTERNAL_IDENTITY_MAP, "list");

/// 建立外部身份映射。
pub const EXTERNAL_IDENTITY_MAP_CREATE: Permission =
    Permission::new(resources::EXTERNAL_IDENTITY_MAP, "create");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "SOURCE_SYSTEM_LIST",
        PermissionDef {
            permission: &SOURCE_SYSTEM_LIST,
            group: groups::SOURCE_REGISTRY,
            description: "查询来源系统列表",
        },
    ),
    (
        "SOURCE_SYSTEM_CREATE",
        PermissionDef {
            permission: &SOURCE_SYSTEM_CREATE,
            group: groups::SOURCE_REGISTRY,
            description: "创建来源系统",
        },
    ),
    (
        "SOURCE_SYSTEM_UPDATE",
        PermissionDef {
            permission: &SOURCE_SYSTEM_UPDATE,
            group: groups::SOURCE_REGISTRY,
            description: "更新来源系统",
        },
    ),
    (
        "EXTERNAL_IDENTITY_MAP_LIST",
        PermissionDef {
            permission: &EXTERNAL_IDENTITY_MAP_LIST,
            group: groups::SOURCE_REGISTRY,
            description: "查询外部身份映射列表",
        },
    ),
    (
        "EXTERNAL_IDENTITY_MAP_CREATE",
        PermissionDef {
            permission: &EXTERNAL_IDENTITY_MAP_CREATE,
            group: groups::SOURCE_REGISTRY,
            description: "建立外部身份映射",
        },
    ),
];
