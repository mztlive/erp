//! 域 D07 `party`：party、party_revision、party_contact、party_address、
//! party_tax_profile、party_bank_account（页面：W14、W03）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.2；公共字段归属按 §4.3 判定：
//! - `party` 是「稳定基础资料」→ 组合 [`crate::common::StableBase`]；
//! - `party_revision` 是不可变修订 → 组合 [`crate::common::RevisionBase`]，
//!   法定名称/简称按 §2.2 / §4.4 内联为结构化快照；
//! - `party_contact` / `party_address` / `party_tax_profile` / `party_bank_account`
//!   是「支持有效期的从属事实行」（§5.2 / §6.2），按字段字典精确建模：
//!   `BaseModel` + 业务字段 + 启停状态 + 生效区间，不硬套 StableBase；
//! - 敏感值（银行账号、联系人手机号、履约地址）按 §4.5.5 / P1 §2.1 建模为
//!   `*_ciphertext`（P3 加密填充）+ `*_query_hmac`（带密钥 HMAC 查询指纹）
//!   双字段，明文字段永远不进入 `Debug` 输出。

#[allow(clippy::module_inception)]
pub mod party;
pub mod party_address;
pub mod party_bank_account;
pub mod party_contact;
pub mod party_revision;
pub mod party_tax_profile;
mod sensitive;
pub mod status;

pub use crate::ids::{
    PartyAddressId, PartyBankAccountId, PartyContactId, PartyId, PartyRevisionId, PartyTaxProfileId,
};
pub use party::{Party, PartyData, PartyKind, PartyStatus, PartyUpdate};
pub use party_address::{AddressType, PartyAddress, PartyAddressData, PartyAddressUpdate};
pub use party_bank_account::{PartyBankAccount, PartyBankAccountData, PartyBankAccountUpdate};
pub use party_contact::{PartyContact, PartyContactData, PartyContactUpdate};
pub use party_revision::{PartyRevision, PartyRevisionData};
pub use party_tax_profile::{PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileUpdate};
pub use status::EffectiveRecordStatus;
