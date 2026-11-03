//! source_registry 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询来源系统列表。
pub const SOURCE_SYSTEM_LIST: PermissionDef = PermissionDef {
    code: "source_system:list",
    group: groups::SOURCE_REGISTRY,
    description: "查询来源系统列表",
};

/// 创建来源系统。
pub const SOURCE_SYSTEM_CREATE: PermissionDef = PermissionDef {
    code: "source_system:create",
    group: groups::SOURCE_REGISTRY,
    description: "创建来源系统",
};

/// 更新来源系统。
pub const SOURCE_SYSTEM_UPDATE: PermissionDef = PermissionDef {
    code: "source_system:update",
    group: groups::SOURCE_REGISTRY,
    description: "更新来源系统",
};

/// 查询外部身份映射列表。
pub const EXTERNAL_IDENTITY_MAP_LIST: PermissionDef = PermissionDef {
    code: "external_identity_map:list",
    group: groups::SOURCE_REGISTRY,
    description: "查询外部身份映射列表",
};

/// 建立外部身份映射。
pub const EXTERNAL_IDENTITY_MAP_CREATE: PermissionDef = PermissionDef {
    code: "external_identity_map:create",
    group: groups::SOURCE_REGISTRY,
    description: "建立外部身份映射",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("SOURCE_SYSTEM_LIST", SOURCE_SYSTEM_LIST),
    ("SOURCE_SYSTEM_CREATE", SOURCE_SYSTEM_CREATE),
    ("SOURCE_SYSTEM_UPDATE", SOURCE_SYSTEM_UPDATE),
    ("EXTERNAL_IDENTITY_MAP_LIST", EXTERNAL_IDENTITY_MAP_LIST),
    ("EXTERNAL_IDENTITY_MAP_CREATE", EXTERNAL_IDENTITY_MAP_CREATE),
];
