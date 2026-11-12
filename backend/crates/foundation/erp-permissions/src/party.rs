//! party 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询公司主体。
pub const COMPANY_COMPANY_LIST: Permission = Permission::new(resources::COMPANY, "list");

/// 查看公司主体。
pub const COMPANY_COMPANY_DETAIL: Permission = Permission::new(resources::COMPANY, "detail");

/// 新建公司主体。
pub const COMPANY_COMPANY_CREATE: Permission = Permission::new(resources::COMPANY, "create");

/// 维护公司主体。
pub const COMPANY_COMPANY_UPDATE: Permission = Permission::new(resources::COMPANY, "update");

/// 查询主体列表。
pub const LIST: Permission = Permission::new(resources::PARTY, "list");

/// 创建主体。
pub const CREATE: Permission = Permission::new(resources::PARTY, "create");

/// 查询主体详情。
pub const DETAIL: Permission = Permission::new(resources::PARTY, "detail");

/// 更新主体并形成新修订。
pub const UPDATE: Permission = Permission::new(resources::PARTY, "update");

/// 删除主体。
pub const DELETE: Permission = Permission::new(resources::PARTY, "delete");

/// 查询主体修订列表。
pub const REVISION_LIST: Permission = Permission::new(resources::PARTY_REVISION, "list");

/// 查询联系人列表。
pub const CONTACT_LIST: Permission = Permission::new(resources::PARTY_CONTACT, "list");

/// 创建联系人。
pub const CONTACT_CREATE: Permission = Permission::new(resources::PARTY_CONTACT, "create");

/// 更新联系人。
pub const CONTACT_UPDATE: Permission = Permission::new(resources::PARTY_CONTACT, "update");

/// 查询地址列表。
pub const ADDRESS_LIST: Permission = Permission::new(resources::PARTY_ADDRESS, "list");

/// 创建地址。
pub const ADDRESS_CREATE: Permission = Permission::new(resources::PARTY_ADDRESS, "create");

/// 更新地址。
pub const ADDRESS_UPDATE: Permission = Permission::new(resources::PARTY_ADDRESS, "update");

/// 查询税务资料列表。
pub const TAX_PROFILE_LIST: Permission = Permission::new(resources::PARTY_TAX_PROFILE, "list");

/// 创建税务资料。
pub const TAX_PROFILE_CREATE: Permission = Permission::new(resources::PARTY_TAX_PROFILE, "create");

/// 更新税务资料。
pub const TAX_PROFILE_UPDATE: Permission = Permission::new(resources::PARTY_TAX_PROFILE, "update");

/// 查询银行账户列表。
pub const BANK_ACCOUNT_LIST: Permission = Permission::new(resources::PARTY_BANK_ACCOUNT, "list");

/// 创建银行账户。
pub const BANK_ACCOUNT_CREATE: Permission = Permission::new(resources::PARTY_BANK_ACCOUNT, "create");

/// 更新银行账户。
pub const BANK_ACCOUNT_UPDATE: Permission = Permission::new(resources::PARTY_BANK_ACCOUNT, "update");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "COMPANY_COMPANY_LIST",
        PermissionDef {
            permission: &COMPANY_COMPANY_LIST,
            group: groups::PARTY,
            description: "查询公司主体",
        },
    ),
    (
        "COMPANY_COMPANY_DETAIL",
        PermissionDef {
            permission: &COMPANY_COMPANY_DETAIL,
            group: groups::PARTY,
            description: "查看公司主体",
        },
    ),
    (
        "COMPANY_COMPANY_CREATE",
        PermissionDef {
            permission: &COMPANY_COMPANY_CREATE,
            group: groups::PARTY,
            description: "新建公司主体",
        },
    ),
    (
        "COMPANY_COMPANY_UPDATE",
        PermissionDef {
            permission: &COMPANY_COMPANY_UPDATE,
            group: groups::PARTY,
            description: "维护公司主体",
        },
    ),
    ("LIST", PermissionDef { permission: &LIST, group: groups::PARTY_2, description: "查询主体列表" }),
    ("CREATE", PermissionDef { permission: &CREATE, group: groups::PARTY_2, description: "创建主体" }),
    ("DETAIL", PermissionDef { permission: &DETAIL, group: groups::PARTY_2, description: "查询主体详情" }),
    (
        "UPDATE",
        PermissionDef {
            permission: &UPDATE, group: groups::PARTY_2, description: "更新主体并形成新修订"
        },
    ),
    ("DELETE", PermissionDef { permission: &DELETE, group: groups::PARTY_2, description: "删除主体" }),
    (
        "REVISION_LIST",
        PermissionDef {
            permission: &REVISION_LIST, group: groups::PARTY_2, description: "查询主体修订列表"
        },
    ),
    (
        "CONTACT_LIST",
        PermissionDef {
            permission: &CONTACT_LIST, group: groups::PARTY_2, description: "查询联系人列表"
        },
    ),
    (
        "CONTACT_CREATE",
        PermissionDef { permission: &CONTACT_CREATE, group: groups::PARTY_2, description: "创建联系人" },
    ),
    (
        "CONTACT_UPDATE",
        PermissionDef { permission: &CONTACT_UPDATE, group: groups::PARTY_2, description: "更新联系人" },
    ),
    (
        "ADDRESS_LIST",
        PermissionDef {
            permission: &ADDRESS_LIST, group: groups::PARTY_2, description: "查询地址列表"
        },
    ),
    (
        "ADDRESS_CREATE",
        PermissionDef { permission: &ADDRESS_CREATE, group: groups::PARTY_2, description: "创建地址" },
    ),
    (
        "ADDRESS_UPDATE",
        PermissionDef { permission: &ADDRESS_UPDATE, group: groups::PARTY_2, description: "更新地址" },
    ),
    (
        "TAX_PROFILE_LIST",
        PermissionDef {
            permission: &TAX_PROFILE_LIST,
            group: groups::PARTY_2,
            description: "查询税务资料列表",
        },
    ),
    (
        "TAX_PROFILE_CREATE",
        PermissionDef {
            permission: &TAX_PROFILE_CREATE,
            group: groups::PARTY_2,
            description: "创建税务资料",
        },
    ),
    (
        "TAX_PROFILE_UPDATE",
        PermissionDef {
            permission: &TAX_PROFILE_UPDATE,
            group: groups::PARTY_2,
            description: "更新税务资料",
        },
    ),
    (
        "BANK_ACCOUNT_LIST",
        PermissionDef {
            permission: &BANK_ACCOUNT_LIST,
            group: groups::PARTY_2,
            description: "查询银行账户列表",
        },
    ),
    (
        "BANK_ACCOUNT_CREATE",
        PermissionDef {
            permission: &BANK_ACCOUNT_CREATE,
            group: groups::PARTY_2,
            description: "创建银行账户",
        },
    ),
    (
        "BANK_ACCOUNT_UPDATE",
        PermissionDef {
            permission: &BANK_ACCOUNT_UPDATE,
            group: groups::PARTY_2,
            description: "更新银行账户",
        },
    ),
];
