//! `party_tax_profile`：税号及税务资料（数据模型 §5.2 / §6.2 归属）。
//!
//! 税务资料按「有效期事实追加」维护（W11.1 历史版本），内容变更
//! 不原地修改；税号不属 §4.5.5 加密清单（银行账号、联系人手机号、
//! 履约地址），按普通受控文本建模。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;

use super::status::EffectiveRecordStatus;

pub use crate::ids::{PartyId, PartyTaxProfileId};

/// 税号最大长度。
const TAX_NO_MAX_LEN: usize = 32;

/// 税务资料创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyTaxProfileData {
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 纳税人识别号（统一社会信用代码或旧税号）。
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期；`None` 表示长期有效。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认税务资料。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
}

/// 税务资料更新数据。
///
/// 税号变更按有效期事实追加维护，原地更新只允许切换启停状态、
/// 结束有效期与调整默认标记。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartyTaxProfileUpdate {
    /// 启停状态；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<EffectiveRecordStatus>,
    /// 生效结束日期更新意图（`Set` 时校验晚于 `valid_from`）。
    #[serde(default, skip_serializing_if = "FieldUpdate::is_unchanged")]
    pub valid_to: FieldUpdate<BusinessDate>,
    /// 默认标记；`None` 表示不修改。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub is_default: Option<bool>,
}

/// 税务资料实体（§5.2：税号及税务资料；§4.3：稳定基础资料从属行）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PartyTaxProfile {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属企业主体 ID。
    pub party_id: PartyId,
    /// 纳税人识别号（去空白、大写规范化）。
    pub tax_no: String,
    /// 生效开始日期。
    pub valid_from: BusinessDate,
    /// 生效结束日期。
    pub valid_to: Option<BusinessDate>,
    /// 是否为当前默认税务资料。
    pub is_default: bool,
    /// 启停状态。
    pub status: EffectiveRecordStatus,
    /// 创建人。
    pub created_by: String,
    /// 最后更新人。
    pub updated_by: String,
}

impl PartyTaxProfile {
    /// 创建税务资料。
    ///
    /// 完成税号的必填校验与规范化（去首尾空白、转大写、字母数字、
    /// 长度上限）；强制 `valid_to` 晚于 `valid_from`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PartyTaxProfileId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的税务资料实体。
    ///
    /// # 错误
    /// 当税号为空/超长/含非法字符或生效区间倒挂时返回错误。
    pub fn new(
        id: PartyTaxProfileId,
        data: PartyTaxProfileData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let tax_no = normalize_tax_no(data.tax_no)?;
        let created_by = created_by.into();
        ensure_window_valid(data.valid_from, data.valid_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            party_id: data.party_id,
            tax_no,
            valid_from: data.valid_from,
            valid_to: data.valid_to,
            is_default: data.is_default,
            status: data.status,
            updated_by: created_by.clone(),
            created_by,
        })
    }

    /// 更新税务资料（仅限生命周期字段）。
    ///
    /// 税号变更必须通过新的有效期事实行追加；原地更新只允许切换
    /// 启停状态（固定状态机）、结束有效期与调整默认标记。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当状态迁移非法或 `valid_to` 不晚于 `valid_from` 时返回错误。
    pub fn update(&mut self, update: PartyTaxProfileUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(to) = update.status {
            self.status.transition_to(to)?;
        }
        if let Some(valid_to) = update.valid_to.into_option() {
            ensure_window_valid(self.valid_from, Some(valid_to))?;
            self.valid_to = Some(valid_to);
        }
        if let Some(is_default) = update.is_default {
            self.is_default = is_default;
        }
        self.updated_by = updated_by.into();
        Ok(())
    }

    /// 判断税务资料是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.status.is_active()
    }
}

/// 规范化税号：去首尾空白、转大写并校验字母数字与长度。
///
/// # 参数
/// * `value` - 原始输入
///
/// # 返回
/// 返回规范化后的税号。
///
/// # 错误
/// 税号为空、超长或含非字母数字字符时返回错误。
fn normalize_tax_no(value: String) -> Result<String> {
    let value = value.trim().to_uppercase();
    if value.is_empty() {
        return Err(Error::from("税号不能为空"));
    }
    if value.chars().count() > TAX_NO_MAX_LEN {
        return Err(Error::from("税号过长"));
    }
    if !value.chars().all(|ch| ch.is_ascii_alphanumeric()) {
        return Err(Error::from("税号只能包含字母和数字"));
    }
    Ok(value)
}

/// 校验生效区间：`valid_to` 必须晚于 `valid_from`。
///
/// # 参数
/// * `valid_from` - 生效开始日期
/// * `valid_to` - 生效结束日期（可空）
///
/// # 返回
/// 区间合法返回 `Ok(())`。
///
/// # 错误
/// 结束日期不晚于开始日期时返回错误。
fn ensure_window_valid(valid_from: BusinessDate, valid_to: Option<BusinessDate>) -> Result<()> {
    if let Some(valid_to) = valid_to {
        if valid_to <= valid_from {
            return Err(Error::from("生效结束日期必须晚于生效开始日期"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PartyTaxProfile, PartyTaxProfileData, PartyTaxProfileUpdate};
    use crate::common::time::BusinessDate;
    use crate::field_update::FieldUpdate;
    use crate::ids::{PartyId, PartyTaxProfileId};
    use crate::party::status::EffectiveRecordStatus;

    fn tax_profile_data() -> PartyTaxProfileData {
        PartyTaxProfileData {
            party_id: PartyId::new("party-1"),
            tax_no: " 91310000ma1bl4kw9x ".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            is_default: true,
            status: EffectiveRecordStatus::Active,
        }
    }

    /// happy path：税号去空白并转大写。
    #[test]
    fn new_trims_and_uppercases_tax_no() {
        let profile =
            PartyTaxProfile::new(PartyTaxProfileId::new("tax-1"), tax_profile_data(), "admin-1").unwrap();
        assert_eq!(profile.tax_no, "91310000MA1BL4KW9X");
        assert!(profile.is_default);
        assert!(profile.is_active());
    }

    /// 失败路径：税号为空/超长/含非法字符、区间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank = PartyTaxProfileData {
            tax_no: "   ".to_string(),
            ..tax_profile_data()
        };
        assert!(PartyTaxProfile::new(PartyTaxProfileId::new("t"), blank, "admin-1").is_err());

        let overlong = PartyTaxProfileData {
            tax_no: "x".repeat(33),
            ..tax_profile_data()
        };
        assert!(PartyTaxProfile::new(PartyTaxProfileId::new("t"), overlong, "admin-1").is_err());

        let illegal = PartyTaxProfileData {
            tax_no: "91-3100".to_string(),
            ..tax_profile_data()
        };
        assert!(PartyTaxProfile::new(PartyTaxProfileId::new("t"), illegal, "admin-1").is_err());

        let reversed = PartyTaxProfileData {
            valid_to: Some(BusinessDate::from_ymd(2025, 12, 31).unwrap()),
            ..tax_profile_data()
        };
        assert!(PartyTaxProfile::new(PartyTaxProfileId::new("t"), reversed, "admin-1").is_err());
    }

    /// 状态机与生命周期更新：启停切换、结束有效期。
    #[test]
    fn lifecycle_updates_are_validated() {
        let mut profile =
            PartyTaxProfile::new(PartyTaxProfileId::new("tax-2"), tax_profile_data(), "admin-1").unwrap();
        profile
            .update(
                PartyTaxProfileUpdate {
                    status: Some(EffectiveRecordStatus::Disabled),
                    valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2026, 6, 30).unwrap()),
                    is_default: Some(false),
                },
                "admin-2",
            )
            .unwrap();
        assert!(!profile.is_active());
        assert_eq!(
            profile.valid_to,
            Some(BusinessDate::from_ymd(2026, 6, 30).unwrap())
        );
        assert!(!profile.is_default);

        let reversed = PartyTaxProfileUpdate {
            status: None,
            valid_to: FieldUpdate::Set(BusinessDate::from_ymd(2025, 1, 1).unwrap()),
            is_default: None,
        };
        assert!(profile.update(reversed, "admin-3").is_err());
    }

    /// 实体 BSON 往返。
    #[test]
    fn bson_roundtrip() {
        let profile =
            PartyTaxProfile::new(PartyTaxProfileId::new("tax-3"), tax_profile_data(), "admin-1").unwrap();
        let roundtrip: PartyTaxProfile = bson::from_document(bson::to_document(&profile).unwrap()).unwrap();
        assert_eq!(roundtrip, profile);
    }
}
