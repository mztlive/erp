//! sales_selection 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询选品册列表。
pub const BOOKLET_LIST: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "list");

/// 查询选品册详情。
pub const BOOKLET_DETAIL: Permission = ADMIN_IMAGE;

/// 创建选品册。
pub const BOOKLET_CREATE: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "create");

/// 准备及重生成选品册。
pub const BOOKLET_PREPARE: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "prepare");

/// 维护选品册陈列。
pub const BOOKLET_DELETE_ITEM: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "maintain");

/// 维护选品册陈列。
pub const BOOKLET_DELETE_ITEM_POST: Permission = BOOKLET_DELETE_ITEM;

/// 调整选品册客户售价，复用陈列维护权限。
pub const BOOKLET_UPDATE_PRICES: Permission = BOOKLET_DELETE_ITEM;

/// 发布选品册。
pub const BOOKLET_PUBLISH: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "publish");

/// 复制选品链接。
pub const BOOKLET_COPY_LINK: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "copy_link");

/// 复制选品链接。
pub const BOOKLET_COPY_LINK_URL: Permission = BOOKLET_COPY_LINK;

/// 查询选品册详情。
pub const BOOKLET_SESSION: Permission = ADMIN_IMAGE;

/// 更换选品链接。
pub const BOOKLET_ROTATE_LINK: Permission =
    Permission::new(resources::SALES_SELECTION_BOOKLET, "rotate_link");

/// 关闭或撤销选品链接。
pub const BOOKLET_CLOSE: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "close");

/// 关闭或撤销选品链接。
pub const BOOKLET_REVOKE: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "revoke");

/// 发布前作废选品册。
pub const BOOKLET_VOID: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "void");

/// 查询销售方案列表。
pub const PROPOSAL_LIST: Permission = Permission::new(resources::SALES_SELECTION_PROPOSAL, "list");

/// 查询销售方案详情。
pub const PROPOSAL_DETAIL: Permission = Permission::new(resources::SALES_SELECTION_PROPOSAL, "get");

/// 查询选品册详情。
pub const ADMIN_IMAGE: Permission = Permission::new(resources::SALES_SELECTION_BOOKLET, "get");

/// 维护选品册访问密码。
pub const PROTECTION_BOOKLET_PASSWORD: Permission = BOOKLET_DELETE_ITEM;

/// 复制选品册复用创建权限，原册读取范围由用例重验。
pub const BOOKLET_COPY: Permission = BOOKLET_CREATE;

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "BOOKLET_LIST",
        PermissionDef {
            permission: &BOOKLET_LIST,
            group: groups::SALES_SELECTION,
            description: "查询选品册列表",
        },
    ),
    (
        "BOOKLET_CREATE",
        PermissionDef {
            permission: &BOOKLET_CREATE,
            group: groups::SALES_SELECTION,
            description: "创建选品册",
        },
    ),
    (
        "BOOKLET_PREPARE",
        PermissionDef {
            permission: &BOOKLET_PREPARE,
            group: groups::SALES_SELECTION,
            description: "准备及重生成选品册",
        },
    ),
    (
        "BOOKLET_DELETE_ITEM",
        PermissionDef {
            permission: &BOOKLET_DELETE_ITEM,
            group: groups::SALES_SELECTION,
            description: "维护选品册陈列",
        },
    ),
    (
        "BOOKLET_PUBLISH",
        PermissionDef {
            permission: &BOOKLET_PUBLISH,
            group: groups::SALES_SELECTION,
            description: "发布选品册",
        },
    ),
    (
        "BOOKLET_COPY_LINK",
        PermissionDef {
            permission: &BOOKLET_COPY_LINK,
            group: groups::SALES_SELECTION,
            description: "复制选品链接",
        },
    ),
    (
        "BOOKLET_ROTATE_LINK",
        PermissionDef {
            permission: &BOOKLET_ROTATE_LINK,
            group: groups::SALES_SELECTION,
            description: "更换选品链接",
        },
    ),
    (
        "BOOKLET_CLOSE",
        PermissionDef {
            permission: &BOOKLET_CLOSE,
            group: groups::SALES_SELECTION,
            description: "关闭或撤销选品链接",
        },
    ),
    (
        "BOOKLET_REVOKE",
        PermissionDef {
            permission: &BOOKLET_REVOKE,
            group: groups::SALES_SELECTION,
            description: "关闭或撤销选品链接",
        },
    ),
    (
        "BOOKLET_VOID",
        PermissionDef {
            permission: &BOOKLET_VOID,
            group: groups::SALES_SELECTION,
            description: "发布前作废选品册",
        },
    ),
    (
        "PROPOSAL_LIST",
        PermissionDef {
            permission: &PROPOSAL_LIST,
            group: groups::SALES_SELECTION,
            description: "查询销售方案列表",
        },
    ),
    (
        "PROPOSAL_DETAIL",
        PermissionDef {
            permission: &PROPOSAL_DETAIL,
            group: groups::SALES_SELECTION,
            description: "查询销售方案详情",
        },
    ),
    (
        "ADMIN_IMAGE",
        PermissionDef {
            permission: &ADMIN_IMAGE,
            group: groups::SALES_SELECTION,
            description: "查询选品册详情",
        },
    ),
];
