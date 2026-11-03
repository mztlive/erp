//! supplier_portal 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询供应商门户账号。
pub const ADMIN_ACCOUNTS: PermissionDef = PermissionDef {
    code: "supplier_portal_account:list",
    group: groups::SUPPLIER_PORTAL,
    description: "查询供应商门户账号",
};

/// 开通供应商门户账号。
pub const ADMIN_CREATE_ACCOUNT: PermissionDef = PermissionDef {
    code: "supplier_portal_account:create",
    group: groups::SUPPLIER_PORTAL,
    description: "开通供应商门户账号",
};

/// 管理供应商门户账号。
pub const ADMIN_UPDATE_ACCOUNT: PermissionDef = PermissionDef {
    code: "supplier_portal_account:update",
    group: groups::SUPPLIER_PORTAL,
    description: "管理供应商门户账号",
};

/// 查询供应商定向报价目录。
pub const ADMIN_GRANTS: PermissionDef = PermissionDef {
    code: "supplier_portal_catalog:list",
    group: groups::SUPPLIER_PORTAL,
    description: "查询供应商定向报价目录",
};

/// 维护供应商定向报价目录。
pub const ADMIN_UPDATE_GRANT: PermissionDef = PermissionDef {
    code: "supplier_portal_catalog:update",
    group: groups::SUPPLIER_PORTAL,
    description: "维护供应商定向报价目录",
};

/// 查询供应商门户申请。
pub const ADMIN_APPLICATIONS: PermissionDef = PermissionDef {
    code: "supplier_portal_request:list",
    group: groups::SUPPLIER_PORTAL,
    description: "查询供应商门户申请",
};

/// 查看供应商门户申请。
pub const ADMIN_APPLICATION: PermissionDef =
    ADMIN_IMPACTS.described(groups::SUPPLIER_PORTAL, "查看供应商门户申请");

/// 确认供应商门户申请。
pub const ADMIN_REVIEW: PermissionDef = PermissionDef {
    code: "supplier_portal_request:review",
    group: groups::SUPPLIER_PORTAL,
    description: "确认供应商门户申请",
};

/// 查询供应商新品审核字典。
pub const ADMIN_DICTIONARIES: PermissionDef =
    ADMIN_REVIEW.described(groups::SUPPLIER_PORTAL, "查询供应商新品审核字典");

/// 匹配供应商新品已有商品。
pub const ADMIN_DUPLICATES: PermissionDef =
    ADMIN_REVIEW.described(groups::SUPPLIER_PORTAL, "匹配供应商新品已有商品");

/// 核对供给未完成履约影响。
pub const ADMIN_IMPACTS: PermissionDef = PermissionDef {
    code: "supplier_portal_request:detail",
    group: groups::SUPPLIER_PORTAL,
    description: "核对供给未完成履约影响",
};

/// 下载供应商专项审核素材。
pub const ASSETS_REVIEW_DOWNLOAD: PermissionDef =
    ADMIN_IMPACTS.described(groups::SUPPLIER_PORTAL, "下载供应商专项审核素材");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("ADMIN_ACCOUNTS", ADMIN_ACCOUNTS),
    ("ADMIN_CREATE_ACCOUNT", ADMIN_CREATE_ACCOUNT),
    ("ADMIN_UPDATE_ACCOUNT", ADMIN_UPDATE_ACCOUNT),
    ("ADMIN_GRANTS", ADMIN_GRANTS),
    ("ADMIN_UPDATE_GRANT", ADMIN_UPDATE_GRANT),
    ("ADMIN_APPLICATIONS", ADMIN_APPLICATIONS),
    ("ADMIN_APPLICATION", ADMIN_APPLICATION),
    ("ADMIN_REVIEW", ADMIN_REVIEW),
    ("ADMIN_DICTIONARIES", ADMIN_DICTIONARIES),
    ("ADMIN_DUPLICATES", ADMIN_DUPLICATES),
    ("ADMIN_IMPACTS", ADMIN_IMPACTS),
    ("ASSETS_REVIEW_DOWNLOAD", ASSETS_REVIEW_DOWNLOAD),
];
