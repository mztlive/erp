//! 公共权限目录与展示引用；不依赖 HTTP 或授权执行器。
use crate::Permission;

/// 权限展示分组。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionGroupDef {
    pub name: &'static str,
    pub description: &'static str,
}

/// 权限的公共分组与默认说明；接口差异由应用补充。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PermissionDef {
    pub permission: &'static Permission,
    pub group: PermissionGroupDef,
    pub description: &'static str,
}

/// 显式模块索引；符号名仅用于构建阶段解析路由引用。
#[derive(Debug, Clone, Copy)]
pub struct PermissionModule {
    pub name: &'static str,
    pub entries: &'static [(&'static str, PermissionDef)],
}
