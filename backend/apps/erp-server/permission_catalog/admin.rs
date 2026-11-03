//! admin 的接口权限目录。
use super::{PermissionDef, groups};

/// 创建管理员。
pub const ACCOUNT_CREATE_ADMIN: PermissionDef =
    PermissionDef { code: "admin:create", group: groups::ADMIN, description: "创建管理员" };

/// 查询管理员列表。
pub const ACCOUNT_LIST_ADMINS: PermissionDef =
    PermissionDef { code: "admin:list", group: groups::ADMIN, description: "查询管理员列表" };

/// 更新管理员信息。
pub const ACCOUNT_UPDATE_ADMIN: PermissionDef =
    PermissionDef { code: "admin:update", group: groups::ADMIN, description: "更新管理员信息" };

/// 删除管理员。
pub const ACCOUNT_DELETE_ADMIN: PermissionDef =
    PermissionDef { code: "admin:delete", group: groups::ADMIN, description: "删除管理员" };

/// 更新管理员角色。
pub const ACCOUNT_UPDATE_ADMIN_ROLE: PermissionDef =
    PermissionDef { code: "admin:update_role", group: groups::ADMIN, description: "更新管理员角色" };

/// 查询审计日志。
pub const AUDIT_LOG_LIST_AUDIT_LOGS: PermissionDef =
    PermissionDef { code: "audit_log:list", group: groups::AUDIT_LOG, description: "查询审计日志" };

/// 查询采购责任规则。
pub const PROCUREMENT_RESPONSIBILITY_LIST_RULES: PermissionDef =
    PROCUREMENT_RESPONSIBILITY_RESOLVE.described(groups::PROCUREMENT_RESPONSIBILITY, "查询采购责任规则");

/// 创建采购责任规则。
pub const PROCUREMENT_RESPONSIBILITY_CREATE_RULE: PermissionDef = PermissionDef {
    code: "procurement_responsibility:manage",
    group: groups::PROCUREMENT_RESPONSIBILITY,
    description: "创建采购责任规则",
};

/// 更新采购责任规则。
pub const PROCUREMENT_RESPONSIBILITY_UPDATE_RULE: PermissionDef =
    PROCUREMENT_RESPONSIBILITY_CREATE_RULE.described(groups::PROCUREMENT_RESPONSIBILITY, "更新采购责任规则");

/// 预览采购责任解析。
pub const PROCUREMENT_RESPONSIBILITY_RESOLVE: PermissionDef = PermissionDef {
    code: "procurement_responsibility:list",
    group: groups::PROCUREMENT_RESPONSIBILITY,
    description: "预览采购责任解析",
};

/// 创建角色。
pub const ROLE_CREATE_ROLE: PermissionDef =
    PermissionDef { code: "role:create", group: groups::ROLE, description: "创建角色" };

/// 查询角色列表。
pub const ROLE_LIST_ROLES: PermissionDef =
    PermissionDef { code: "role:list", group: groups::ROLE, description: "查询角色列表" };

/// 查询可分配角色。
pub const ROLE_LIST_ASSIGNABLE_ROLES: PermissionDef =
    PermissionDef { code: "role:assignable_list", group: groups::ROLE, description: "查询可分配角色" };

/// 更新角色信息。
pub const ROLE_UPDATE_ROLE: PermissionDef =
    PermissionDef { code: "role:update", group: groups::ROLE, description: "更新角色信息" };

/// 删除角色。
pub const ROLE_DELETE_ROLE: PermissionDef =
    PermissionDef { code: "role:delete", group: groups::ROLE, description: "删除角色" };

/// 查询角色列表。
pub const ROLE_TEMPLATE_LIST_ROLE_TEMPLATES: PermissionDef = ROLE_LIST_ROLES;

/// 创建角色。
pub const ROLE_TEMPLATE_GENERATE_BUILTIN_ROLES: PermissionDef = ROLE_CREATE_ROLE;

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("ACCOUNT_CREATE_ADMIN", ACCOUNT_CREATE_ADMIN),
    ("ACCOUNT_LIST_ADMINS", ACCOUNT_LIST_ADMINS),
    ("ACCOUNT_UPDATE_ADMIN", ACCOUNT_UPDATE_ADMIN),
    ("ACCOUNT_DELETE_ADMIN", ACCOUNT_DELETE_ADMIN),
    ("ACCOUNT_UPDATE_ADMIN_ROLE", ACCOUNT_UPDATE_ADMIN_ROLE),
    ("AUDIT_LOG_LIST_AUDIT_LOGS", AUDIT_LOG_LIST_AUDIT_LOGS),
    ("PROCUREMENT_RESPONSIBILITY_LIST_RULES", PROCUREMENT_RESPONSIBILITY_LIST_RULES),
    ("PROCUREMENT_RESPONSIBILITY_CREATE_RULE", PROCUREMENT_RESPONSIBILITY_CREATE_RULE),
    ("PROCUREMENT_RESPONSIBILITY_UPDATE_RULE", PROCUREMENT_RESPONSIBILITY_UPDATE_RULE),
    ("PROCUREMENT_RESPONSIBILITY_RESOLVE", PROCUREMENT_RESPONSIBILITY_RESOLVE),
    ("ROLE_CREATE_ROLE", ROLE_CREATE_ROLE),
    ("ROLE_LIST_ROLES", ROLE_LIST_ROLES),
    ("ROLE_LIST_ASSIGNABLE_ROLES", ROLE_LIST_ASSIGNABLE_ROLES),
    ("ROLE_UPDATE_ROLE", ROLE_UPDATE_ROLE),
    ("ROLE_DELETE_ROLE", ROLE_DELETE_ROLE),
    ("ROLE_TEMPLATE_LIST_ROLE_TEMPLATES", ROLE_TEMPLATE_LIST_ROLE_TEMPLATES),
    ("ROLE_TEMPLATE_GENERATE_BUILTIN_ROLES", ROLE_TEMPLATE_GENERATE_BUILTIN_ROLES),
];
