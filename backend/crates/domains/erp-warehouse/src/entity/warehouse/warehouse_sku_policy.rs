//! `warehouse_sku_policy` 仓库-SKU 库存预警策略（数据模型 §6.3）。
//!
//! 只生成预警，不是库存事实：不得写 `on_hand`、`reserved` 或商城缓存库存。
//! 同一仓库和 SKU 的启用区间不得重叠；区间与冲突判定由本实体和值对象统一实现，
//! Service 只负责加载既有策略并编排持久化。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::state::ensure_transition;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SkuId, WarehouseId, WarehouseSkuPolicyId};
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::status::EnableStatus;

/// 仓库-SKU 策略的半开生效区间。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseSkuPolicyPeriod {
    /// 生效开始日（含）。
    effective_from: BusinessDate,
    /// 生效结束日（不含）；空表示无限期。
    effective_to: Option<BusinessDate>,
}

impl WarehouseSkuPolicyPeriod {
    /// 构造供仓库策略使用的半开区间，包含开始日、不包含结束日，`None` 表示无上界；结束日不晚于开始日时拒绝。
    pub fn new(effective_from: BusinessDate, effective_to: Option<BusinessDate>) -> Result<Self> {
        if effective_to.is_some_and(|effective_to| effective_to <= effective_from) {
            return Err(Error::from("生效结束日必须晚于生效开始日"));
        }
        Ok(Self { effective_from, effective_to })
    }

    /// 按半开区间的统一规则判断业务日是否生效，结束日当天不命中，未配置结束日则不设上界。
    pub fn contains(self, business_day: BusinessDate) -> bool {
        business_day >= self.effective_from
            && self.effective_to.is_none_or(|effective_to| business_day < effective_to)
    }

    /// 判断两个半开区间是否共享至少一个业务日，避免把首尾相接的策略当成重叠。
    pub fn overlaps(self, other: Self) -> bool {
        // 非退化半开区间相交当且仅当一方的起点落在另一方内。
        self.contains(other.effective_from) || other.contains(self.effective_from)
    }

    /// 提供包含在生效区间内的起始日，供策略调期和存储使用。
    pub fn effective_from(self) -> BusinessDate {
        self.effective_from
    }

    /// 提供不包含在生效区间内的结束日；`None` 表示无限期。
    pub fn effective_to(self) -> Option<BusinessDate> {
        self.effective_to
    }
}

/// 仓库-SKU 预警策略创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseSkuPolicyData {
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 最低可用量预警阈值（定点数，非负）。
    pub minimum_available_quantity: Quantity,
    /// 启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
}

/// 仓库-SKU 预警策略更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WarehouseSkuPolicyUpdate {
    /// 最低可用量预警阈值；`None` 表示不修改。
    pub minimum_available_quantity: Option<Quantity>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 仓库-SKU 预警策略实体（数据模型 §6.3，只用 `BaseModel` 持久化元数据）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct WarehouseSkuPolicy {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 最低可用量预警阈值。
    pub minimum_available_quantity: Quantity,
    /// 启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示无限期。
    pub effective_to: Option<BusinessDate>,
}

impl WarehouseSkuPolicy {
    /// 创建指定仓库与 SKU 的预警策略；负数阈值、结束日等于或早于开始日都会被拒绝，避免保存无法执行的规则。
    pub fn new(id: WarehouseSkuPolicyId, data: WarehouseSkuPolicyData) -> Result<Self> {
        ensure_non_negative_quantity(data.minimum_available_quantity)?;
        WarehouseSkuPolicyPeriod::new(data.effective_from, data.effective_to)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            warehouse_id: data.warehouse_id,
            sku_id: data.sku_id,
            minimum_available_quantity: data.minimum_available_quantity,
            status: data.status,
            effective_from: data.effective_from,
            effective_to: data.effective_to,
        })
    }

    /// 修改预警阈值和启停状态，不改变作为身份的仓库与 SKU。负数阈值或公共状态机拒绝的迁移返回错误。
    pub fn update(&mut self, update: WarehouseSkuPolicyUpdate) -> Result<()> {
        if let Some(minimum_available_quantity) = update.minimum_available_quantity {
            ensure_non_negative_quantity(minimum_available_quantity)?;
            self.minimum_available_quantity = minimum_available_quantity;
        }

        if let Some(status) = update.status {
            ensure_transition(self.status, status)?;
            self.status = status;
        }
        Ok(())
    }

    /// 将已持久化的起止日作为半开区间交给比较逻辑，此处不重新验证日期顺序。
    pub fn effective_period(&self) -> WarehouseSkuPolicyPeriod {
        WarehouseSkuPolicyPeriod { effective_from: self.effective_from, effective_to: self.effective_to }
    }

    /// 只有启用策略才参与同维度防重叠约束，停用策略不构成冲突。
    pub fn is_operable(&self) -> bool {
        self.status.is_active()
    }

    /// 比较不同 ID 的策略，只有同仓库、同 SKU、均启用且区间重叠时才构成冲突，更新时不和自身比较。
    pub fn conflicts_with(&self, other: &Self) -> bool {
        self.base.id != other.base.id
            && self.warehouse_id == other.warehouse_id
            && self.sku_id == other.sku_id
            && self.is_operable()
            && other.is_operable()
            && self.effective_period().overlaps(other.effective_period())
    }

    /// 对既有策略集逐项核对启用区间，遇到同仓库同 SKU 的重叠便拒绝；调用方须提供该维度的完整集合。
    pub fn ensure_no_overlap(&self, existing: &[Self]) -> Result<()> {
        if existing.iter().any(|policy| self.conflicts_with(policy)) {
            return Err(Error::from("同一仓库和SKU的启用区间不得重叠"));
        }
        Ok(())
    }

    /// 核对策略当前乐观锁版本与调用方期望是否一致，供更新前识别并发变化。
    pub fn matches_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 调整策略的半开生效区间，结束日缺失时不设上界；结束日不晚于开始日时返回错误。跨记录的启用区间重叠由用例在事务内核查。
    pub fn reschedule(
        &mut self,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
    ) -> Result<()> {
        let period = WarehouseSkuPolicyPeriod::new(effective_from, effective_to)?;
        self.effective_from = period.effective_from();
        self.effective_to = period.effective_to();
        Ok(())
    }
}

// 统一新建和更新的定点阈值校验，预警数量为负时拒绝，零允许。
fn ensure_non_negative_quantity(value: Quantity) -> Result<()> {
    if value.to_decimal().is_sign_negative() {
        return Err(Error::from("预警阈值不能为负数"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::state::{assert_adjacency_closed, ensure_transition};
    use erp_core::ids::WarehouseSkuPolicyId;

    use super::*;

    fn data() -> WarehouseSkuPolicyData {
        WarehouseSkuPolicyData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            minimum_available_quantity: Quantity::from_str("10.000000").unwrap(),
            status: EnableStatus::Active,
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: None,
        }
    }

    // 验证新策略保存给定的定点阈值和生效日期，供预警判定按原值执行。
    #[test]
    fn new_normalizes_policy() {
        let policy = WarehouseSkuPolicy::new(WarehouseSkuPolicyId::new("policy-1"), data()).unwrap();

        assert_eq!(policy.warehouse_id, WarehouseId::new("wh-1"));
        assert_eq!(policy.sku_id, SkuId::new("sku-1"));
        assert_eq!(policy.minimum_available_quantity, Quantity::from_str("10.000000").unwrap());
        assert!(policy.status.is_active());
    }

    // 验证半开区间包含开始日、排除结束日，且无限期策略不受上界限制。
    #[test]
    fn period_contains_is_half_open() {
        let period = WarehouseSkuPolicyPeriod::new(
            BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            Some(BusinessDate::from_ymd(2026, 3, 1).unwrap()),
        )
        .unwrap();
        assert!(period.contains(BusinessDate::from_ymd(2026, 1, 1).unwrap()));
        assert!(period.contains(BusinessDate::from_ymd(2026, 2, 1).unwrap()));
        assert!(!period.contains(BusinessDate::from_ymd(2026, 3, 1).unwrap()));
        assert!(!period.contains(BusinessDate::from_ymd(2025, 12, 31).unwrap()));

        let open_ended =
            WarehouseSkuPolicyPeriod::new(BusinessDate::from_ymd(2026, 1, 1).unwrap(), None).unwrap();
        assert!(open_ended.contains(BusinessDate::from_ymd(2030, 1, 1).unwrap()));
        assert!(!open_ended.contains(BusinessDate::from_ymd(2025, 12, 31).unwrap()));
    }

    // 验证只有不同记录、同仓库同 SKU、均启用且实际重叠才冲突，相邻区间可以共存。
    #[test]
    fn effective_period_overlap_respects_boundaries_and_status() {
        let first = WarehouseSkuPolicy::new(
            WarehouseSkuPolicyId::new("policy-1"),
            WarehouseSkuPolicyData {
                effective_to: Some(BusinessDate::from_ymd(2026, 3, 1).unwrap()),
                ..data()
            },
        )
        .unwrap();
        let overlapping = WarehouseSkuPolicy::new(
            WarehouseSkuPolicyId::new("policy-2"),
            WarehouseSkuPolicyData {
                effective_from: BusinessDate::from_ymd(2026, 2, 1).unwrap(),
                effective_to: Some(BusinessDate::from_ymd(2026, 4, 1).unwrap()),
                ..data()
            },
        )
        .unwrap();
        let adjacent = WarehouseSkuPolicy::new(
            WarehouseSkuPolicyId::new("policy-3"),
            WarehouseSkuPolicyData {
                effective_from: BusinessDate::from_ymd(2026, 3, 1).unwrap(),
                effective_to: Some(BusinessDate::from_ymd(2026, 4, 1).unwrap()),
                ..data()
            },
        )
        .unwrap();
        let disabled = WarehouseSkuPolicy::new(
            WarehouseSkuPolicyId::new("policy-4"),
            WarehouseSkuPolicyData {
                status: EnableStatus::Disabled,
                effective_from: BusinessDate::from_ymd(2026, 2, 1).unwrap(),
                effective_to: Some(BusinessDate::from_ymd(2026, 4, 1).unwrap()),
                ..data()
            },
        )
        .unwrap();

        assert!(first.conflicts_with(&overlapping));
        assert!(first.ensure_no_overlap(&[overlapping]).is_err());
        assert!(!first.conflicts_with(&adjacent), "相邻半开区间不重叠");
        assert!(!first.conflicts_with(&disabled), "停用策略不参与启用区间冲突");
        assert!(first.ensure_no_overlap(&[adjacent, disabled]).is_ok());
    }

    // 验证负预警阈值和倒挂的生效日期都不能建立策略。
    #[test]
    fn new_rejects_negative_threshold_and_reversed_window() {
        let negative = WarehouseSkuPolicyData {
            minimum_available_quantity: Quantity::from_str("-0.100000").unwrap(),
            ..data()
        };
        assert!(WarehouseSkuPolicy::new(WarehouseSkuPolicyId::new("policy-1"), negative).is_err());

        let reversed = WarehouseSkuPolicyData {
            effective_from: BusinessDate::from_ymd(2026, 3, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 2, 1).unwrap()),
            ..data()
        };
        assert!(WarehouseSkuPolicy::new(WarehouseSkuPolicyId::new("policy-1"), reversed).is_err());
    }

    // 验证预警阈值保持定点精度、拒绝过多小数位，JSON 使用字符串存储其文本形态。
    #[test]
    fn threshold_is_fixed_point_with_string_wire_shape() {
        assert!(Quantity::from_str("10.0000001").is_err());

        let policy = WarehouseSkuPolicy::new(WarehouseSkuPolicyId::new("policy-1"), data()).unwrap();
        let json = serde_json::to_string(&policy).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(value["minimum_available_quantity"], serde_json::json!("10.000000"));

        let back: WarehouseSkuPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, policy);
    }

    // 验证更新只改阈值和状态，生效区间由独立调期入口修改。
    #[test]
    fn update_and_reschedule_apply_fields() {
        let mut policy = WarehouseSkuPolicy::new(WarehouseSkuPolicyId::new("policy-1"), data()).unwrap();
        policy.base.version = 2;
        assert!(policy.matches_version(2));
        assert!(!policy.matches_version(1));
        assert!(policy.is_operable());

        policy
            .update(WarehouseSkuPolicyUpdate {
                minimum_available_quantity: Some(Quantity::from_str("5.000000").unwrap()),
                status: Some(EnableStatus::Disabled),
            })
            .unwrap();
        assert_eq!(policy.minimum_available_quantity, Quantity::from_str("5.000000").unwrap());
        assert!(!policy.status.is_active());
        assert!(!policy.is_operable());

        policy
            .reschedule(
                BusinessDate::from_ymd(2026, 4, 1).unwrap(),
                Some(BusinessDate::from_ymd(2026, 6, 1).unwrap()),
            )
            .unwrap();
        assert_eq!(policy.effective_from, BusinessDate::from_ymd(2026, 4, 1).unwrap());
        assert_eq!(policy.effective_to, Some(BusinessDate::from_ymd(2026, 6, 1).unwrap()));

        assert!(
            policy
                .reschedule(
                    BusinessDate::from_ymd(2026, 6, 1).unwrap(),
                    Some(BusinessDate::from_ymd(2026, 5, 1).unwrap())
                )
                .is_err()
        );

        assert!(
            policy
                .update(WarehouseSkuPolicyUpdate {
                    minimum_available_quantity: Some(Quantity::from_str("-1.000000").unwrap()),
                    ..Default::default()
                })
                .is_err()
        );
    }

    // 验证策略启停沿用公共状态图，切换与同状态重复操作均有确定语义。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
