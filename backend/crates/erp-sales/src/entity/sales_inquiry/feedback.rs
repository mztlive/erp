//! 对销售发布的独立反馈白名单，不含供应商及内部报价身份。
use std::collections::{HashMap, HashSet};

use erp_core::common::time::Instant;
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{
    InquiryFulfillmentKind, InquiryRequirementData, InquiryRequirementLine, sum_decimal, validate_text,
};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquirySupplyConclusion {
    Available,
    PartiallyAvailable,
    Unavailable,
    Pending,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryFeeConfirmation {
    Estimated,
    Confirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquiryFeeCategory {
    Logistics,
    Printing,
    Warehousing,
    Delivery,
    PlatformTech,
    OfflineService,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquirySupplyAllocation {
    pub allocation_id: String,
    pub quantity: Quantity,
}

/// 每个价格段独立表达数量、地址、税率、交期、有效期和起订量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquirySupplyPrice {
    pub price_id: String,
    pub quantity: Quantity,
    /// 草稿允许未报价；发布前必须填写，零价仍是明确报价。
    pub unit_price_gross: Option<UnitPrice>,
    /// 草稿允许尚未确认税率；发布前必须填写。
    pub tax_rate: Option<Rate>,
    pub delivery_due_at: Instant,
    pub valid_until: Instant,
    pub min_quantity: Quantity,
    #[serde(default)]
    pub max_quantity: Option<Quantity>,
    #[serde(default)]
    pub allow_cumulative: bool,
    #[serde(default)]
    pub allocations: Vec<InquirySupplyAllocation>,
    #[serde(default)]
    pub included_cost_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryFeedbackLine {
    pub line_id: String,
    pub conclusion: InquirySupplyConclusion,
    pub available_quantity: Quantity,
    #[serde(default)]
    pub prices: Vec<InquirySupplyPrice>,
    #[serde(default)]
    pub public_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquiryFee {
    pub fee_id: String,
    pub category: InquiryFeeCategory,
    pub name: String,
    pub amount_gross: Amount,
    pub tax_rate: Rate,
    #[serde(default)]
    pub line_ids: Vec<String>,
    #[serde(default)]
    pub allocation_ids: Vec<String>,
    pub included_in_goods_price: bool,
    pub confirmation: InquiryFeeConfirmation,
    pub requires_erp_payment: bool,
    pub valid_until: Instant,
    #[serde(default)]
    pub valid_conditions: String,
    #[serde(default)]
    pub public_note: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct InquiryFeedbackData {
    pub lines: Vec<InquiryFeedbackLine>,
    #[serde(default)]
    pub fees: Vec<InquiryFee>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryFeedbackSnapshot {
    pub round: u32,
    pub requirement_version: u32,
    pub published_by: String,
    pub published_at: Instant,
    pub data: InquiryFeedbackData,
}

/// 未发布工作片段只向当前责任采购开放。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquiryFeedbackWork {
    pub requirement_version: u32,
    pub actor_id: String,
    pub line_ids: Vec<String>,
    pub data: InquiryFeedbackData,
    pub saved_at: Instant,
    pub frozen: bool,
}

impl InquiryFeedbackData {
    /// 校验完整反馈，逐需求行和地址保留明确最终供给结论。
    /// # 参数
    /// `requirement` 为当前冻结需求；`now` 为发布时刻。
    /// # 返回
    /// 所有行与费用合法且反馈完整时通过。
    /// # 错误
    /// 缺行、待确认、供给数量、地址、价格或有效期非法时拒绝。
    pub fn validate(&self, requirement: &InquiryRequirementData, now: Instant) -> Result<()> {
        if self.lines.len() != requirement.lines.len() || self.fees.len() > 100 {
            return Err(Error::ValidationError("反馈必须覆盖全部需求行且费用不得超过 100 行".into()));
        }
        let mut ids = HashSet::new();
        for line in &self.lines {
            if !ids.insert(&line.line_id) {
                return Err(Error::ValidationError("反馈商品行重复".into()));
            }
            let demand = requirement
                .lines
                .iter()
                .find(|row| row.line_id == line.line_id)
                .ok_or_else(|| Error::ValidationError("反馈包含不属于当前需求的商品行".into()))?;
            line.validate(demand, now)?;
        }
        let mut fee_ids = HashSet::new();
        for fee in &self.fees {
            if !fee_ids.insert(&fee.fee_id) {
                return Err(Error::ValidationError("费用来源身份重复".into()));
            }
            fee.validate(requirement, now)?;
        }
        Ok(())
    }

    /// 汇总尚未含入商品价格的额外成本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回包含暂估项的含税额外成本，暂估标记仍由逐费用返回。
    /// # 错误
    /// 金额溢出时拒绝。
    pub fn extra_cost_total(&self) -> Result<Amount> {
        Ok(Amount::try_from(sum_decimal(
            self.fees
                .iter()
                .filter(|fee| !fee.included_in_goods_price)
                .map(|fee| fee.amount_gross.to_decimal()),
        )?)?)
    }
}

impl InquiryFeedbackLine {
    fn validate(&self, demand: &InquiryRequirementLine, now: Instant) -> Result<()> {
        let available = self.available_quantity.to_decimal();
        let wanted = demand.quantity.to_decimal();
        let valid = match self.conclusion {
            InquirySupplyConclusion::Available => available == wanted,
            InquirySupplyConclusion::PartiallyAvailable => {
                available > Decimal::ZERO && available < wanted && !self.public_note.trim().is_empty()
            },
            InquirySupplyConclusion::Unavailable => {
                available == Decimal::ZERO && !self.public_note.trim().is_empty() && self.prices.is_empty()
            },
            InquirySupplyConclusion::Pending => false,
        };
        if !valid {
            return Err(Error::ValidationError(
                "供给结论、可供数量或剩余说明不符合要求；待确认不得发布".into(),
            ));
        }
        if self.public_note.chars().count() > 4000 || self.prices.len() > 100 {
            return Err(Error::ValidationError("供给说明或价格分段超出限制".into()));
        }
        if available == Decimal::ZERO {
            return Ok(());
        }
        if self.prices.is_empty()
            || sum_decimal(self.prices.iter().map(|price| price.quantity.to_decimal()))? != available
        {
            return Err(Error::ValidationError("价格分段数量合计必须等于可供数量".into()));
        }
        let mut price_ids = HashSet::new();
        let mut allocation_totals = HashMap::<String, Decimal>::new();
        for price in &self.prices {
            if !price_ids.insert(&price.price_id) {
                return Err(Error::ValidationError("供给价格分段身份重复".into()));
            }
            price.validate(demand, now)?;
            for allocation in &price.allocations {
                let total = allocation_totals.entry(allocation.allocation_id.clone()).or_default();
                *total = total
                    .checked_add(allocation.quantity.to_decimal())
                    .ok_or_else(|| Error::ValidationError("可供数量溢出".into()))?;
            }
        }
        validate_allocation_totals(demand, allocation_totals)
    }
}

fn validate_allocation_totals(
    demand: &InquiryRequirementLine,
    totals: HashMap<String, Decimal>,
) -> Result<()> {
    for (id, total) in totals {
        let allocation = demand
            .allocations
            .iter()
            .find(|row| row.allocation_id == id)
            .ok_or_else(|| Error::ValidationError("反馈配送范围不属于需求".into()))?;
        if total > allocation.quantity.to_decimal() {
            return Err(Error::ValidationError("地址可供数量超过需求".into()));
        }
    }
    Ok(())
}

impl InquirySupplyPrice {
    fn validate(&self, demand: &InquiryRequirementLine, now: Instant) -> Result<()> {
        validate_text(&self.price_id, "供给价格身份", 128)?;
        let unit_price = self
            .unit_price_gross
            .ok_or_else(|| Error::ValidationError("请填写含税供给单价后再发布".into()))?;
        let tax_rate =
            self.tax_rate.ok_or_else(|| Error::ValidationError("请填写报价税率后再发布".into()))?;
        if self.quantity.to_decimal() <= Decimal::ZERO
            || self.quantity.to_decimal().normalize().scale() > demand.quantity_scale
            || unit_price.to_decimal() < Decimal::ZERO
        {
            return Err(Error::ValidationError("供给价格或数量非法".into()));
        }
        validate_rate(tax_rate)?;
        if self.valid_until <= now || self.delivery_due_at > demand.delivery_due_at {
            return Err(Error::ValidationError("供给结果已过期或不能按要求交付".into()));
        }
        if self.min_quantity.to_decimal() <= Decimal::ZERO
            || self.min_quantity > self.quantity
            || self.max_quantity.is_some_and(|max| max < self.min_quantity || max < self.quantity)
        {
            return Err(Error::ValidationError("供给数量不满足报价数量档位".into()));
        }
        if self.allocations.len() > 100
            || (demand.fulfillment_kind == InquiryFulfillmentKind::Physical && self.allocations.is_empty())
        {
            return Err(Error::ValidationError("实物供给必须明确至多 100 个配送地址及数量".into()));
        }
        let mut ids = HashSet::new();
        for allocation in &self.allocations {
            if !ids.insert(&allocation.allocation_id)
                || allocation.quantity.to_decimal() <= Decimal::ZERO
                || allocation.quantity.to_decimal().normalize().scale() > demand.quantity_scale
                || !demand.allocations.iter().any(|row| row.allocation_id == allocation.allocation_id)
            {
                return Err(Error::ValidationError("供给配送地址重复、无效或数量非法".into()));
            }
        }
        if !self.allocations.is_empty()
            && sum_decimal(self.allocations.iter().map(|row| row.quantity.to_decimal()))?
                != self.quantity.to_decimal()
        {
            return Err(Error::ValidationError("供给价格分段的地址数量不守恒".into()));
        }
        Ok(())
    }
}

impl InquiryFee {
    fn validate(&self, requirement: &InquiryRequirementData, now: Instant) -> Result<()> {
        validate_text(&self.fee_id, "费用身份", 128)?;
        validate_text(&self.name, "费用名称", 256)?;
        validate_rate(self.tax_rate)?;
        if self.amount_gross.to_decimal() < Decimal::ZERO || self.valid_until <= now {
            return Err(Error::ValidationError("费用金额非法或已失效".into()));
        }
        let mut ids = HashSet::new();
        for id in &self.line_ids {
            if !ids.insert(id) || !requirement.lines.iter().any(|line| &line.line_id == id) {
                return Err(Error::ValidationError("费用商品范围无效或重复".into()));
            }
        }
        ids.clear();
        for id in &self.allocation_ids {
            if !ids.insert(id)
                || !requirement
                    .lines
                    .iter()
                    .filter(|line| self.line_ids.is_empty() || self.line_ids.contains(&line.line_id))
                    .any(|line| line.allocations.iter().any(|allocation| &allocation.allocation_id == id))
            {
                return Err(Error::ValidationError("费用地址范围无效或不属于费用商品范围".into()));
            }
        }
        if self.public_note.chars().count() > 4000 || self.valid_conditions.chars().count() > 2000 {
            return Err(Error::ValidationError("费用说明过长".into()));
        }
        Ok(())
    }
}

fn validate_rate(rate: Rate) -> Result<()> {
    if rate.to_decimal() < Decimal::ZERO || rate.to_decimal() > Decimal::ONE {
        return Err(Error::ValidationError("税率必须在 0 至 1 之间".into()));
    }
    Ok(())
}
