//! supplier_portal 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询供应商门户账号。
pub const ADMIN_ACCOUNTS: Permission = Permission::new(resources::SUPPLIER_PORTAL_ACCOUNT, "list");

/// 开通供应商门户账号。
pub const ADMIN_CREATE_ACCOUNT: Permission = Permission::new(resources::SUPPLIER_PORTAL_ACCOUNT, "create");

/// 管理供应商门户账号。
pub const ADMIN_UPDATE_ACCOUNT: Permission = Permission::new(resources::SUPPLIER_PORTAL_ACCOUNT, "update");

/// 查询供应商定向报价目录。
pub const ADMIN_GRANTS: Permission = Permission::new(resources::SUPPLIER_PORTAL_CATALOG, "list");

/// 维护供应商定向报价目录。
pub const ADMIN_UPDATE_GRANT: Permission = Permission::new(resources::SUPPLIER_PORTAL_CATALOG, "update");

/// 查询供应商门户申请。
pub const ADMIN_APPLICATIONS: Permission = Permission::new(resources::SUPPLIER_PORTAL_REQUEST, "list");

/// 查看供应商门户申请。
pub const ADMIN_APPLICATION: Permission = ADMIN_IMPACTS;

/// 确认供应商门户申请。
pub const ADMIN_REVIEW: Permission = Permission::new(resources::SUPPLIER_PORTAL_REQUEST, "review");

/// 查询供应商新品审核字典。
pub const ADMIN_DICTIONARIES: Permission = ADMIN_REVIEW;

/// 匹配供应商新品已有商品。
pub const ADMIN_DUPLICATES: Permission = ADMIN_REVIEW;

/// 核对供给未完成履约影响。
pub const ADMIN_IMPACTS: Permission = Permission::new(resources::SUPPLIER_PORTAL_REQUEST, "detail");

/// 下载供应商专项审核素材。
pub const ASSETS_REVIEW_DOWNLOAD: Permission = ADMIN_IMPACTS;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "ADMIN_ACCOUNTS",
        PermissionDef {
            permission: &ADMIN_ACCOUNTS,
            group: groups::SUPPLIER_PORTAL,
            description: "查询供应商门户账号",
        },
    ),
    (
        "ADMIN_CREATE_ACCOUNT",
        PermissionDef {
            permission: &ADMIN_CREATE_ACCOUNT,
            group: groups::SUPPLIER_PORTAL,
            description: "开通供应商门户账号",
        },
    ),
    (
        "ADMIN_UPDATE_ACCOUNT",
        PermissionDef {
            permission: &ADMIN_UPDATE_ACCOUNT,
            group: groups::SUPPLIER_PORTAL,
            description: "管理供应商门户账号",
        },
    ),
    (
        "ADMIN_GRANTS",
        PermissionDef {
            permission: &ADMIN_GRANTS,
            group: groups::SUPPLIER_PORTAL,
            description: "查询供应商定向报价目录",
        },
    ),
    (
        "ADMIN_UPDATE_GRANT",
        PermissionDef {
            permission: &ADMIN_UPDATE_GRANT,
            group: groups::SUPPLIER_PORTAL,
            description: "维护供应商定向报价目录",
        },
    ),
    (
        "ADMIN_APPLICATIONS",
        PermissionDef {
            permission: &ADMIN_APPLICATIONS,
            group: groups::SUPPLIER_PORTAL,
            description: "查询供应商门户申请",
        },
    ),
    (
        "ADMIN_REVIEW",
        PermissionDef {
            permission: &ADMIN_REVIEW,
            group: groups::SUPPLIER_PORTAL,
            description: "确认供应商门户申请",
        },
    ),
    (
        "ADMIN_IMPACTS",
        PermissionDef {
            permission: &ADMIN_IMPACTS,
            group: groups::SUPPLIER_PORTAL,
            description: "核对供给未完成履约影响",
        },
    ),
];
