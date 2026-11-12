//! organization 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询组织。
pub const LIST: Permission = Permission::new(resources::ORG_UNIT, "list");

/// 管理组织。
pub const PREVIEW: Permission = CHANGE;

/// 管理组织。
pub const CHANGE: Permission = Permission::new(resources::ORG_UNIT, "manage");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("LIST", PermissionDef { permission: &LIST, group: groups::ORGANIZATION, description: "查询组织" }),
    ("CHANGE", PermissionDef { permission: &CHANGE, group: groups::ORGANIZATION, description: "管理组织" }),
];
