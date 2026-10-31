//! organization 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询组织。
pub const LIST: PermissionDef =
    PermissionDef { code: "org_unit:list", group: groups::ORGANIZATION, description: "查询组织" };

/// 管理组织。
pub const PREVIEW: PermissionDef = CHANGE;

/// 管理组织。
pub const CHANGE: PermissionDef =
    PermissionDef { code: "org_unit:manage", group: groups::ORGANIZATION, description: "管理组织" };

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[("LIST", LIST), ("PREVIEW", PREVIEW), ("CHANGE", CHANGE)];
