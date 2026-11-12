//! admin 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 创建管理员。
pub const ACCOUNT_CREATE_ADMIN: Permission = Permission::new(resources::ADMIN, "create");

/// 查询管理员列表。
pub const ACCOUNT_LIST_ADMINS: Permission = Permission::new(resources::ADMIN, "list");

/// 更新管理员信息。
pub const ACCOUNT_UPDATE_ADMIN: Permission = Permission::new(resources::ADMIN, "update");

/// 删除管理员。
pub const ACCOUNT_DELETE_ADMIN: Permission = Permission::new(resources::ADMIN, "delete");

/// 更新管理员角色。
pub const ACCOUNT_UPDATE_ADMIN_ROLE: Permission = Permission::new(resources::ADMIN, "update_role");

/// 查询审计日志。
pub const AUDIT_LOG_LIST_AUDIT_LOGS: Permission = Permission::new(resources::AUDIT_LOG, "list");

/// 查询采购责任规则。
pub const PROCUREMENT_RESPONSIBILITY_LIST_RULES: Permission = PROCUREMENT_RESPONSIBILITY_RESOLVE;

/// 创建采购责任规则。
pub const PROCUREMENT_RESPONSIBILITY_CREATE_RULE: Permission =
    Permission::new(resources::PROCUREMENT_RESPONSIBILITY, "manage");

/// 更新采购责任规则。
pub const PROCUREMENT_RESPONSIBILITY_UPDATE_RULE: Permission = PROCUREMENT_RESPONSIBILITY_CREATE_RULE;

/// 预览采购责任解析。
pub const PROCUREMENT_RESPONSIBILITY_RESOLVE: Permission =
    Permission::new(resources::PROCUREMENT_RESPONSIBILITY, "list");

/// 创建角色。
pub const ROLE_CREATE_ROLE: Permission = Permission::new(resources::ROLE, "create");

/// 查询角色列表。
pub const ROLE_LIST_ROLES: Permission = Permission::new(resources::ROLE, "list");

/// 查询可分配角色。
pub const ROLE_LIST_ASSIGNABLE_ROLES: Permission = Permission::new(resources::ROLE, "assignable_list");

/// 更新角色信息。
pub const ROLE_UPDATE_ROLE: Permission = Permission::new(resources::ROLE, "update");

/// 删除角色。
pub const ROLE_DELETE_ROLE: Permission = Permission::new(resources::ROLE, "delete");

/// 查询角色列表。
pub const ROLE_TEMPLATE_LIST_ROLE_TEMPLATES: Permission = ROLE_LIST_ROLES;

/// 创建角色。
pub const ROLE_TEMPLATE_GENERATE_BUILTIN_ROLES: Permission = ROLE_CREATE_ROLE;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "ACCOUNT_CREATE_ADMIN",
        PermissionDef {
            permission: &ACCOUNT_CREATE_ADMIN, group: groups::ADMIN, description: "创建管理员"
        },
    ),
    (
        "ACCOUNT_LIST_ADMINS",
        PermissionDef {
            permission: &ACCOUNT_LIST_ADMINS,
            group: groups::ADMIN,
            description: "查询管理员列表",
        },
    ),
    (
        "ACCOUNT_UPDATE_ADMIN",
        PermissionDef {
            permission: &ACCOUNT_UPDATE_ADMIN,
            group: groups::ADMIN,
            description: "更新管理员信息",
        },
    ),
    (
        "ACCOUNT_DELETE_ADMIN",
        PermissionDef {
            permission: &ACCOUNT_DELETE_ADMIN, group: groups::ADMIN, description: "删除管理员"
        },
    ),
    (
        "ACCOUNT_UPDATE_ADMIN_ROLE",
        PermissionDef {
            permission: &ACCOUNT_UPDATE_ADMIN_ROLE,
            group: groups::ADMIN,
            description: "更新管理员角色",
        },
    ),
    (
        "AUDIT_LOG_LIST_AUDIT_LOGS",
        PermissionDef {
            permission: &AUDIT_LOG_LIST_AUDIT_LOGS,
            group: groups::AUDIT_LOG,
            description: "查询审计日志",
        },
    ),
    (
        "PROCUREMENT_RESPONSIBILITY_CREATE_RULE",
        PermissionDef {
            permission: &PROCUREMENT_RESPONSIBILITY_CREATE_RULE,
            group: groups::PROCUREMENT_RESPONSIBILITY,
            description: "创建采购责任规则",
        },
    ),
    (
        "PROCUREMENT_RESPONSIBILITY_RESOLVE",
        PermissionDef {
            permission: &PROCUREMENT_RESPONSIBILITY_RESOLVE,
            group: groups::PROCUREMENT_RESPONSIBILITY,
            description: "预览采购责任解析",
        },
    ),
    (
        "ROLE_CREATE_ROLE",
        PermissionDef { permission: &ROLE_CREATE_ROLE, group: groups::ROLE, description: "创建角色" },
    ),
    (
        "ROLE_LIST_ROLES",
        PermissionDef {
            permission: &ROLE_LIST_ROLES, group: groups::ROLE, description: "查询角色列表"
        },
    ),
    (
        "ROLE_LIST_ASSIGNABLE_ROLES",
        PermissionDef {
            permission: &ROLE_LIST_ASSIGNABLE_ROLES,
            group: groups::ROLE,
            description: "查询可分配角色",
        },
    ),
    (
        "ROLE_UPDATE_ROLE",
        PermissionDef {
            permission: &ROLE_UPDATE_ROLE, group: groups::ROLE, description: "更新角色信息"
        },
    ),
    (
        "ROLE_DELETE_ROLE",
        PermissionDef { permission: &ROLE_DELETE_ROLE, group: groups::ROLE, description: "删除角色" },
    ),
];
