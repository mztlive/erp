//! `supplier_offering_availability` 高频可供投影。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierOfferingAvailabilityId, SupplierOfferingId};
use crate::money::Quantity;
use crate::supplier_offering::{AvailabilityInterruptionReason, AvailabilityStatus};
use crate::validation::normalize_optional_text;

const SOURCE_REVISION_TOKEN_MAX_LEN: usize = 256;

/// 可供投影写入数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingAvailabilityData {
    /// 所属供给。
    pub supplier_offering_id: SupplierOfferingId,
    /// 当前可供状态。
    pub availability_status: AvailabilityStatus,
    /// 当前可供数量；空表示供应商未返回数量上限。
    pub available_quantity: Option<Quantity>,
    /// 来源更新时间。
    pub source_updated_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 更新人或系统身份。
    pub updated_by: String,
}

/// 每条供给唯一一行、可覆盖更新的实时可供投影。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierOfferingAvailability {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属供给。
    pub supplier_offering_id: SupplierOfferingId,
    /// 当前可供状态。
    pub availability_status: AvailabilityStatus,
    /// 当前可供数量。
    pub available_quantity: Option<Quantity>,
    /// 来源更新时间。
    pub source_updated_at: Instant,
    /// ERP 接收时间。
    pub received_at: Instant,
    /// 来源版本标识。
    pub source_revision_token: Option<String>,
    /// 最近更新人或系统身份。
    pub updated_by: String,
}

impl SupplierOfferingAvailability {
    /// 创建实时可供投影。
    ///
    /// # 参数
    /// * `id` - 投影主键
    /// * `data` - 当前可供事实
    ///
    /// # 返回
    /// 返回规范化后的投影。
    ///
    /// # 错误
    /// 数量为负、来源版本过长或更新人为空时返回错误。
    pub fn new(id: SupplierOfferingAvailabilityId, data: SupplierOfferingAvailabilityData) -> Result<Self> {
        let source_revision_token = normalize_optional_text(
            data.source_revision_token,
            "来源版本",
            SOURCE_REVISION_TOKEN_MAX_LEN,
        )?;
        let updated_by = data.updated_by.trim().to_string();
        ensure_valid(data.available_quantity, &updated_by)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            supplier_offering_id: data.supplier_offering_id,
            availability_status: data.availability_status,
            available_quantity: data.available_quantity,
            source_updated_at: data.source_updated_at,
            received_at: data.received_at,
            source_revision_token,
            updated_by,
        })
    }

    /// 应用不早于当前来源时间的新可供事实。
    ///
    /// # 参数
    /// * `data` - 新可供事实；所属供给必须保持一致
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 来源时间倒退、所属供给变化或字段非法时返回错误。
    pub fn apply(&mut self, data: SupplierOfferingAvailabilityData) -> Result<()> {
        if data.supplier_offering_id != self.supplier_offering_id {
            return Err(Error::from("可供投影不得更换所属供给"));
        }
        if data.source_updated_at < self.source_updated_at {
            return Err(Error::from("可供来源时间早于当前数据"));
        }
        let source_revision_token = normalize_optional_text(
            data.source_revision_token,
            "来源版本",
            SOURCE_REVISION_TOKEN_MAX_LEN,
        )?;
        let updated_by = data.updated_by.trim().to_string();
        ensure_valid(data.available_quantity, &updated_by)?;
        self.availability_status = data.availability_status;
        self.available_quantity = data.available_quantity;
        self.source_updated_at = data.source_updated_at;
        self.received_at = data.received_at;
        self.source_revision_token = source_revision_token;
        self.updated_by = updated_by;
        Ok(())
    }

    /// 校验调用方持有的乐观锁版本仍是当前版本。
    ///
    /// # 参数
    /// * `expected` - 调用方读取到的投影版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 版本不一致时返回领域错误。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("可供投影版本不一致"));
        }
        Ok(())
    }

    /// 返回下一次成功持久化后的投影版本。
    ///
    /// # 返回
    /// 返回当前乐观锁版本加一。
    ///
    /// # 错误
    /// 当前版本已达到 `u64` 上限时返回领域错误。
    pub fn next_persisted_version(&self) -> Result<u64> {
        self.base
            .version
            .checked_add(1)
            .ok_or_else(|| Error::from("可供投影版本已达到上限"))
    }

    /// 返回当前可供事实对应的销售安全中断原因。
    ///
    /// # 返回
    /// 明确停止、不可供、过期或零库存时返回领域原因；正常可供时返回 `None`。
    pub fn interruption_reason(&self) -> Option<AvailabilityInterruptionReason> {
        match (self.availability_status, self.available_quantity) {
            (AvailabilityStatus::Stopped, _) => Some(AvailabilityInterruptionReason::SupplierStopped),
            (AvailabilityStatus::Unavailable, _) => Some(AvailabilityInterruptionReason::SupplyUnavailable),
            (AvailabilityStatus::Stale, _) => Some(AvailabilityInterruptionReason::AvailabilityStale),
            (AvailabilityStatus::Available, Some(quantity)) if quantity.to_decimal().is_zero() => {
                Some(AvailabilityInterruptionReason::ZeroInventory)
            }
            _ => None,
        }
    }

    /// 判断当前投影是否可参与采购且数量未耗尽。
    ///
    /// # 返回
    /// 状态为可供且数量为空或大于零时返回 `true`。
    pub fn is_available(&self) -> bool {
        self.availability_status == AvailabilityStatus::Available
            && self
                .available_quantity
                .is_none_or(|quantity| quantity.to_decimal() > rust_decimal::Decimal::ZERO)
    }
}

fn ensure_valid(quantity: Option<Quantity>, updated_by: &str) -> Result<()> {
    if quantity.is_some_and(|value| value.to_decimal().is_sign_negative()) {
        return Err(Error::from("可供数量不能为负"));
    }
    if updated_by.is_empty() {
        return Err(Error::from("可供更新人不能为空"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{SupplierOfferingAvailability, SupplierOfferingAvailabilityData};
    use crate::common::time::Instant;
    use crate::ids::{SupplierOfferingAvailabilityId, SupplierOfferingId};
    use crate::money::Quantity;
    use crate::supplier_offering::{AvailabilityInterruptionReason, AvailabilityStatus};

    fn data(at: i64) -> SupplierOfferingAvailabilityData {
        SupplierOfferingAvailabilityData {
            supplier_offering_id: SupplierOfferingId::new("offering-1"),
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some(Quantity::from_str("8").unwrap()),
            source_updated_at: Instant::from_unix_secs(at),
            received_at: Instant::from_unix_secs(at),
            source_revision_token: Some(format!("v{at}")),
            updated_by: "system".to_string(),
        }
    }

    #[test]
    fn availability_is_independent_and_rejects_time_regression() {
        let mut availability = SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new("availability-1"),
            data(10),
        )
        .unwrap();
        assert!(availability.is_available());
        assert!(availability.apply(data(9)).is_err());

        let unavailable = SupplierOfferingAvailabilityData {
            availability_status: AvailabilityStatus::Unavailable,
            ..data(11)
        };
        availability.apply(unavailable).unwrap();
        assert!(!availability.is_available());
        assert_eq!(
            availability.interruption_reason(),
            Some(AvailabilityInterruptionReason::SupplyUnavailable)
        );
    }

    #[test]
    fn availability_classifies_zero_inventory_and_checks_version() {
        let mut availability = SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new("availability-1"),
            data(10),
        )
        .unwrap();
        availability
            .apply(SupplierOfferingAvailabilityData {
                available_quantity: Some(Quantity::from_str("0").unwrap()),
                ..data(11)
            })
            .unwrap();

        assert_eq!(
            availability.interruption_reason(),
            Some(AvailabilityInterruptionReason::ZeroInventory)
        );
        assert!(availability.ensure_version(availability.base.version).is_ok());
        assert!(availability
            .ensure_version(availability.base.version.saturating_add(1))
            .is_err());
        assert_eq!(
            availability.next_persisted_version().unwrap(),
            availability.base.version + 1
        );
    }
}
