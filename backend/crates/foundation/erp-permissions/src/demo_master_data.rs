//! demo_master_data 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查看演示主数据。
pub const STATUS: Permission = Permission::new(resources::DEMO_MASTER_DATA, "read");

/// 准备岗位账号、审批流程和默认责任规则。
pub const FOUNDATION: Permission = APPLY;

/// 生成演示主数据。
pub const APPLY: Permission = Permission::new(resources::DEMO_MASTER_DATA, "apply");

/// 清空演示数据库（仅保留 admin）。
pub const REMOVE: Permission = Permission::new(resources::DEMO_MASTER_DATA, "remove");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "STATUS",
        PermissionDef {
            permission: &STATUS, group: groups::DEMO_MASTER_DATA, description: "查看演示主数据"
        },
    ),
    (
        "APPLY",
        PermissionDef {
            permission: &APPLY, group: groups::DEMO_MASTER_DATA, description: "生成演示主数据"
        },
    ),
    (
        "REMOVE",
        PermissionDef {
            permission: &REMOVE,
            group: groups::DEMO_MASTER_DATA,
            description: "清空演示数据库（仅保留 admin）",
        },
    ),
];
