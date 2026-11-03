//! party 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询公司主体。
pub const COMPANY_COMPANY_LIST: PermissionDef =
    PermissionDef { code: "company:list", group: groups::PARTY, description: "查询公司主体" };

/// 查看公司主体。
pub const COMPANY_COMPANY_DETAIL: PermissionDef =
    PermissionDef { code: "company:detail", group: groups::PARTY, description: "查看公司主体" };

/// 新建公司主体。
pub const COMPANY_COMPANY_CREATE: PermissionDef =
    PermissionDef { code: "company:create", group: groups::PARTY, description: "新建公司主体" };

/// 维护公司主体。
pub const COMPANY_COMPANY_UPDATE: PermissionDef =
    PermissionDef { code: "company:update", group: groups::PARTY, description: "维护公司主体" };

/// 查询主体列表。
pub const LIST: PermissionDef =
    PermissionDef { code: "party:list", group: groups::PARTY_2, description: "查询主体列表" };

/// 创建主体。
pub const CREATE: PermissionDef =
    PermissionDef { code: "party:create", group: groups::PARTY_2, description: "创建主体" };

/// 查询主体详情。
pub const DETAIL: PermissionDef =
    PermissionDef { code: "party:detail", group: groups::PARTY_2, description: "查询主体详情" };

/// 更新主体并形成新修订。
pub const UPDATE: PermissionDef =
    PermissionDef {
        code: "party:update", group: groups::PARTY_2, description: "更新主体并形成新修订"
    };

/// 删除主体。
pub const DELETE: PermissionDef =
    PermissionDef { code: "party:delete", group: groups::PARTY_2, description: "删除主体" };

/// 查询主体修订列表。
pub const REVISION_LIST: PermissionDef =
    PermissionDef {
        code: "party_revision:list", group: groups::PARTY_2, description: "查询主体修订列表"
    };

/// 查询联系人列表。
pub const CONTACT_LIST: PermissionDef =
    PermissionDef {
        code: "party_contact:list", group: groups::PARTY_2, description: "查询联系人列表"
    };

/// 创建联系人。
pub const CONTACT_CREATE: PermissionDef =
    PermissionDef { code: "party_contact:create", group: groups::PARTY_2, description: "创建联系人" };

/// 更新联系人。
pub const CONTACT_UPDATE: PermissionDef =
    PermissionDef { code: "party_contact:update", group: groups::PARTY_2, description: "更新联系人" };

/// 查询地址列表。
pub const ADDRESS_LIST: PermissionDef =
    PermissionDef { code: "party_address:list", group: groups::PARTY_2, description: "查询地址列表" };

/// 创建地址。
pub const ADDRESS_CREATE: PermissionDef =
    PermissionDef { code: "party_address:create", group: groups::PARTY_2, description: "创建地址" };

/// 更新地址。
pub const ADDRESS_UPDATE: PermissionDef =
    PermissionDef { code: "party_address:update", group: groups::PARTY_2, description: "更新地址" };

/// 查询税务资料列表。
pub const TAX_PROFILE_LIST: PermissionDef =
    PermissionDef {
        code: "party_tax_profile:list", group: groups::PARTY_2, description: "查询税务资料列表"
    };

/// 创建税务资料。
pub const TAX_PROFILE_CREATE: PermissionDef =
    PermissionDef {
        code: "party_tax_profile:create", group: groups::PARTY_2, description: "创建税务资料"
    };

/// 更新税务资料。
pub const TAX_PROFILE_UPDATE: PermissionDef =
    PermissionDef {
        code: "party_tax_profile:update", group: groups::PARTY_2, description: "更新税务资料"
    };

/// 查询银行账户列表。
pub const BANK_ACCOUNT_LIST: PermissionDef = PermissionDef {
    code: "party_bank_account:list",
    group: groups::PARTY_2,
    description: "查询银行账户列表",
};

/// 创建银行账户。
pub const BANK_ACCOUNT_CREATE: PermissionDef =
    PermissionDef {
        code: "party_bank_account:create", group: groups::PARTY_2, description: "创建银行账户"
    };

/// 更新银行账户。
pub const BANK_ACCOUNT_UPDATE: PermissionDef =
    PermissionDef {
        code: "party_bank_account:update", group: groups::PARTY_2, description: "更新银行账户"
    };

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("COMPANY_COMPANY_LIST", COMPANY_COMPANY_LIST),
    ("COMPANY_COMPANY_DETAIL", COMPANY_COMPANY_DETAIL),
    ("COMPANY_COMPANY_CREATE", COMPANY_COMPANY_CREATE),
    ("COMPANY_COMPANY_UPDATE", COMPANY_COMPANY_UPDATE),
    ("LIST", LIST),
    ("CREATE", CREATE),
    ("DETAIL", DETAIL),
    ("UPDATE", UPDATE),
    ("DELETE", DELETE),
    ("REVISION_LIST", REVISION_LIST),
    ("CONTACT_LIST", CONTACT_LIST),
    ("CONTACT_CREATE", CONTACT_CREATE),
    ("CONTACT_UPDATE", CONTACT_UPDATE),
    ("ADDRESS_LIST", ADDRESS_LIST),
    ("ADDRESS_CREATE", ADDRESS_CREATE),
    ("ADDRESS_UPDATE", ADDRESS_UPDATE),
    ("TAX_PROFILE_LIST", TAX_PROFILE_LIST),
    ("TAX_PROFILE_CREATE", TAX_PROFILE_CREATE),
    ("TAX_PROFILE_UPDATE", TAX_PROFILE_UPDATE),
    ("BANK_ACCOUNT_LIST", BANK_ACCOUNT_LIST),
    ("BANK_ACCOUNT_CREATE", BANK_ACCOUNT_CREATE),
    ("BANK_ACCOUNT_UPDATE", BANK_ACCOUNT_UPDATE),
];
