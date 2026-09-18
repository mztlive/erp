//! 域 D07 `party`：party、party_revision、party_contact、party_address、
//! party_tax_profile、party_bank_account（页面：W14、W03）。
//!
//! 实体层无跨域依赖：只引用 `entities::ids` 的 ID newtype 与 `common` 基元。
//! 字段字典与唯一约束见数据模型 §6.2；公共字段归属按 §4.3 判定：
//! - `party` 是「稳定基础资料」→ 组合 [`erp_core::common::StableBase`]；
//! - `party_revision` 是不可变修订 → 组合 [`erp_core::common::RevisionBase`]，
//!   法定名称/简称按 §2.2 / §4.4 内联为结构化快照；
//! - `party_contact` / `party_address` / `party_tax_profile` / `party_bank_account`
//!   是「支持有效期的从属事实行」（§5.2 / §6.2），按字段字典精确建模：
//!   `BaseModel` + 业务字段 + 启停状态 + 生效区间，不硬套 StableBase；
//! - 敏感值（银行账号、联系人手机号、履约地址）按 §4.5.5 / P1 §2.1 建模为
//!   `*_ciphertext`（P3 加密填充）+ `*_query_hmac`（带密钥 HMAC 查询指纹）
//!   双字段，明文字段永远不进入 `Debug` 输出。

mod close;
pub mod company;
mod content_match;
mod entity;
pub mod party_address;
pub mod party_bank_account;
pub mod party_contact;
pub mod party_revision;
pub mod party_tax_profile;
mod sensitive;
pub mod status;

pub use content_match::{
    PartyAddressContentMatch, PartyBankAccountContentMatch, PartyContactContentMatch, QueryFingerprint,
    SensitiveFactReuse,
};
pub use entity::{Party, PartyData, PartyKind, PartyStatus, PartyUpdate};
use erp_core::common::time::BusinessDate;
pub use erp_core::ids::{
    PartyAddressId, PartyBankAccountId, PartyContactId, PartyId, PartyRevisionId, PartyTaxProfileId,
};
use erp_core::{Error, Result};
pub use party_address::{AddressType, PartyAddress, PartyAddressData, PartyAddressUpdate};
pub use party_bank_account::{PartyBankAccount, PartyBankAccountData, PartyBankAccountUpdate};
pub use party_contact::{PartyContact, PartyContactData, PartyContactUpdate};
pub use party_revision::{PartyRevision, PartyRevisionData};
pub use party_tax_profile::{PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileUpdate};
pub use status::{EffectiveRecordStatus, SymmetricActiveStatus, select_current_default};

/// 校验期望版本与实体当前版本一致（erp-party-008）。
///
/// 主体与四个从属事实的乐观锁冲突文案唯一来源；各实体 `ensure_version`
/// 均转调本函数，Service 层只做错误类映射。
///
/// # 参数
/// * `base` - 实体基础字段（含当前版本）
/// * `expected` - 调用方持有的期望版本
///
/// # 返回
/// 版本一致返回 `Ok(())`。
///
/// # 错误
/// 版本不一致时返回版本冲突错误。
pub(crate) fn ensure_base_version(base: &entity_core::BaseModel, expected: u64) -> Result<()> {
    if base.version == expected {
        return Ok(());
    }
    Err(Error::from("数据已被其他请求修改，请刷新后重试"))
}

/// 校验从属事实生效区间（erp-party-010）。
///
/// 四个从属事实（联系人/地址/税务资料/银行账户）的生效区间规则唯一来源；
/// 各实体 `new`/`update` 均转调本函数。空结束日期表示长期有效。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 生效结束日期；`None` 表示长期有效
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回错误。
pub(crate) fn ensure_valid_window(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to
        && valid_to <= valid_from
    {
        return Err(Error::from("生效结束日期必须晚于生效开始日期"));
    }
    Ok(())
}

/// 由单一 Party 拥有的从属实体。
pub trait PartyOwned {
    /// 返回实体所属 Party ID。
    ///
    /// # 返回
    /// 返回稳定 Party ID 引用。
    fn party_id(&self) -> &PartyId;

    /// 校验实体属于指定 Party。
    ///
    /// # 参数
    /// * `expected` - 期望的 Party ID
    ///
    /// # 返回
    /// Party 归属一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 实体归属其他 Party 时返回错误。
    fn ensure_party(&self, expected: &PartyId) -> Result<()> {
        if self.party_id() == expected {
            return Ok(());
        }
        Err(Error::from("资料事实不属于指定主体"))
    }
}

impl PartyOwned for PartyRevision {
    fn party_id(&self) -> &PartyId {
        &self.party_id
    }
}

impl PartyOwned for PartyContact {
    fn party_id(&self) -> &PartyId {
        &self.party_id
    }
}

impl PartyOwned for PartyAddress {
    fn party_id(&self) -> &PartyId {
        &self.party_id
    }
}

impl PartyOwned for PartyTaxProfile {
    fn party_id(&self) -> &PartyId {
        &self.party_id
    }
}

impl PartyOwned for PartyBankAccount {
    fn party_id(&self) -> &PartyId {
        &self.party_id
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;

    use super::{
        EffectiveRecordStatus, PartyContact, PartyContactData, PartyContactId, PartyId, PartyOwned,
        ensure_valid_window,
    };

    #[test]
    fn valid_window_accepts_open_and_ordered_range() {
        let from = BusinessDate::from_ymd(2026, 1, 1).unwrap();
        assert!(ensure_valid_window(from, None).is_ok());
        assert!(ensure_valid_window(from, Some(BusinessDate::from_ymd(2026, 1, 2).unwrap())).is_ok());
    }

    #[test]
    fn valid_window_rejects_same_day_and_reversed_range() {
        let from = BusinessDate::from_ymd(2026, 1, 1).unwrap();
        assert!(ensure_valid_window(from, Some(from)).is_err());
        assert!(ensure_valid_window(from, Some(BusinessDate::from_ymd(2025, 12, 31).unwrap())).is_err());
    }

    #[test]
    fn party_owned_accepts_matching_party_and_rejects_other_party() {
        let contact = PartyContact::new(
            PartyContactId::new("contact-owned"),
            PartyContactData {
                party_id: PartyId::new("party-1"),
                contact_name: "张三".to_string(),
                title: None,
                mobile: "13800138000".to_string(),
                telephone: None,
                email: None,
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                is_default: false,
                status: EffectiveRecordStatus::Active,
            },
            b"party-owned-test-key",
            "admin-1",
        )
        .unwrap();

        assert!(contact.ensure_party(&PartyId::new("party-1")).is_ok());
        assert!(contact.ensure_party(&PartyId::new("party-2")).is_err());
    }
}
