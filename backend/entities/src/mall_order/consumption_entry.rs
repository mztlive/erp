//! `mall_consumption_entry`：消费事实与消费冲减（数据模型 §6.17）。
//!
//! 消费事实不可变（§4.5）：退款按原商品和原支付来源追加反向消费，不覆盖原消费。
//! `occurred_at` 必须等于所引 `mall_order_fact.occurred_at`，属跨实体一致性，
//! 由 P3 事务校验（P3 条目：§6.17 消费时间与事实一致）；卡券消费归原销售单、
//! 微信支付不挂企业卡券收入归属同样依赖支付来源与卡实例的跨实体查询
//! （P3 条目：§6.17 消费归属规则）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::ensure_transition;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    CustomerAccountId, MallConsumptionEntryId, MallOrderFactId, MallOrderItemId, MallPaymentSourceId,
    SalesOrderId, SalesOrderLineId,
};
use crate::mall_order::types::AttributionStatus;
use crate::money::Amount;

/// 消费方向（数据模型 §6.17：消费或消费冲减）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConsumptionDirection {
    /// 消费。
    Consumption,
    /// 消费冲减（退款按原商品、原支付来源追加的反向消费）。
    ConsumptionReversal,
}

impl ConsumptionDirection {
    /// 返回方向的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Consumption => "消费",
            Self::ConsumptionReversal => "消费冲减",
        }
    }

    /// 返回方向的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Consumption => "consumption",
            Self::ConsumptionReversal => "consumption_reversal",
        }
    }

    /// 判断是否为消费冲减。
    ///
    /// # 返回
    /// 方向为 `ConsumptionReversal` 时返回 `true`。
    pub fn is_reversal(&self) -> bool {
        matches!(self, Self::ConsumptionReversal)
    }
}

/// 消费事实创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionEntryData {
    /// 支付或退款事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 资金来源。
    pub mall_payment_source_id: MallPaymentSourceId,
    /// 消费或消费冲减。
    pub direction: ConsumptionDirection,
    /// 本来源消费金额。
    pub amount: Amount,
    /// 卡券经营归属：企业客户。
    pub customer_id: Option<CustomerAccountId>,
    /// 卡券经营归属：原销售单。
    pub origin_sales_order_id: Option<SalesOrderId>,
    /// 卡券经营归属：销售单唯一卡券明细。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 不可变业务发生时间。
    pub occurred_at: Instant,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 退款冲减原消费。
    pub reverses_consumption_entry_id: Option<MallConsumptionEntryId>,
}

/// 消费事实实体（数据模型 §6.17）。
///
/// 同一业务事实、商品明细、支付来源和方向唯一由 P2 唯一索引落实。消费不可变，
/// 只提供 `new()` 与归集进度推进。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallConsumptionEntry {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 支付或退款事实。
    pub mall_order_fact_id: MallOrderFactId,
    /// 商品明细。
    pub mall_order_item_id: MallOrderItemId,
    /// 资金来源。
    pub mall_payment_source_id: MallPaymentSourceId,
    /// 消费或消费冲减。
    pub direction: ConsumptionDirection,
    /// 本来源消费金额。
    pub amount: Amount,
    /// 卡券经营归属：企业客户。
    pub customer_id: Option<CustomerAccountId>,
    /// 卡券经营归属：原销售单。
    pub origin_sales_order_id: Option<SalesOrderId>,
    /// 卡券经营归属：销售单唯一卡券明细。
    pub sales_order_line_id: Option<SalesOrderLineId>,
    /// 不可变业务发生时间。
    pub occurred_at: Instant,
    /// 归集进度状态。
    pub attribution_status: AttributionStatus,
    /// 退款冲减原消费。
    pub reverses_consumption_entry_id: Option<MallConsumptionEntryId>,
}

impl MallConsumptionEntry {
    /// 创建消费事实。
    ///
    /// 强制两条不变式（§6.17）：金额必须大于零；`ConsumptionReversal` 必须携带
    /// `reverses_consumption_entry_id`（退款冲减原消费），`Consumption` 不得携带。
    /// 文本字段无（ID 均类型化）；`occurred_at` 与所引事实一致由 P3 校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallConsumptionEntryId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的消费事实实体。
    ///
    /// # 错误
    /// 当金额非正或冲减引用与方向不一致时返回错误。
    pub fn new(id: MallConsumptionEntryId, data: MallConsumptionEntryData) -> Result<Self> {
        if data.amount.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("消费金额必须大于零"));
        }
        if data.direction.is_reversal() != data.reverses_consumption_entry_id.is_some() {
            return Err(Error::from("消费冲减必须引用原消费，正向消费不得携带冲减引用"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_order_fact_id: data.mall_order_fact_id,
            mall_order_item_id: data.mall_order_item_id,
            mall_payment_source_id: data.mall_payment_source_id,
            direction: data.direction,
            amount: data.amount,
            customer_id: data.customer_id,
            origin_sales_order_id: data.origin_sales_order_id,
            sales_order_line_id: data.sales_order_line_id,
            occurred_at: data.occurred_at,
            attribution_status: data.attribution_status,
            reverses_consumption_entry_id: data.reverses_consumption_entry_id,
        })
    }

    /// 推进归集进度状态。
    ///
    /// 固定邻接（§6.17）：待归集 → 已归集 | 差异；差异 → 待归集；已归集为终态。
    ///
    /// # 参数
    /// * `to` - 目标归集状态
    ///
    /// # 返回
    /// 迁移合法返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标不在后继列表中且与当前状态不同时返回 `InvalidStateTransition`。
    pub fn update_attribution_status(&mut self, to: AttributionStatus) -> Result<()> {
        ensure_transition(self.attribution_status, to)?;
        self.attribution_status = to;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{ConsumptionDirection, MallConsumptionEntry, MallConsumptionEntryData};
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::{
        MallConsumptionEntryId, MallOrderFactId, MallOrderItemId, MallPaymentSourceId, SalesOrderId,
        SalesOrderLineId,
    };
    use crate::mall_order::types::AttributionStatus;
    use crate::money::Amount;
    use std::str::FromStr;

    fn data() -> MallConsumptionEntryData {
        MallConsumptionEntryData {
            mall_order_fact_id: MallOrderFactId::new("fact-1"),
            mall_order_item_id: MallOrderItemId::new("item-1"),
            mall_payment_source_id: MallPaymentSourceId::new("ps-1"),
            direction: ConsumptionDirection::Consumption,
            amount: Amount::from_str("80.00").unwrap(),
            customer_id: None,
            origin_sales_order_id: Some(SalesOrderId::new("so-1")),
            sales_order_line_id: Some(SalesOrderLineId::new("sol-1")),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            attribution_status: AttributionStatus::PendingAttribution,
            reverses_consumption_entry_id: None,
        }
    }

    fn reversal_data() -> MallConsumptionEntryData {
        MallConsumptionEntryData {
            direction: ConsumptionDirection::ConsumptionReversal,
            amount: Amount::from_str("80.00").unwrap(),
            origin_sales_order_id: None,
            sales_order_line_id: None,
            reverses_consumption_entry_id: Some(MallConsumptionEntryId::new("ce-1")),
            ..data()
        }
    }

    /// happy path：正向消费落库，归属字段保留。
    #[test]
    fn new_keeps_consumption_attribution() {
        let entry = MallConsumptionEntry::new(MallConsumptionEntryId::new("ce-1"), data()).unwrap();

        assert_eq!(entry.direction, ConsumptionDirection::Consumption);
        assert_eq!(entry.amount, Amount::from_str("80.00").unwrap());
        assert_eq!(entry.origin_sales_order_id, Some(SalesOrderId::new("so-1")));
        assert_eq!(entry.sales_order_line_id, Some(SalesOrderLineId::new("sol-1")));
        assert!(entry.reverses_consumption_entry_id.is_none());
    }

    /// 失败路径 + 金额：金额非正拒绝；方向与冲减引用不一致拒绝。
    #[test]
    fn new_rejects_non_positive_amount_and_direction_mismatch() {
        let zero_amount = MallConsumptionEntryData {
            amount: Amount::from_str("0.00").unwrap(),
            ..data()
        };
        assert!(MallConsumptionEntry::new(MallConsumptionEntryId::new("ce-2"), zero_amount).is_err());

        let reversal_without_ref = MallConsumptionEntryData {
            reverses_consumption_entry_id: None,
            ..reversal_data()
        };
        assert!(
            MallConsumptionEntry::new(MallConsumptionEntryId::new("ce-3"), reversal_without_ref).is_err()
        );

        let consumption_with_ref = MallConsumptionEntryData {
            reverses_consumption_entry_id: Some(MallConsumptionEntryId::new("ce-1")),
            ..data()
        };
        assert!(
            MallConsumptionEntry::new(MallConsumptionEntryId::new("ce-4"), consumption_with_ref).is_err()
        );

        let reversal_ok =
            MallConsumptionEntry::new(MallConsumptionEntryId::new("ce-5"), reversal_data()).unwrap();
        assert_eq!(
            reversal_ok.reverses_consumption_entry_id,
            Some(MallConsumptionEntryId::new("ce-1"))
        );
    }

    /// 归集推进与状态机：合法/非法迁移。
    #[test]
    fn attribution_advances_and_machine_edges_are_fixed() {
        let mut entry = MallConsumptionEntry::new(MallConsumptionEntryId::new("ce-1"), data()).unwrap();
        entry
            .update_attribution_status(AttributionStatus::Attributed)
            .unwrap();
        assert!(entry
            .update_attribution_status(AttributionStatus::Difference)
            .is_err());

        assert!(ensure_transition(
            AttributionStatus::Difference,
            AttributionStatus::PendingAttribution
        )
        .is_ok());
        assert!(ensure_transition(AttributionStatus::Attributed, AttributionStatus::Difference).is_err());
    }
}
