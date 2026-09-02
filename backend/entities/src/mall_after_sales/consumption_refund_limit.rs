//! 原消费退款额度计划（INT-R11）。
//!
//! 先按原消费 entry 聚合本请求 `APPLY`/`REVERSE` 净额，再验证
//! 「历史净额 + 本请求净额」不超过原消费金额。不生成 ID、不读写数据库；
//! 跨聚合归属与并发额度占用仍由 Service／Repository 负责。

use std::collections::HashMap;
use std::str::FromStr;

use crate::errors::{Error, Result};
use crate::ids::MallConsumptionEntryId;
use crate::mall_after_sales::AllocationAction;
use crate::mall_order::MallConsumptionEntry;
use crate::money::Amount;

/// 本请求中一条待校验的退款分配额度行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingConsumptionRefund {
    /// 原商品 × 原支付来源消费事实。
    pub original_consumption_entry_id: MallConsumptionEntryId,
    /// 本行分配金额。
    pub amount: Amount,
    /// `APPLY` 增加净占用，`REVERSE` 减少净占用。
    pub action: AllocationAction,
}

/// 按原消费 entry 聚合后的本请求净退款额。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumptionRefundRequestNet {
    /// 原消费事实。
    pub original_consumption_entry_id: MallConsumptionEntryId,
    /// 本请求对该 entry 的净退款额（`APPLY − REVERSE`）。
    pub request_net: Amount,
}

/// 原消费退款额度计划：请求内聚合 + 历史上限校验。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConsumptionRefundLimitPlan {
    request_nets: Vec<ConsumptionRefundRequestNet>,
}

impl ConsumptionRefundLimitPlan {
    /// 按原消费 entry 聚合本请求净额，并校验历史加本次不超过原消费金额。
    ///
    /// # 参数
    /// * `entries` - 已批量装载的原消费事实（按 ID 索引）
    /// * `historical_nets` - 各 entry 历史 `APPLY − REVERSE` 净额；缺项按精确零
    /// * `pending` - 本请求待写入分配（可含同一 entry 的多行与 `REVERSE`）
    ///
    /// # 返回
    /// 返回按首次出现顺序排列的本请求净额计划。
    ///
    /// # 错误
    /// 原消费缺失、金额溢出、累计超过原消费金额时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 纯内存确定性计算；不裁决跨聚合归属，不代替并发 CAS／额度占用。
    pub fn validate(
        entries: &HashMap<MallConsumptionEntryId, MallConsumptionEntry>,
        historical_nets: &HashMap<MallConsumptionEntryId, Amount>,
        pending: &[PendingConsumptionRefund],
    ) -> Result<Self> {
        let request_nets = aggregate_request_nets(pending)?;
        for net in &request_nets {
            let entry = entries.get(&net.original_consumption_entry_id).ok_or_else(|| {
                Error::from(format!("原消费事实不存在: {}", net.original_consumption_entry_id))
            })?;
            let historical = historical_nets
                .get(&net.original_consumption_entry_id)
                .copied()
                .unwrap_or_else(zero_amount);
            let accrued = checked_add(historical, net.request_net)?;
            if !entry.allows_cumulative_refund(accrued) {
                return Err(Error::from(format!(
                    "原消费累计退款不得超过原消费金额: {}",
                    entry.base.id
                )));
            }
        }
        Ok(Self { request_nets })
    }

    /// 返回按首次出现顺序排列的本请求净额切片。
    ///
    /// # 返回
    /// 返回只读净额切片。
    pub fn request_nets(&self) -> &[ConsumptionRefundRequestNet] {
        &self.request_nets
    }
}

/// 按原消费 entry 聚合本请求 `APPLY − REVERSE` 净额。
///
/// # 参数
/// * `pending` - 本请求待写入分配
///
/// # 返回
/// 返回按首次出现顺序的净额列表；空输入返回空列表。
///
/// # 错误
/// 金额运算溢出时返回 [`Error::LogicError`]。
fn aggregate_request_nets(pending: &[PendingConsumptionRefund]) -> Result<Vec<ConsumptionRefundRequestNet>> {
    let mut order: Vec<MallConsumptionEntryId> = Vec::new();
    let mut index: HashMap<String, usize> = HashMap::new();
    let mut nets: Vec<Amount> = Vec::new();
    for line in pending {
        let key = line.original_consumption_entry_id.to_string();
        let position = if let Some(position) = index.get(&key).copied() {
            position
        } else {
            let position = order.len();
            index.insert(key, position);
            order.push(line.original_consumption_entry_id.clone());
            nets.push(zero_amount());
            position
        };
        nets[position] = match line.action {
            AllocationAction::Apply => checked_add(nets[position], line.amount)?,
            AllocationAction::Reverse => checked_sub(nets[position], line.amount)?,
        };
    }
    Ok(order
        .into_iter()
        .zip(nets)
        .map(
            |(original_consumption_entry_id, request_net)| ConsumptionRefundRequestNet {
                original_consumption_entry_id,
                request_net,
            },
        )
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

/// 受检金额减法。
fn checked_sub(left: Amount, right: Amount) -> Result<Amount> {
    let diff = left.to_decimal() - right.to_decimal();
    Amount::try_from(diff).map_err(|error| Error::from(error.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{zero_amount, ConsumptionRefundLimitPlan, PendingConsumptionRefund};
    use crate::common::time::Instant;
    use crate::ids::{
        MallConsumptionEntryId, MallOrderFactId, MallOrderItemId, MallPaymentSourceId, SalesOrderId,
        SalesOrderLineId,
    };
    use crate::mall_after_sales::AllocationAction;
    use crate::mall_order::{
        AttributionStatus, ConsumptionDirection, MallConsumptionEntry, MallConsumptionEntryData,
    };
    use crate::money::Amount;
    use std::collections::HashMap;
    use std::str::FromStr;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn entry(id: &str, amount_value: &str) -> MallConsumptionEntry {
        MallConsumptionEntry::new(
            MallConsumptionEntryId::new(id),
            MallConsumptionEntryData {
                mall_order_fact_id: MallOrderFactId::new("fact-1"),
                mall_order_item_id: MallOrderItemId::new("item-1"),
                mall_payment_source_id: MallPaymentSourceId::new("ps-1"),
                direction: ConsumptionDirection::Consumption,
                amount: amount(amount_value),
                customer_id: None,
                origin_sales_order_id: Some(SalesOrderId::new("so-1")),
                sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
                occurred_at: Instant::from_unix_secs(1_700_000_000),
                attribution_status: AttributionStatus::Attributed,
                reverses_consumption_entry_id: None,
            },
        )
        .unwrap()
    }

    fn pending(entry_id: &str, amount_value: &str, action: AllocationAction) -> PendingConsumptionRefund {
        PendingConsumptionRefund {
            original_consumption_entry_id: MallConsumptionEntryId::new(entry_id),
            amount: amount(amount_value),
            action,
        }
    }

    /// 请求内重复 entry 必须先聚合再与历史上限比较。
    #[test]
    fn duplicate_entries_in_request_are_aggregated_before_limit_check() {
        let entries = HashMap::from([(MallConsumptionEntryId::new("ce-1"), entry("ce-1", "80.00"))]);
        let historical = HashMap::from([(MallConsumptionEntryId::new("ce-1"), amount("10.00"))]);
        let ok = ConsumptionRefundLimitPlan::validate(
            &entries,
            &historical,
            &[
                pending("ce-1", "40.00", AllocationAction::Apply),
                pending("ce-1", "30.00", AllocationAction::Apply),
            ],
        )
        .unwrap();
        assert_eq!(ok.request_nets().len(), 1);
        assert_eq!(ok.request_nets()[0].request_net, amount("70.00"));

        let err = ConsumptionRefundLimitPlan::validate(
            &entries,
            &historical,
            &[
                pending("ce-1", "40.00", AllocationAction::Apply),
                pending("ce-1", "30.01", AllocationAction::Apply),
            ],
        );
        assert!(err.is_err(), "历史 10 + 请求 70.01 超过 80 必须失败");
    }

    /// APPLY/REVERSE 顺序与多次历史净额边界。
    #[test]
    fn apply_reverse_and_history_boundaries() {
        let entries = HashMap::from([(MallConsumptionEntryId::new("ce-1"), entry("ce-1", "100.00"))]);
        let historical = HashMap::from([(MallConsumptionEntryId::new("ce-1"), amount("60.00"))]);
        let plan = ConsumptionRefundLimitPlan::validate(
            &entries,
            &historical,
            &[
                pending("ce-1", "50.00", AllocationAction::Apply),
                pending("ce-1", "10.00", AllocationAction::Reverse),
            ],
        )
        .unwrap();
        assert_eq!(plan.request_nets()[0].request_net, amount("40.00"));

        assert!(ConsumptionRefundLimitPlan::validate(
            &entries,
            &historical,
            &[pending("ce-1", "40.00", AllocationAction::Apply)],
        )
        .is_ok());
        assert!(ConsumptionRefundLimitPlan::validate(
            &entries,
            &historical,
            &[pending("ce-1", "40.01", AllocationAction::Apply)],
        )
        .is_err());
    }

    /// 缺失 entry 与无历史（精确零）路径。
    #[test]
    fn missing_entry_fails_and_absent_history_is_exact_zero() {
        let entries = HashMap::from([(MallConsumptionEntryId::new("ce-1"), entry("ce-1", "50.00"))]);
        let historical = HashMap::new();
        assert!(ConsumptionRefundLimitPlan::validate(
            &entries,
            &historical,
            &[pending("ce-1", "50.00", AllocationAction::Apply)],
        )
        .is_ok());
        assert_eq!(
            historical
                .get(&MallConsumptionEntryId::new("ce-1"))
                .copied()
                .unwrap_or_else(zero_amount),
            amount("0.00")
        );
        assert!(ConsumptionRefundLimitPlan::validate(
            &entries,
            &historical,
            &[pending("ce-missing", "1.00", AllocationAction::Apply)],
        )
        .is_err());
    }
}
