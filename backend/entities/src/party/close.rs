//! 从属事实关闭生命周期：停用、结束日期与取消默认。
//!
//! `close_at < valid_from` 必须失败且零 mutation；同日关闭不写零长度
//! `valid_to`；晚于开始日才写入 `valid_to = close_at`。

use super::party_address::{PartyAddress, PartyAddressUpdate};
use super::party_bank_account::{PartyBankAccount, PartyBankAccountUpdate};
use super::party_contact::{PartyContact, PartyContactUpdate};
use super::status::EffectiveRecordStatus;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::field_update::FieldUpdate;

/// 关闭日期相对生效开始日的生命周期计划。
enum ClosePlan {
    /// 同日关闭：只停用并取消默认，不写 `valid_to`。
    SameDay,
    /// 晚于开始日：写入结束日期后再停用。
    After { valid_to: BusinessDate },
}

impl ClosePlan {
    /// 按关闭日与开始日计算生命周期计划。
    ///
    /// # 参数
    /// * `valid_from` - 既有事实生效开始日期
    /// * `close_at` - 请求关闭日期
    ///
    /// # 返回
    /// 同日或晚于开始日的关闭计划。
    ///
    /// # 错误
    /// `close_at` 早于 `valid_from` 时返回 [`Error::LogicError`]。
    fn from_dates(valid_from: BusinessDate, close_at: BusinessDate) -> Result<Self> {
        if close_at < valid_from {
            return Err(Error::from("关闭日期不能早于生效开始日期"));
        }
        if close_at == valid_from {
            Ok(Self::SameDay)
        } else {
            Ok(Self::After { valid_to: close_at })
        }
    }

    /// 转换为实体更新使用的结束日期意图。
    ///
    /// # 返回
    /// 同日返回 `Unchanged`，否则返回 `Set(close_at)`。
    fn valid_to_update(self) -> FieldUpdate<BusinessDate> {
        match self {
            Self::SameDay => FieldUpdate::Unchanged,
            Self::After { valid_to } => FieldUpdate::Set(valid_to),
        }
    }
}

impl PartyContact {
    /// 在指定业务日关闭联系人当前事实。
    ///
    /// 倒序日期失败且不修改任何字段；同日只停用并取消默认；晚于开始日
    /// 写入 `valid_to` 后再停用。
    ///
    /// # 参数
    /// * `close_at` - 关闭业务日期
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 关闭成功返回 `Ok(())`。
    ///
    /// # 错误
    /// `close_at < valid_from` 或状态迁移非法时返回领域错误。
    ///
    /// # 关键业务约束
    /// 不得把倒序日期静默当成 `Unchanged` 后继续停用。
    pub fn close_at(&mut self, close_at: BusinessDate, updated_by: impl Into<String>) -> Result<()> {
        let plan = ClosePlan::from_dates(self.valid_from, close_at)?;
        self.update(
            PartyContactUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: plan.valid_to_update(),
                is_default: Some(false),
            },
            updated_by,
        )
    }
}

impl PartyAddress {
    /// 在指定业务日关闭地址当前事实。
    ///
    /// # 参数
    /// * `close_at` - 关闭业务日期
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 关闭成功返回 `Ok(())`。
    ///
    /// # 错误
    /// `close_at < valid_from` 或状态迁移非法时返回领域错误。
    ///
    /// # 关键业务约束
    /// 同日关闭不写零长度 `valid_to`；倒序日期零 mutation。
    pub fn close_at(&mut self, close_at: BusinessDate, updated_by: impl Into<String>) -> Result<()> {
        let plan = ClosePlan::from_dates(self.valid_from, close_at)?;
        self.update(
            PartyAddressUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: plan.valid_to_update(),
                is_default: Some(false),
            },
            updated_by,
        )
    }
}

impl PartyBankAccount {
    /// 在指定业务日关闭银行账户当前事实。
    ///
    /// # 参数
    /// * `close_at` - 关闭业务日期
    /// * `updated_by` - 操作人
    ///
    /// # 返回
    /// 关闭成功返回 `Ok(())`。
    ///
    /// # 错误
    /// `close_at < valid_from` 或状态迁移非法时返回领域错误。
    ///
    /// # 关键业务约束
    /// 同日关闭不写零长度 `valid_to`；倒序日期零 mutation。
    pub fn close_at(&mut self, close_at: BusinessDate, updated_by: impl Into<String>) -> Result<()> {
        let plan = ClosePlan::from_dates(self.valid_from, close_at)?;
        self.update(
            PartyBankAccountUpdate {
                status: Some(EffectiveRecordStatus::Disabled),
                valid_to: plan.valid_to_update(),
                is_default: Some(false),
            },
            updated_by,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{PartyAddress, PartyBankAccount, PartyContact};
    use crate::common::time::BusinessDate;
    use crate::errors::Error;
    use crate::ids::{PartyAddressId, PartyBankAccountId, PartyContactId, PartyId};
    use crate::party::party_address::{AddressType, PartyAddressData};
    use crate::party::party_bank_account::PartyBankAccountData;
    use crate::party::party_contact::PartyContactData;
    use crate::party::status::EffectiveRecordStatus;

    const KEY: &[u8] = b"close-at-test-key";

    fn date(year: i32, month: u32, day: u32) -> BusinessDate {
        BusinessDate::from_ymd(year, month, day).unwrap()
    }

    fn contact() -> PartyContact {
        PartyContact::new(
            PartyContactId::new("contact-close"),
            PartyContactData {
                party_id: PartyId::new("party-1"),
                contact_name: "张三".to_string(),
                title: None,
                mobile: "13800138000".to_string(),
                telephone: None,
                email: None,
                valid_from: date(2026, 1, 1),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            KEY,
            "admin-1",
        )
        .unwrap()
    }

    fn address() -> PartyAddress {
        PartyAddress::new(
            PartyAddressId::new("addr-close"),
            PartyAddressData {
                party_id: PartyId::new("party-1"),
                address_type: AddressType::Operating,
                contact_name: None,
                address: "上海市浦东新区".to_string(),
                valid_from: date(2026, 1, 1),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            KEY,
            "admin-1",
        )
        .unwrap()
    }

    fn bank_account() -> PartyBankAccount {
        PartyBankAccount::new(
            PartyBankAccountId::new("ba-close"),
            PartyBankAccountData {
                bank_account_no: "BA-CLOSE-001".to_string(),
                party_id: PartyId::new("party-1"),
                account_name: "上海示例科技有限公司".to_string(),
                bank_name: "招商银行".to_string(),
                bank_branch_name: None,
                account_number: "6225880212345678".to_string(),
                valid_from: date(2026, 1, 1),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            KEY,
            "admin-1",
        )
        .unwrap()
    }

    #[test]
    fn contact_close_at_covers_order_already_closed_and_default() {
        let mut earlier = contact();
        let snapshot_status = earlier.status;
        let snapshot_valid_to = earlier.valid_to;
        let snapshot_default = earlier.is_default;
        let snapshot_updated_by = earlier.updated_by.clone();
        let error = earlier.close_at(date(2025, 12, 31), "admin-2").unwrap_err();
        assert!(matches!(error, Error::LogicError(_)));
        assert_eq!(earlier.status, snapshot_status);
        assert_eq!(earlier.valid_to, snapshot_valid_to);
        assert_eq!(earlier.is_default, snapshot_default);
        assert_eq!(earlier.updated_by, snapshot_updated_by);

        let mut same_day = contact();
        same_day.close_at(date(2026, 1, 1), "admin-2").unwrap();
        assert_eq!(same_day.status, EffectiveRecordStatus::Disabled);
        assert_eq!(same_day.valid_to, None);
        assert!(!same_day.is_default);
        assert_eq!(same_day.updated_by, "admin-2");

        let mut later = contact();
        later.close_at(date(2026, 3, 1), "admin-3").unwrap();
        assert_eq!(later.status, EffectiveRecordStatus::Disabled);
        assert_eq!(later.valid_to, Some(date(2026, 3, 1)));
        assert!(!later.is_default);

        later.close_at(date(2026, 4, 1), "admin-4").unwrap();
        assert_eq!(later.status, EffectiveRecordStatus::Disabled);
        assert_eq!(later.valid_to, Some(date(2026, 4, 1)));
        assert!(!later.is_default);
        assert_eq!(later.updated_by, "admin-4");
    }

    #[test]
    fn address_close_at_covers_order_already_closed_and_default() {
        let mut earlier = address();
        let snapshot_status = earlier.status;
        let snapshot_valid_to = earlier.valid_to;
        let snapshot_default = earlier.is_default;
        let snapshot_updated_by = earlier.updated_by.clone();
        assert!(earlier.close_at(date(2025, 12, 1), "other").is_err());
        assert_eq!(earlier.status, snapshot_status);
        assert_eq!(earlier.valid_to, snapshot_valid_to);
        assert_eq!(earlier.is_default, snapshot_default);
        assert_eq!(earlier.updated_by, snapshot_updated_by);

        let mut same_day = address();
        same_day.close_at(date(2026, 1, 1), "admin-2").unwrap();
        assert_eq!(same_day.status, EffectiveRecordStatus::Disabled);
        assert_eq!(same_day.valid_to, None);
        assert!(!same_day.is_default);

        let mut later = address();
        later.close_at(date(2026, 2, 1), "admin-3").unwrap();
        assert_eq!(later.valid_to, Some(date(2026, 2, 1)));
        later.close_at(date(2026, 2, 1), "admin-4").unwrap();
        assert_eq!(later.status, EffectiveRecordStatus::Disabled);
        assert!(!later.is_default);
    }

    #[test]
    fn bank_close_at_covers_order_already_closed_and_default() {
        let mut earlier = bank_account();
        let snapshot_status = earlier.status;
        let snapshot_valid_to = earlier.valid_to;
        let snapshot_default = earlier.is_default;
        let snapshot_updated_by = earlier.updated_by.clone();
        assert!(earlier.close_at(date(2025, 1, 1), "other").is_err());
        assert_eq!(earlier.status, snapshot_status);
        assert_eq!(earlier.valid_to, snapshot_valid_to);
        assert_eq!(earlier.is_default, snapshot_default);
        assert_eq!(earlier.updated_by, snapshot_updated_by);

        let mut same_day = bank_account();
        same_day.close_at(date(2026, 1, 1), "admin-2").unwrap();
        assert_eq!(same_day.status, EffectiveRecordStatus::Disabled);
        assert_eq!(same_day.valid_to, None);
        assert!(!same_day.is_default);

        let mut later = bank_account();
        later.close_at(date(2026, 6, 1), "admin-3").unwrap();
        assert_eq!(later.valid_to, Some(date(2026, 6, 1)));
        later.close_at(date(2026, 7, 1), "admin-4").unwrap();
        assert_eq!(later.status, EffectiveRecordStatus::Disabled);
        assert!(!later.is_default);
        assert_eq!(later.updated_by, "admin-4");
    }
}
