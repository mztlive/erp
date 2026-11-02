//! sales_selection 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询选品册列表。
pub const BOOKLET_LIST: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:list",
    group: groups::SALES_SELECTION,
    description: "查询选品册列表",
};

/// 查询选品册详情。
pub const BOOKLET_DETAIL: PermissionDef = ADMIN_IMAGE;

/// 创建选品册。
pub const BOOKLET_CREATE: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:create",
    group: groups::SALES_SELECTION,
    description: "创建选品册",
};

/// 准备及重生成选品册。
pub const BOOKLET_PREPARE: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:prepare",
    group: groups::SALES_SELECTION,
    description: "准备及重生成选品册",
};

/// 维护选品册陈列。
pub const BOOKLET_DELETE_ITEM: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:maintain",
    group: groups::SALES_SELECTION,
    description: "维护选品册陈列",
};

/// 维护选品册陈列。
pub const BOOKLET_DELETE_ITEM_POST: PermissionDef = BOOKLET_DELETE_ITEM;

/// 发布选品册。
pub const BOOKLET_PUBLISH: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:publish",
    group: groups::SALES_SELECTION,
    description: "发布选品册",
};

/// 复制选品链接。
pub const BOOKLET_COPY_LINK: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:copy_link",
    group: groups::SALES_SELECTION,
    description: "复制选品链接",
};

/// 复制选品链接。
pub const BOOKLET_COPY_LINK_URL: PermissionDef = BOOKLET_COPY_LINK;

/// 查询选品册详情。
pub const BOOKLET_SESSION: PermissionDef = ADMIN_IMAGE;

/// 更换选品链接。
pub const BOOKLET_ROTATE_LINK: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:rotate_link",
    group: groups::SALES_SELECTION,
    description: "更换选品链接",
};

/// 关闭或撤销选品链接。
pub const BOOKLET_CLOSE: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:close",
    group: groups::SALES_SELECTION,
    description: "关闭或撤销选品链接",
};

/// 关闭或撤销选品链接。
pub const BOOKLET_REVOKE: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:revoke",
    group: groups::SALES_SELECTION,
    description: "关闭或撤销选品链接",
};

/// 发布前作废选品册。
pub const BOOKLET_VOID: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:void",
    group: groups::SALES_SELECTION,
    description: "发布前作废选品册",
};

/// 查询销售方案列表。
pub const PROPOSAL_LIST: PermissionDef = PermissionDef {
    code: "sales_selection_proposal:list",
    group: groups::SALES_SELECTION,
    description: "查询销售方案列表",
};

/// 查询销售方案详情。
pub const PROPOSAL_DETAIL: PermissionDef = PermissionDef {
    code: "sales_selection_proposal:get",
    group: groups::SALES_SELECTION,
    description: "查询销售方案详情",
};

/// 查询选品册详情。
pub const ADMIN_IMAGE: PermissionDef = PermissionDef {
    code: "sales_selection_booklet:get",
    group: groups::SALES_SELECTION,
    description: "查询选品册详情",
};

/// 维护选品册访问密码。
pub const PROTECTION_BOOKLET_PASSWORD: PermissionDef =
    BOOKLET_DELETE_ITEM.described(groups::SALES_SELECTION, "维护选品册访问密码");

/// 读取选品册个人提货码。
pub const PROTECTION_BOOKLET_VOUCHERS: PermissionDef =
    BOOKLET_COPY_LINK.described(groups::SALES_SELECTION, "读取选品册个人提货码");

/// 查询选品册个人选品明细。
pub const PROTECTION_BOOKLET_SELECTION_DETAILS: PermissionDef =
    ADMIN_IMAGE.described(groups::SALES_SELECTION, "查询选品册个人选品明细");

/// 复制选品册复用创建权限，原册读取范围由用例重验。
pub const BOOKLET_COPY: PermissionDef = BOOKLET_CREATE.described(groups::SALES_SELECTION, "复制选品册");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("BOOKLET_COPY", BOOKLET_COPY),
    ("BOOKLET_LIST", BOOKLET_LIST),
    ("BOOKLET_DETAIL", BOOKLET_DETAIL),
    ("BOOKLET_CREATE", BOOKLET_CREATE),
    ("BOOKLET_PREPARE", BOOKLET_PREPARE),
    ("BOOKLET_DELETE_ITEM", BOOKLET_DELETE_ITEM),
    ("BOOKLET_DELETE_ITEM_POST", BOOKLET_DELETE_ITEM_POST),
    ("BOOKLET_PUBLISH", BOOKLET_PUBLISH),
    ("BOOKLET_COPY_LINK", BOOKLET_COPY_LINK),
    ("BOOKLET_COPY_LINK_URL", BOOKLET_COPY_LINK_URL),
    ("BOOKLET_SESSION", BOOKLET_SESSION),
    ("BOOKLET_ROTATE_LINK", BOOKLET_ROTATE_LINK),
    ("BOOKLET_CLOSE", BOOKLET_CLOSE),
    ("BOOKLET_REVOKE", BOOKLET_REVOKE),
    ("BOOKLET_VOID", BOOKLET_VOID),
    ("PROPOSAL_LIST", PROPOSAL_LIST),
    ("PROPOSAL_DETAIL", PROPOSAL_DETAIL),
    ("ADMIN_IMAGE", ADMIN_IMAGE),
    ("PROTECTION_BOOKLET_PASSWORD", PROTECTION_BOOKLET_PASSWORD),
    ("PROTECTION_BOOKLET_VOUCHERS", PROTECTION_BOOKLET_VOUCHERS),
    ("PROTECTION_BOOKLET_SELECTION_DETAILS", PROTECTION_BOOKLET_SELECTION_DETAILS),
];
