//! 对销售发布的独立反馈白名单，不含供应商及内部报价身份。
use std::collections::HashSet;

use erp_core::common::time::Instant;
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{InquiryRequirementData, InquiryRequirementLine, sum_decimal, validate_text};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InquirySupplyConclusion {
    Available,
    PartiallyAvailable,
    Unavailable,
    Pending,
}

/// 商品供给单价及税率；数量与配送要求由所属反馈行和需求确定。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquirySupplyPrice {
    pub price_id: String,
    /// 草稿允许未报价；发布前必须填写，零价仍是明确报价。
    pub unit_price_gross: Option<UnitPrice>,
    /// 草稿允许尚未确认税率；发布前必须填写。
    pub tax_rate: Option<Rate>,
    /// 仅兼容历史存储；不输出、不参与报价资格判断。
    #[serde(default = "legacy_quote_date", skip_serializing)]
    pub valid_until: Instant,
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
    pub name: String,
    pub amount_gross: Amount,
    pub tax_rate: Rate,
    #[serde(default)]
    pub line_ids: Vec<String>,
    /// 仅兼容历史存储；不输出、不参与报价资格判断。
    #[serde(default = "legacy_quote_date", skip_serializing)]
    pub valid_until: Instant,
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
    /// 缺行、待确认、供给数量、地址、价格非法时拒绝。
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

    /// 汇总独立于商品价格的额外成本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回本轮反馈的含税额外成本。
    /// # 错误
    /// 金额溢出时拒绝。
    pub fn extra_cost_total(&self) -> Result<Amount> {
        Ok(Amount::try_from(sum_decimal(self.fees.iter().map(|fee| fee.amount_gross.to_decimal()))?)?)
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
        if self.public_note.chars().count() > 4000 {
            return Err(Error::ValidationError("供给说明超出限制".into()));
        }
        if available == Decimal::ZERO {
            return Ok(());
        }
        if self.prices.len() != 1 {
            return Err(Error::ValidationError("每件可供商品必须填写一条报价".into()));
        }
        demand.validate_supply_quantity(self.available_quantity)?;
        self.prices[0].validate(now)
    }
}

impl InquirySupplyPrice {
    fn validate(&self, _now: Instant) -> Result<()> {
        validate_text(&self.price_id, "供给价格身份", 128)?;
        let unit_price = self
            .unit_price_gross
            .ok_or_else(|| Error::ValidationError("请填写含税供给单价后再发布".into()))?;
        let tax_rate =
            self.tax_rate.ok_or_else(|| Error::ValidationError("请填写报价税率后再发布".into()))?;
        if unit_price.to_decimal() < Decimal::ZERO {
            return Err(Error::ValidationError("含税供给单价不得为负数".into()));
        }
        validate_rate(tax_rate)?;
        Ok(())
    }
}

impl InquiryFee {
    fn validate(&self, requirement: &InquiryRequirementData, _now: Instant) -> Result<()> {
        validate_text(&self.fee_id, "费用身份", 128)?;
        validate_text(&self.name, "费用名称", 256)?;
        validate_rate(self.tax_rate)?;
        if self.amount_gross.to_decimal() < Decimal::ZERO {
            return Err(Error::ValidationError("费用金额不得为负数".into()));
        }
        let mut ids = HashSet::new();
        for id in &self.line_ids {
            if !ids.insert(id) || !requirement.lines.iter().any(|line| &line.line_id == id) {
                return Err(Error::ValidationError("费用商品范围无效或重复".into()));
            }
        }
        if self.public_note.chars().count() > 4000 {
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

fn legacy_quote_date() -> Instant {
    Instant::from_unix_secs(0)
}
