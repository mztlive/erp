//! demo_master_data 的接口权限目录。
use super::{PermissionDef, groups};

/// 查看演示主数据。
pub const STATUS: PermissionDef = PermissionDef {
    code: "demo_master_data:read",
    group: groups::DEMO_MASTER_DATA,
    description: "查看演示主数据",
};

/// 准备岗位账号、审批流程和默认责任规则。
pub const FOUNDATION: PermissionDef =
    APPLY.described(groups::DEMO_MASTER_DATA, "准备岗位账号、审批流程和默认责任规则");

/// 生成演示主数据。
pub const APPLY: PermissionDef = PermissionDef {
    code: "demo_master_data:apply",
    group: groups::DEMO_MASTER_DATA,
    description: "生成演示主数据",
};

/// 清空演示数据库（仅保留 admin）。
pub const REMOVE: PermissionDef = PermissionDef {
    code: "demo_master_data:remove",
    group: groups::DEMO_MASTER_DATA,
    description: "清空演示数据库（仅保留 admin）",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] =
    &[("STATUS", STATUS), ("FOUNDATION", FOUNDATION), ("APPLY", APPLY), ("REMOVE", REMOVE)];
