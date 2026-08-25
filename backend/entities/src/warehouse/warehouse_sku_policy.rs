//! `warehouse_sku_policy` 仓库-SKU 库存预警策略（数据模型 §6.3）。
//!
//! 只生成预警，不是库存事实：不得写 `on_hand`、`reserved` 或商城缓存库存。
//! 同一仓库和 SKU 的启用区间不得重叠；区间与冲突判定由本实体和值对象统一实现，
//! Service 只负责加载既有策略并编排持久化。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::ensure_transition;
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::{SkuId, WarehouseId, WarehouseSkuPolicyId};
use crate::money::Quantity;
use crate::warehouse::status::EnableStatus;

/// 仓库-SKU 策略的半开生效区间。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseSkuPolicyPeriod {
    /// 生效开始日（含）。
    effective_from: BusinessDate,
    /// 生效结束日（不含）；空表示无限期。
    effective_to: Option<BusinessDate>,
}

impl WarehouseSkuPolicyPeriod {
    /// 构造仓库-SKU 策略生效区间。
    ///
    /// # 参数
    /// * `effective_from` - 生效开始日（含）
    /// * `effective_to` - 生效结束日（不含）；空表示无限期
    ///
    /// # 返回
    /// 返回合法的半开区间值对象。
    ///
    /// # 错误
    /// 结束日不晚于开始日时返回错误。
    pub fn new(effective_from: BusinessDate, effective_to: Option<BusinessDate>) -> Result<Self> {
        if effective_to.is_some_and(|effective_to| effective_to <= effective_from) {
            return Err(Error::from("生效结束日必须晚于生效开始日"));
        }
        Ok(Self {
            effective_from,
            effective_to,
        })
    }

    /// 判断两个半开区间是否存在交集。
    ///
    /// # 参数
    /// * `other` - 另一个策略生效区间
    ///
    /// # 返回
    /// 两个区间存在至少一个共同业务日时返回 `true`；相邻区间返回 `false`。
    pub fn overlaps(self, other: Self) -> bool {
        self.effective_to.is_none_or(|end| other.effective_from < end)
            && other.effective_to.is_none_or(|end| self.effective_from < end)
    }

    /// 返回生效开始日。
    ///
    /// # 返回
    /// 返回半开区间的包含端点。
    pub fn effective_from(self) -> BusinessDate {
        self.effective_from
    }

    /// 返回生效结束日。
    ///
    /// # 返回
    /// 返回不包含的结束日；`None` 表示无限期。
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
    /// 创建仓库-SKU 预警策略。
    ///
    /// 完成预警阈值非负校验与生效区间校验（结束日晚于开始日）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::WarehouseSkuPolicyId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的策略实体。
    ///
    /// # 错误
    /// 当预警阈值为负数或生效区间倒挂时返回错误。
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

    /// 更新仓库-SKU 预警策略。
    ///
    /// 复用 `new` 的校验规则；`warehouse_id`/`sku_id` 是策略身份，不允许修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当预警阈值为负数时返回错误。
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

    /// 返回当前策略的半开生效区间。
    ///
    /// # 返回
    /// 返回由实体持久化起止日构造的合法区间值对象。
    pub fn effective_period(&self) -> WarehouseSkuPolicyPeriod {
        WarehouseSkuPolicyPeriod {
            effective_from: self.effective_from,
            effective_to: self.effective_to,
        }
    }

    /// 判断当前策略是否仍参与启用策略约束。
    ///
    /// # 返回
    /// 策略状态为启用时返回 `true`。
    pub fn is_operable(&self) -> bool {
        self.status.is_active()
    }

    /// 判断两个策略是否构成同维度启用区间冲突。
    ///
    /// # 参数
    /// * `other` - 待比较的另一条策略
    ///
    /// # 返回
    /// 两者不同主键、同仓库、同 SKU、均启用且区间重叠时返回 `true`。
    pub fn conflicts_with(&self, other: &Self) -> bool {
        self.base.id != other.base.id
            && self.warehouse_id == other.warehouse_id
            && self.sku_id == other.sku_id
            && self.is_operable()
            && other.is_operable()
            && self.effective_period().overlaps(other.effective_period())
    }

    /// 校验当前策略不与既有策略的启用区间重叠。
    ///
    /// # 参数
    /// * `existing` - 同维度或批量加载的既有策略
    ///
    /// # 返回
    /// 不存在冲突时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一同仓库、同 SKU 的启用策略区间重叠时返回错误。
    pub fn ensure_no_overlap(&self, existing: &[Self]) -> Result<()> {
        if existing.iter().any(|policy| self.conflicts_with(policy)) {
            return Err(Error::from("同一仓库和SKU的启用区间不得重叠"));
        }
        Ok(())
    }

    /// 判断当前乐观锁版本是否与期望版本一致。
    ///
    /// # 参数
    /// * `expected` - 调用方读取后携带的期望版本
    ///
    /// # 返回
    /// 当前版本等于期望版本时返回 `true`。
    pub fn matches_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 调整策略生效区间。
    ///
    /// 用于策略重排；「同一仓库和 SKU 的启用区间不得重叠」需要跨行校验，
    /// 由 P3 服务层在事务内完成（数据模型 §6.3）。
    ///
    /// # 参数
    /// * `effective_from` - 新的生效开始日
    /// * `effective_to` - 新的生效结束日；空表示无限期
    ///
    /// # 返回
    /// 调整成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当生效区间倒挂时返回错误。
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

/// 校验预警阈值为非负定点数量。
///
/// # 参数
/// * `value` - 预警阈值
///
/// # 返回
/// 非负时返回 `Ok(())`。
///
/// # 错误
/// 为负数时返回错误。
fn ensure_non_negative_quantity(value: Quantity) -> Result<()> {
    if value.to_decimal().is_sign_negative() {
        return Err(Error::from("预警阈值不能为负数"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::WarehouseSkuPolicyId;
    use std::str::FromStr;

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

    /// happy path：阈值与区间落位。
    #[test]
    fn new_normalizes_policy() {
        let policy = WarehouseSkuPolicy::new(WarehouseSkuPolicyId::new("policy-1"), data()).unwrap();

        assert_eq!(policy.warehouse_id, WarehouseId::new("wh-1"));
        assert_eq!(policy.sku_id, SkuId::new("sku-1"));
        assert_eq!(
            policy.minimum_available_quantity,
            Quantity::from_str("10.000000").unwrap()
        );
        assert!(policy.status.is_active());
    }

    /// 半开区间与启用状态共同决定同维度策略冲突。
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

    /// 失败路径：越界（负阈值）与关联不一致（生效区间倒挂）各一条。
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

    /// 金额/数量：定点类型拒绝超位小数，阈值 JSON 形态为字符串。
    #[test]
    fn threshold_is_fixed_point_with_string_wire_shape() {
        assert!(Quantity::from_str("10.0000001").is_err());

        let policy = WarehouseSkuPolicy::new(WarehouseSkuPolicyId::new("policy-1"), data()).unwrap();
        let json = serde_json::to_string(&policy).unwrap();
        let value: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(
            value["minimum_available_quantity"],
            serde_json::json!("10.000000")
        );

        let back: WarehouseSkuPolicy = serde_json::from_str(&json).unwrap();
        assert_eq!(back, policy);
    }

    /// update 修改阈值与状态；reschedule 调整区间。
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
        assert_eq!(
            policy.minimum_available_quantity,
            Quantity::from_str("5.000000").unwrap()
        );
        assert!(!policy.status.is_active());
        assert!(!policy.is_operable());

        policy
            .reschedule(
                BusinessDate::from_ymd(2026, 4, 1).unwrap(),
                Some(BusinessDate::from_ymd(2026, 6, 1).unwrap()),
            )
            .unwrap();
        assert_eq!(policy.effective_from, BusinessDate::from_ymd(2026, 4, 1).unwrap());
        assert_eq!(
            policy.effective_to,
            Some(BusinessDate::from_ymd(2026, 6, 1).unwrap())
        );

        assert!(policy
            .reschedule(
                BusinessDate::from_ymd(2026, 6, 1).unwrap(),
                Some(BusinessDate::from_ymd(2026, 5, 1).unwrap())
            )
            .is_err());
        assert!(policy
            .update(WarehouseSkuPolicyUpdate {
                minimum_available_quantity: Some(Quantity::from_str("-1.000000").unwrap()),
                ..Default::default()
            })
            .is_err());
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
