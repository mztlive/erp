//! 专项报价适用数量、地址、交期条件。
use std::collections::BTreeSet;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{SkuId, SupplierAccountId};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 报价精确适用的稳定配送分配及收件资料指纹；不保存可公开的供应商信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuoteAddressLimit {
    pub allocation_id: String,
    pub address_fingerprint: String,
    pub quantity: Quantity,
}

/// 不可变的供应商实际采购条件，与销售可见供给价分别存储。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct QuoteTerms {
    /// 无法供货的回复只留存，不得采用。
    #[serde(default)]
    pub unavailable: bool,
    /// 本行总运费；None 表示未确认，零表示已含运费。
    #[serde(default)]
    pub freight_gross: Option<Amount>,
    pub unit_cost_gross: UnitPrice,
    pub input_tax_rate: Rate,
    pub available_quantity: Quantity,
    /// 需求地址的适用额度；总和可超过可供量，实际采用另受总余额限制。
    pub address_limits: Vec<QuoteAddressLimit>,
    pub expected_delivery_date: BusinessDate,
    /// 仅兼容历史存储，不输出且不参与采用校验。
    #[serde(default, skip_serializing)]
    pub valid_to: Option<BusinessDate>,
}

/// 由正式采购事务读取的实际采用条件；消耗量不得来自客户端。
#[derive(Debug, Clone)]
pub struct InquiryQuoteUse {
    pub inquiry_id: String,
    pub requirement_version: u32,
    pub line_id: String,
    pub sku_id: SkuId,
    pub supplier_id: SupplierAccountId,
    pub quantity: Quantity,
    pub addresses: Vec<QuoteAddressLimit>,
    pub expected_delivery_date: BusinessDate,
    pub on_date: BusinessDate,
    pub consumed_quantity: Quantity,
}

impl QuoteTerms {
    /// 规范化并校验报价商业条件。
    /// # 参数
    /// `self` 为采购输入的原始条件。
    /// # 返回
    /// 返回规范化且数量、税率、期限合法的条款。
    /// # 错误
    /// 非正价格/数量、非法税率、重复地址或地址额度不足时拒绝。
    pub fn validated(self) -> Result<Self> {
        if !Self::unit_cost_is_valid(self.unit_cost_gross.to_decimal(), self.unavailable)
            || (!self.unavailable && self.available_quantity.to_decimal() <= Decimal::ZERO)
            || (self.unavailable && self.available_quantity.to_decimal() != Decimal::ZERO)
            || self.freight_gross.is_some_and(|value| value.to_decimal() < Decimal::ZERO)
            || !Self::tax_rate_is_valid(self.input_tax_rate.to_decimal())
        {
            return Err(Error::ValidationError("专项报价价格、税率或数量非法".into()));
        }
        validate_addresses(&self.address_limits, self.available_quantity, false)?;
        Ok(self)
    }

    /// 核对回复价格是否符合正常报价或无法供货的价格规则。
    /// # 参数
    /// `unit_cost` 为含税单价；`unavailable` 表示无法供货。
    /// # 返回
    /// 正常报价单价为正、无法供货单价为零时返回真。
    /// # 错误
    /// 无。
    pub fn unit_cost_is_valid(unit_cost: Decimal, unavailable: bool) -> bool {
        if unavailable { unit_cost == Decimal::ZERO } else { unit_cost > Decimal::ZERO }
    }

    /// 核对内部报价税率的合法区间。
    /// # 参数
    /// `tax_rate` 为小数税率，百分数须先由输入适配层换算。
    /// # 返回
    /// 税率位于闭区间零至一时返回真。
    /// # 错误
    /// 无。
    pub fn tax_rate_is_valid(tax_rate: Decimal) -> bool {
        (Decimal::ZERO..=Decimal::ONE).contains(&tax_rate)
    }

    /// 核对本次采用的数量、地址、交期；禁止静默切换价格。
    /// # 参数
    /// `usage` 为服务端读取且已核验的本次采购事实。
    /// # 返回
    /// 精确条件适用时成功。
    /// # 错误
    /// 地址变化、可供量或交期不满足时拒绝。
    pub fn ensure_applicable(&self, usage: &InquiryQuoteUse) -> Result<()> {
        self.clone().validated()?;
        if self.unavailable {
            return Err(Error::BusinessLogicError("供应商已回复无法供货，请重新确认".into()));
        }
        if usage.quantity.to_decimal() <= Decimal::ZERO
            || usage.consumed_quantity.to_decimal() < Decimal::ZERO
            || usage.expected_delivery_date < self.expected_delivery_date
        {
            return Err(Error::BusinessLogicError("专项报价采购数量或交期不满足，请重新确认".into()));
        }
        self.ensure_quantity(usage)?;
        self.ensure_addresses(usage)
    }

    fn ensure_quantity(&self, usage: &InquiryQuoteUse) -> Result<()> {
        let total = usage
            .quantity
            .to_decimal()
            .checked_add(usage.consumed_quantity.to_decimal())
            .ok_or_else(|| Error::ValidationError("专项报价累计采用数量超出范围".into()))?;
        if total > self.available_quantity.to_decimal() {
            return Err(Error::BusinessLogicError("专项报价累计采购超出可供量，请重新确认".into()));
        }
        Ok(())
    }

    fn ensure_addresses(&self, usage: &InquiryQuoteUse) -> Result<()> {
        validate_addresses(&usage.addresses, usage.quantity, true)?;
        if self.address_limits.is_empty() && usage.addresses.is_empty() {
            return Ok(());
        }
        if self.address_limits.is_empty() || usage.addresses.is_empty() {
            return Err(Error::BusinessLogicError("专项报价适用地址缺失或变化，请重新确认".into()));
        }
        for address in &usage.addresses {
            if !self.address_limits.iter().any(|limit| {
                limit.allocation_id == address.allocation_id
                    && limit.address_fingerprint == address.address_fingerprint
                    && limit.quantity >= address.quantity
            }) {
                return Err(Error::BusinessLogicError("专项报价地址或分配数量不适用，请重新确认".into()));
            }
        }
        Ok(())
    }
}

fn validate_addresses(rows: &[QuoteAddressLimit], quantity: Quantity, exact: bool) -> Result<()> {
    if rows.len() > 100 {
        return Err(Error::ValidationError("专项报价每行最多100个地址".into()));
    }
    let mut total = Decimal::ZERO;
    let mut seen = BTreeSet::new();
    for row in rows {
        if row.allocation_id.trim().is_empty()
            || row.allocation_id.len() > 128
            || row.address_fingerprint.len() != 64
            || !row.address_fingerprint.bytes().all(|value| value.is_ascii_hexdigit())
            || row.quantity.to_decimal() <= Decimal::ZERO
            || !seen.insert(&row.allocation_id)
        {
            return Err(Error::ValidationError("专项报价地址身份、指纹或数量非法".into()));
        }
        total = total
            .checked_add(row.quantity.to_decimal())
            .ok_or_else(|| Error::ValidationError("专项报价地址数量超出范围".into()))?;
    }
    if !rows.is_empty() && (total < quantity.to_decimal() || (exact && total != quantity.to_decimal())) {
        return Err(Error::ValidationError("专项报价配送数量不足或与本次采用数量不匹配".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reply_price_rule_distinguishes_normal_and_unavailable() {
        for (price, normal, unavailable) in
            [(Decimal::NEGATIVE_ONE, false, false), (Decimal::ZERO, false, true), (Decimal::ONE, true, false)]
        {
            assert_eq!(QuoteTerms::unit_cost_is_valid(price, false), normal);
            assert_eq!(QuoteTerms::unit_cost_is_valid(price, true), unavailable);
        }
    }

    #[test]
    fn reply_tax_rate_accepts_both_bounds_and_rejects_outside() {
        for (rate, expected) in
            [("-0.0001", false), ("0", true), ("0.13", true), ("1", true), ("1.0001", false)]
        {
            assert_eq!(QuoteTerms::tax_rate_is_valid(rate.parse().unwrap()), expected);
        }
    }
}
