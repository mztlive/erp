//! 余额恢复额度计划（INT-R12）。
//!
//! 先按原退款 allocation 聚合本请求恢复净额，再验证历史加本次不超过可恢复额。
//! 不生成 ID、不读写数据库；same-case、card 归属与并发额度占用仍由 Service
//! ／Repository 负责。

use std::collections::HashMap;
use std::str::FromStr;

use crate::errors::{Error, Result};
use crate::ids::MallRefundAllocationId;
use crate::mall_after_sales::MallRefundAllocation;
use crate::money::Amount;

/// 本请求中一条待校验的余额恢复分配额度行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingRestorationRefundAllocation {
    /// 原 CARD 退款资金分配。
    pub mall_refund_allocation_id: MallRefundAllocationId,
    /// 本行恢复金额。
    pub amount: Amount,
}

/// 按原退款 allocation 聚合后的本请求恢复净额。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorationRequestNet {
    /// 原 CARD 退款资金分配。
    pub mall_refund_allocation_id: MallRefundAllocationId,
    /// 本请求对该 allocation 的恢复合计。
    pub request_net: Amount,
}

/// 余额恢复额度计划：请求内聚合 + 历史上限校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestorationLimitPlan {
    request_nets: Vec<RestorationRequestNet>,
}

impl RestorationLimitPlan {
    /// 按原退款 allocation 聚合本请求恢复额，并校验历史加本次不超过可恢复额。
    ///
    /// # 参数
    /// * `refund_allocations` - 已批量装载的原退款分配（按 ID 索引）
    /// * `historical_restored` - 各 allocation 历史已恢复合计；缺项按精确零
    /// * `pending` - 本请求待写入恢复分配（可含同一 allocation 的多行）
    ///
    /// # 返回
    /// 返回按首次出现顺序排列的本请求恢复净额计划。
    ///
    /// # 错误
    /// 原分配缺失、非可恢复 `APPLY`、金额溢出或累计超过可恢复额时返回
    /// [`Error::LogicError`]。
    ///
    /// # 约束
    /// 纯内存确定性计算；不裁决 same-case／card 归属，不代替并发 CAS。
    pub fn validate(
        refund_allocations: &HashMap<MallRefundAllocationId, MallRefundAllocation>,
        historical_restored: &HashMap<MallRefundAllocationId, Amount>,
        pending: &[PendingRestorationRefundAllocation],
    ) -> Result<Self> {
        let request_nets = aggregate_request_nets(pending)?;
        for net in &request_nets {
            let allocation = refund_allocations
                .get(&net.mall_refund_allocation_id)
                .ok_or_else(|| Error::from(format!("原退款分配不存在: {}", net.mall_refund_allocation_id)))?;
            if !allocation.is_restorable_apply() {
                return Err(Error::from("余额恢复只能引用净有效的 APPLY 退款分配"));
            }
            let historical = historical_restored
                .get(&net.mall_refund_allocation_id)
                .copied()
                .unwrap_or_else(zero_amount);
            let accrued = checked_add(historical, net.request_net)?;
            if !allocation.allows_cumulative_restoration(accrued) {
                return Err(Error::from("累计恢复金额不得超过对应 CARD 退款净额"));
            }
        }
        Ok(Self { request_nets })
    }

    /// 返回按首次出现顺序排列的本请求恢复净额切片。
    ///
    /// # 返回
    /// 返回只读净额切片。
    pub fn request_nets(&self) -> &[RestorationRequestNet] {
        &self.request_nets
    }
}

/// 按原退款 allocation 聚合本请求恢复合计。
///
/// # 参数
/// * `pending` - 本请求待写入恢复分配
///
/// # 返回
/// 返回按首次出现顺序的合计列表；空输入返回空列表。
///
/// # 错误
/// 金额运算溢出时返回 [`Error::LogicError`]。
fn aggregate_request_nets(
    pending: &[PendingRestorationRefundAllocation],
) -> Result<Vec<RestorationRequestNet>> {
    let mut order: Vec<MallRefundAllocationId> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut nets: Vec<Amount> = Vec::new();
    for line in pending {
        let key = line.mall_refund_allocation_id.to_string();
        let position = if let Some(position) = index.get(&key).copied() {
            position
        } else {
            let position = order.len();
            index.insert(key, position);
            order.push(line.mall_refund_allocation_id.clone());
            nets.push(zero_amount());
            position
        };
        nets[position] = checked_add(nets[position], line.amount)?;
    }
    Ok(order
        .into_iter()
        .zip(nets)
        .map(|(mall_refund_allocation_id, request_net)| RestorationRequestNet {
            mall_refund_allocation_id,
            request_net,
        })
        .collect())
}

/// 返回精确零金额。
fn zero_amount() -> Amount {
    Amount::from_str("0.00").expect("固定零金额必须可解析")
}

/// 受检金额加法。
fn checked_add(left: Amount, right: Amount) -> Result<Amount> {
    let sum = left.to_decimal() + right.to_decimal();
    Amount::try_from(sum).map_err(|error| Error::from(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{zero_amount, PendingRestorationRefundAllocation, RestorationLimitPlan};
    use crate::ids::{MallConsumptionEntryId, MallPaymentSourceId, MallRefundAllocationId, MallRefundLineId};
    use crate::mall_after_sales::{AllocationAction, MallRefundAllocation, MallRefundAllocationData};
    use crate::money::Amount;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn apply_allocation(id: &str, refund_amount: &str) -> MallRefundAllocation {
        MallRefundAllocation::new(
            MallRefundAllocationId::new(id),
            MallRefundAllocationData {
                mall_refund_line_id: MallRefundLineId::new("rl-1"),
                allocation_no: 1,
                original_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
                original_payment_source_id: MallPaymentSourceId::new("ps-1"),
                allocated_refund_amount: amount(refund_amount),
                allocation_action: AllocationAction::Apply,
                reverses_allocation_id: None,
                reversal_consumption_entry_id: Some(MallConsumptionEntryId::new("ce-rev-1")),
            },
        )
        .unwrap()
    }

    fn reverse_allocation(id: &str) -> MallRefundAllocation {
        MallRefundAllocation::new(
            MallRefundAllocationId::new(id),
            MallRefundAllocationData {
                mall_refund_line_id: MallRefundLineId::new("rl-1"),
                allocation_no: 2,
                original_consumption_entry_id: MallConsumptionEntryId::new("ce-1"),
                original_payment_source_id: MallPaymentSourceId::new("ps-1"),
                allocated_refund_amount: amount("10.00"),
                allocation_action: AllocationAction::Reverse,
                reverses_allocation_id: Some(MallRefundAllocationId::new("ra-1")),
                reversal_consumption_entry_id: Some(MallConsumptionEntryId::new("ce-rev-2")),
            },
        )
        .unwrap()
    }

    fn pending(id: &str, amount_value: &str) -> PendingRestorationRefundAllocation {
        PendingRestorationRefundAllocation {
            mall_refund_allocation_id: MallRefundAllocationId::new(id),
            amount: amount(amount_value),
        }
    }

    /// 请求内重复 allocation 必须先聚合再校验上限。
    #[test]
    fn duplicate_allocations_in_request_are_aggregated_before_limit_check() {
        let allocations = HashMap::from([(
            MallRefundAllocationId::new("ra-1"),
            apply_allocation("ra-1", "80.00"),
        )]);
        let historical = HashMap::from([(MallRefundAllocationId::new("ra-1"), amount("10.00"))]);
        let ok = RestorationLimitPlan::validate(
            &allocations,
            &historical,
            &[pending("ra-1", "40.00"), pending("ra-1", "30.00")],
        )
        .unwrap();
        assert_eq!(ok.request_nets().len(), 1);
        assert_eq!(ok.request_nets()[0].request_net, amount("70.00"));

        assert!(RestorationLimitPlan::validate(
            &allocations,
            &historical,
            &[pending("ra-1", "40.00"), pending("ra-1", "30.01")],
        )
        .is_err());
    }

    /// 多 allocation、等于/超过上限、REVERSE 不可恢复、缺失关联。
    #[test]
    fn multi_allocation_boundaries_and_missing_or_reverse() {
        let allocations = HashMap::from([
            (
                MallRefundAllocationId::new("ra-1"),
                apply_allocation("ra-1", "50.00"),
            ),
            (
                MallRefundAllocationId::new("ra-2"),
                apply_allocation("ra-2", "30.00"),
            ),
            (MallRefundAllocationId::new("ra-r"), reverse_allocation("ra-r")),
        ]);
        let historical = HashMap::new();
        let plan = RestorationLimitPlan::validate(
            &allocations,
            &historical,
            &[pending("ra-1", "50.00"), pending("ra-2", "30.00")],
        )
        .unwrap();
        assert_eq!(plan.request_nets().len(), 2);

        assert!(
            RestorationLimitPlan::validate(&allocations, &historical, &[pending("ra-1", "50.01")],).is_err()
        );
        assert!(
            RestorationLimitPlan::validate(&allocations, &historical, &[pending("ra-r", "1.00")],).is_err()
        );
        assert!(
            RestorationLimitPlan::validate(&allocations, &historical, &[pending("ra-missing", "1.00")],)
                .is_err()
        );
        assert_eq!(
            historical
                .get(&MallRefundAllocationId::new("ra-1"))
                .copied()
                .unwrap_or_else(zero_amount),
            amount("0.00")
        );
    }
}
