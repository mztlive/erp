//! 专项报价适用数量、地址、交期和批量条件。
use std::collections::BTreeSet;

use erp_core::common::time::BusinessDate;
use erp_core::ids::{SkuId, SupplierAccountId};
use erp_core::money::{Quantity, Rate, UnitPrice};
use erp_core::validation::normalize_required_text;
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
    pub unit_cost_gross: UnitPrice,
    pub input_tax_rate: Rate,
    pub minimum_order_quantity: Quantity,
    pub available_quantity: Quantity,
    pub address_limits: Vec<QuoteAddressLimit>,
    pub expected_delivery_date: BusinessDate,
    pub valid_from: BusinessDate,
    /// 失效日当天起不可采用。
    pub valid_to: BusinessDate,
    pub payment_term_code: String,
    /// 只有条款明确允许累计分批时才可使用系统核验的累计承诺数量。
    pub allow_cumulative_batches: bool,
}

/// 由正式采购事务读取的实际采用条件；累计承诺和消耗量不得来自客户端。
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
    pub committed_quantity: Quantity,
    pub consumed_quantity: Quantity,
}

impl QuoteTerms {
    /// 规范化并校验报价商业条件。
    /// # 参数
    /// `self` 为采购输入的原始条件。
    /// # 返回
    /// 返回规范化且数量、税率、期限合法的条款。
    /// # 错误
    /// 非正价格/数量、非法税率、重复或超量地址、非法期限时拒绝。
    pub fn validated(mut self) -> Result<Self> {
        self.payment_term_code = normalize_required_text(
            self.payment_term_code,
            "专项报价付款条件不能为空",
            128,
            "专项报价付款条件过长",
        )?;
        if self.unit_cost_gross.to_decimal() <= Decimal::ZERO
            || self.minimum_order_quantity.to_decimal() <= Decimal::ZERO
            || self.available_quantity < self.minimum_order_quantity
            || self.input_tax_rate.to_decimal() < Decimal::ZERO
            || self.input_tax_rate.to_decimal() > Decimal::ONE
            || self.valid_from >= self.valid_to
        {
            return Err(Error::ValidationError("专项报价价格、税率、数量或有效期非法".into()));
        }
        validate_addresses(&self.address_limits, Some(self.available_quantity))?;
        Ok(self)
    }

    /// 核对本次采用的数量、地址、交期及有效期；禁止静默切换价格。
    /// # 参数
    /// `usage` 为服务端读取且已核验的本次采购事实。
    /// # 返回
    /// 精确条件适用时成功。
    /// # 错误
    /// 过期、地址变化、MOQ、可供量或交期不满足时拒绝。
    pub fn ensure_applicable(&self, usage: &InquiryQuoteUse) -> Result<()> {
        self.clone().validated()?;
        if !usage.on_date.is_within(self.valid_from, Some(self.valid_to)) {
            return Err(Error::BusinessLogicError("专项报价已过期或尚未生效，请重新确认".into()));
        }
        if usage.quantity.to_decimal() <= Decimal::ZERO
            || usage.consumed_quantity.to_decimal() < Decimal::ZERO
            || usage.committed_quantity.to_decimal() < Decimal::ZERO
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
        let batch = if self.allow_cumulative_batches {
            if usage.committed_quantity < usage.quantity || usage.committed_quantity > self.available_quantity
            {
                return Err(Error::BusinessLogicError("专项报价累计分批承诺数量未经满足，请重新确认".into()));
            }
            usage.committed_quantity
        } else {
            usage.quantity
        };
        if batch < self.minimum_order_quantity {
            return Err(Error::BusinessLogicError("专项报价本次采购不满足起订量，请重新确认".into()));
        }
        Ok(())
    }

    fn ensure_addresses(&self, usage: &InquiryQuoteUse) -> Result<()> {
        validate_addresses(&usage.addresses, Some(usage.quantity))?;
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

fn validate_addresses(rows: &[QuoteAddressLimit], quantity: Option<Quantity>) -> Result<()> {
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
    if !rows.is_empty() && quantity.is_some_and(|quantity| quantity.to_decimal() != total) {
        return Err(Error::ValidationError("专项报价地址数量合计必须等于适用数量".into()));
    }
    Ok(())
}
