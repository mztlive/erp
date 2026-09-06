//! Consumer-side Party snapshots used by customer DTOs.
//!
//! These types preserve the original HTTP/JSON field names and serde rename
//! rules. They are not Party aggregates and must not be persisted as live
//! Party documents.

use serde::{Deserialize, Serialize};

/// Address type snapshot (`snake_case` wire codes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddressType {
    /// 注册地址。
    Registered,
    /// 经营地址。
    Operating,
    /// 履约地址（敏感值，加密存储）。
    Fulfillment,
}

/// Party 启停状态快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartyStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

/// 从属事实行启停状态快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectiveRecordStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

/// 可揭示的敏感字段类型快照。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveFieldKind {
    /// 联系人手机号。
    ContactMobile,
    /// 履约地址。
    Address,
    /// 银行账号。
    BankAccountNumber,
}

/// 主体修订响应视图快照。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyRevisionView {
    /// 实体主键。
    pub id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 法定名称。
    pub legal_name: String,
    /// 简称。
    pub short_name: Option<String>,
    /// 变更原因。
    pub change_reason: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 联系人响应视图快照（不含明文敏感字段）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyContactView {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 联系人姓名。
    pub contact_name: String,
    /// 职务/用途。
    pub title: Option<String>,
    /// 电话。
    pub telephone: Option<String>,
    /// 手机号掩码；列表与详情均不返回明文。
    pub mobile_masked: String,
    /// 邮箱。
    pub email: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认联系人。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 地址响应视图快照（不含敏感明文）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyAddressView {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 地址类型。
    pub address_type: AddressType,
    /// 联系人。
    pub contact_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认地址。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 税务资料响应视图快照。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyTaxProfileView {
    /// 实体主键。
    pub id: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 纳税人识别号。
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认税务资料。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 银行账户响应视图快照（不含明文账号）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PartyBankAccountView {
    /// 实体主键。
    pub id: String,
    /// ERP 内部稳定账户编号。
    pub bank_account_no: String,
    /// 所属企业主体 ID。
    pub party_id: String,
    /// 户名。
    pub account_name: String,
    /// 银行名称。
    pub bank_name: String,
    /// 银行账号掩码；列表与详情均不返回明文。
    pub account_number_masked: String,
    /// 支行名称。
    pub bank_branch_name: Option<String>,
    /// 生效开始日期。
    pub valid_from: String,
    /// 生效结束日期。
    pub valid_to: Option<String>,
    /// 是否当前默认账户。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}
