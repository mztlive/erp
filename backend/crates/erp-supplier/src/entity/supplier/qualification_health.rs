//! 供应商资质在列表上的健康状态折叠规则。
//!
//! 筛选接口按「是否存在某一状态的资质」命中供应商；列表展示把同一供应商的
//! 全部资质折叠为一条最需要处理的状态，供扫表识别，不改变筛选语义。

use erp_core::common::time::BusinessDate;

use super::supplier_qualification::{QualificationStatus, QualificationType, SupplierQualification};

/// 列表展示用的资质健康状态，语义与筛选枚举一一对应。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QualificationHealth {
    /// 合同有效期缺少起始日或截止日。
    Unverified,
    /// 当前有效。
    Valid,
    /// 当前有效且 30 天内到期。
    Expiring30,
    /// 已标记失效，或有效期已过。
    Expired,
    /// 尚未登记资质记录。
    NotRegistered,
}

impl QualificationHealth {
    /// 返回列表展示优先级；数值越大越需要处理。
    ///
    /// # 返回
    /// 返回稳定的严重程度，用于多份资质折叠时取最差状态。
    fn severity(self) -> u8 {
        match self {
            Self::Expired => 4,
            Self::Unverified => 3,
            Self::Expiring30 => 2,
            Self::Valid => 1,
            Self::NotRegistered => 0,
        }
    }
}

impl SupplierQualification {
    /// 按业务日判断单份资质落入的列表健康状态。
    ///
    /// 停用且日期已核实、或尚未到生效日的资质不参与折叠。未核实日期优先于
    /// 启停状态，与列表筛选「有效期未核实」口径一致。
    ///
    /// # 参数
    /// * `as_of` - 判定所用业务日
    ///
    /// # 返回
    /// 可参与折叠时返回对应状态；应忽略时返回 `None`。
    ///
    /// # 错误
    /// 无。到期窗口溢出时不把该份资质标为即将到期。
    pub fn health_on(&self, as_of: BusinessDate) -> Option<QualificationHealth> {
        if !self.validity_verified() {
            return Some(QualificationHealth::Unverified);
        }
        if self.stable.status == QualificationStatus::Disabled {
            return None;
        }
        let expired_by_date = self.valid_to.is_some_and(|end| end < as_of);
        if self.stable.status == QualificationStatus::Expired || expired_by_date {
            return Some(QualificationHealth::Expired);
        }
        if self.valid_from.is_some_and(|start| start > as_of) {
            return None;
        }
        let expiring = self
            .valid_to
            .is_some_and(|end| Self::expiry_cutoff(as_of, 30).map(|cutoff| end <= cutoff).unwrap_or(false));
        if expiring {
            return Some(QualificationHealth::Expiring30);
        }
        Some(QualificationHealth::Valid)
    }

    /// 将一组资质折叠为列表展示用的健康状态。
    ///
    /// 空集合为未登记。多份资质取严重程度最高者；全部被忽略（仅停用或未到
    /// 生效日）时仍视为未登记，避免把不可用资料显示为有效。
    ///
    /// # 参数
    /// * `qualifications` - 同一供应商的资质集合
    /// * `as_of` - 判定所用业务日
    ///
    /// # 返回
    /// 返回折叠后的健康状态。
    ///
    /// # 错误
    /// 无。
    pub fn rollup_health<'a, I>(qualifications: I, as_of: BusinessDate) -> QualificationHealth
    where
        I: IntoIterator<Item = &'a Self>,
    {
        let mut worst = QualificationHealth::NotRegistered;
        for qualification in qualifications {
            let Some(health) = qualification.health_on(as_of) else {
                continue;
            };
            if health.severity() > worst.severity() {
                worst = health;
            }
        }
        worst
    }

    /// 返回已登记资质类型的去重稳定序列。
    ///
    /// 含停用与过期记录，供列表副行展示「有哪些资料」，不表示当前可用。
    ///
    /// # 参数
    /// * `qualifications` - 同一供应商的资质集合
    ///
    /// # 返回
    /// 按稳定代码排序、去重后的类型列表。
    ///
    /// # 错误
    /// 无。
    pub fn registered_types<'a, I>(qualifications: I) -> Vec<QualificationType>
    where
        I: IntoIterator<Item = &'a Self>,
    {
        let mut types: Vec<QualificationType> =
            qualifications.into_iter().map(|qualification| qualification.qualification_type).collect();
        types.sort_by_key(QualificationType::as_str);
        types.dedup();
        types
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::BusinessDate;
    use erp_core::ids::{SupplierAccountId, SupplierQualificationId};

    use super::{QualificationHealth, SupplierQualification};
    use crate::entity::supplier::{QualificationStatus, QualificationType, SupplierQualificationData};

    /// 构造指定类型、状态与日期窗口的资质。
    ///
    /// # 参数
    /// * `id` - 资质稳定 ID
    /// * `kind` - 资质类型
    /// * `status` - 启停/失效状态
    /// * `valid_from` - 起始日；`None` 表示未核实
    /// * `valid_to` - 截止日；合同缺少截止日视为未核实
    ///
    /// # 返回
    /// 返回构造成功的资质实体。
    fn qualification(
        id: &str,
        kind: QualificationType,
        status: QualificationStatus,
        valid_from: Option<(i32, u32, u32)>,
        valid_to: Option<(i32, u32, u32)>,
    ) -> SupplierQualification {
        SupplierQualification::new(
            SupplierQualificationId::new(id),
            SupplierQualificationData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                qualification_type: kind,
                certificate_no: id.to_string(),
                issuer: None,
                valid_from: valid_from.map(|(y, m, d)| BusinessDate::from_ymd(y, m, d).unwrap()),
                valid_to: valid_to.map(|(y, m, d)| BusinessDate::from_ymd(y, m, d).unwrap()),
                attachment_id: None,
                status,
            },
            "test",
        )
        .unwrap()
    }

    /// 返回测试用业务日 2026-08-31。
    fn as_of() -> BusinessDate {
        BusinessDate::from_ymd(2026, 8, 31).unwrap()
    }

    /// 空集合折叠为未登记。
    #[test]
    fn rollup_empty_is_not_registered() {
        let none: &[SupplierQualification] = &[];
        assert_eq!(SupplierQualification::rollup_health(none, as_of()), QualificationHealth::NotRegistered);
    }

    /// 合同缺日期为未核实；停用且日期已核实时不参与折叠。
    #[test]
    fn unverified_outranks_disabled() {
        let unverified =
            qualification("ht-open", QualificationType::Contract, QualificationStatus::Active, None, None);
        let disabled = qualification(
            "lic-off",
            QualificationType::FoodLicense,
            QualificationStatus::Disabled,
            Some((2026, 1, 1)),
            Some((2026, 12, 31)),
        );
        assert_eq!(unverified.health_on(as_of()), Some(QualificationHealth::Unverified));
        assert_eq!(disabled.health_on(as_of()), None);
        assert_eq!(
            SupplierQualification::rollup_health([&unverified, &disabled], as_of()),
            QualificationHealth::Unverified
        );
    }

    /// 状态失效或截止日期已过均视为已过期，且优先于有效资质。
    #[test]
    fn expired_outranks_valid() {
        let expired_status = qualification(
            "auth-exp",
            QualificationType::Authorization,
            QualificationStatus::Expired,
            Some((2025, 1, 1)),
            Some((2026, 12, 31)),
        );
        let expired_date = qualification(
            "lic-past",
            QualificationType::FoodLicense,
            QualificationStatus::Active,
            Some((2025, 1, 1)),
            Some((2026, 8, 30)),
        );
        let valid = qualification(
            "ht-ok",
            QualificationType::Contract,
            QualificationStatus::Active,
            Some((2026, 1, 1)),
            Some((2027, 12, 31)),
        );
        assert_eq!(expired_status.health_on(as_of()), Some(QualificationHealth::Expired));
        assert_eq!(expired_date.health_on(as_of()), Some(QualificationHealth::Expired));
        assert_eq!(
            SupplierQualification::rollup_health([&expired_date, &valid], as_of()),
            QualificationHealth::Expired
        );
    }

    /// 窗口内到期标为即将到期；长期有效或窗口外截止日期标为有效。
    #[test]
    fn expiring_and_valid_windows() {
        let expiring = qualification(
            "ht-soon",
            QualificationType::Contract,
            QualificationStatus::Active,
            Some((2026, 1, 1)),
            Some((2026, 9, 15)),
        );
        let valid = qualification(
            "cert-long",
            QualificationType::Certificate,
            QualificationStatus::Active,
            Some((2026, 1, 1)),
            None,
        );
        let future = qualification(
            "lic-future",
            QualificationType::Certificate,
            QualificationStatus::Active,
            Some((2026, 10, 1)),
            Some((2027, 10, 1)),
        );
        assert_eq!(expiring.health_on(as_of()), Some(QualificationHealth::Expiring30));
        assert_eq!(valid.health_on(as_of()), Some(QualificationHealth::Valid));
        assert_eq!(future.health_on(as_of()), None);
        assert_eq!(
            SupplierQualification::rollup_health([&valid, &expiring], as_of()),
            QualificationHealth::Expiring30
        );
        assert_eq!(
            SupplierQualification::rollup_health([&future], as_of()),
            QualificationHealth::NotRegistered
        );
    }

    /// 已登记类型按稳定代码去重排序，停用记录仍计入类型摘要。
    #[test]
    fn registered_types_are_stable() {
        let contract = qualification(
            "ht-1",
            QualificationType::Contract,
            QualificationStatus::Active,
            Some((2026, 1, 1)),
            Some((2026, 12, 31)),
        );
        let duplicate = qualification(
            "ht-2",
            QualificationType::Contract,
            QualificationStatus::Disabled,
            Some((2026, 1, 1)),
            Some((2026, 12, 31)),
        );
        let license = qualification(
            "food-1",
            QualificationType::FoodLicense,
            QualificationStatus::Active,
            Some((2026, 1, 1)),
            Some((2026, 12, 31)),
        );
        assert_eq!(
            SupplierQualification::registered_types([&license, &duplicate, &contract]),
            vec![QualificationType::Contract, QualificationType::FoodLicense]
        );
    }
}
