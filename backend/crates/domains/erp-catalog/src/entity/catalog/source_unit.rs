//! 供应来源单位复用与缺项建档规则；不隐式换算数量。
use erp_core::ids::UnitOfMeasureId;

use super::unit_of_measure::{UnitOfMeasure, UnitOfMeasureData};
use crate::{Error, Result};

impl UnitOfMeasure {
    /// 从精确匹配候选中选择单位，缺项时构造稳定身份的待创建单位。
    /// # 参数
    /// `name` 为来源单位，`candidates` 包括停用和删除记录，`actor` 为操作人。
    /// # 返回
    /// 唯一启用单位或尚未保存的新单位；新单位按整项订购，数量精度为零。
    /// # 错误
    /// 单位非法、多候选、停用或删除时拒绝。
    pub fn from_source(name: &str, mut candidates: Vec<Self>, actor: &str) -> Result<Self> {
        let name = name.trim();
        match candidates.len() {
            0 => Ok(Self::new(
                UnitOfMeasureId::new(format!("source-unit-{}", name.to_lowercase())),
                UnitOfMeasureData::new(format!("AUTO-{}", name.to_uppercase()), name, name),
                actor,
            )?),
            1 => {
                let unit = candidates.remove(0);
                if !unit.is_active() || unit.base.is_deleted() {
                    return Err(Error::ValidationError("对应计量单位已停用或删除，请先维护单位字典".into()));
                }
                Ok(unit)
            },
            _ => Err(Error::ConflictError("来源单位对应多个公司单位，请明确选择公司计量单位".into())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::catalog::status::EnableStatus;

    #[test]
    fn missing_unit_has_stable_identity_and_reuses_existing_unit() {
        let planned = UnitOfMeasure::from_source(" 磅 ", vec![], "buyer").unwrap();
        assert_eq!(planned.name, "磅");
        assert_eq!(planned.quantity_scale, 0);
        assert_eq!(planned.base.id, UnitOfMeasure::from_source("磅", vec![], "other").unwrap().base.id);
        let existing = UnitOfMeasure::new(
            UnitOfMeasureId::new("old"),
            UnitOfMeasureData::new("LB", "磅", "lb").with_quantity_scale(3),
            "admin",
        )
        .unwrap();
        let resolved = UnitOfMeasure::from_source("磅", vec![existing], "buyer").unwrap();
        assert_eq!(resolved.base.id, "old");
        assert_eq!(resolved.quantity_scale, 3);
    }

    #[test]
    fn invalid_disabled_deleted_and_ambiguous_units_are_not_recreated() {
        assert!(UnitOfMeasure::from_source("", vec![], "buyer").is_err());
        assert!(UnitOfMeasure::from_source(&"字".repeat(33), vec![], "buyer").is_err());
        let mut unit = UnitOfMeasure::from_source("盒", vec![], "buyer").unwrap();
        assert!(UnitOfMeasure::from_source("盒", vec![unit.clone(), unit.clone()], "buyer").is_err());
        unit.stable.status = EnableStatus::Disabled;
        assert!(UnitOfMeasure::from_source("盒", vec![unit.clone()], "buyer").is_err());
        unit.stable.status = EnableStatus::Active;
        unit.base.deleted_at = 1;
        assert!(UnitOfMeasure::from_source("盒", vec![unit], "buyer").is_err());
    }
}
