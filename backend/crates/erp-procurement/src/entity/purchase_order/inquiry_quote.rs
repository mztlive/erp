//! 正式采购冻结的询价专项条款及独立需求来源链。
use std::collections::BTreeSet;

use application_core::CommandFingerprint;
use erp_core::common::time::BusinessDate;
use erp_core::money::{Quantity, Rate, UnitPrice};
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::PurchaseDeliveryAllocation;

/// 采购采用的准确询价配送范围；收件资料由服务端摘要固定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseInquiryAddress {
    pub allocation_id: String,
    pub address_fingerprint: String,
    pub quantity: Quantity,
}

impl PurchaseInquiryAddress {
    /// 对收件身份生成固定字段顺序的摘要，数量与地区名称不参与身份。
    /// # 参数
    /// `recipient`、`phone`、`region_code`、`address` 为冻结需求的真实收件字段。
    /// # 返回
    /// 返回64位SHA256摘要。
    /// # 错误
    /// 无。
    pub fn fingerprint(recipient: &str, phone: &str, region_code: &str, address: &str) -> String {
        CommandFingerprint::from_parts([
            recipient.to_string(),
            phone.to_string(),
            region_code.to_string(),
            address.to_string(),
        ])
        .digest_hex()
        .to_string()
    }
    /// 按当前采购承担地址匹配准确的询价稳定地址，禁止伪造分配身份。
    /// # 参数
    /// `rows` 为本次采购地址，`limits` 为服务器读取的专项范围。
    /// # 返回
    /// 返回对应的稳定地址与本次数量。
    /// # 错误
    /// 任一地址不属于报价范围或身份不唯一时拒绝。
    pub fn from_delivery(rows: &[PurchaseDeliveryAllocation], limits: &[Self]) -> Result<Vec<Self>> {
        rows.iter()
            .map(|row| {
                let fingerprint =
                    Self::fingerprint(&row.recipient, &row.phone, row.region_code.as_str(), &row.address);
                let matches = limits
                    .iter()
                    .filter(|limit| limit.address_fingerprint == fingerprint)
                    .collect::<Vec<_>>();
                if matches.len() != 1 {
                    return Err(Error::from("采购收件地址不属于专项报价范围或身份不唯一"));
                }
                Ok(Self {
                    allocation_id: matches[0].allocation_id.clone(),
                    address_fingerprint: fingerprint,
                    quantity: row.quantity,
                })
            })
            .collect()
    }
}

/// 与通用供给条款分离的本单专项采购依据；仅由服务端实际报价构造。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseInquiryQuoteSource {
    pub inquiry_id: String,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub inquiry_line_id: String,
    pub quote_id: String,
    pub quote_revision_id: String,
    pub quote_revision_no: u32,
    pub quote_version: u64,
    pub unit_cost_gross: UnitPrice,
    pub input_tax_rate: Rate,
    pub minimum_order_quantity: Quantity,
    pub available_quantity: Quantity,
    pub address_limits: Vec<PurchaseInquiryAddress>,
    pub expected_delivery_date: BusinessDate,
    pub valid_from: BusinessDate,
    pub valid_to: BusinessDate,
    pub payment_term_code: String,
    pub allow_cumulative_batches: bool,
}
impl PurchaseInquiryQuoteSource {
    /// 校验冻结专项依据的身份及商业条件，不接受缺失版本的历史伪来源。
    /// # 参数
    /// 无。
    /// # 返回
    /// 作用域及条件完整时成功。
    /// # 错误
    /// 缺失身份、非法金额税率、数量或期限时拒绝。
    pub fn validate(&self) -> Result<()> {
        if [
            &self.inquiry_id,
            &self.inquiry_line_id,
            &self.quote_id,
            &self.quote_revision_id,
            &self.payment_term_code,
        ]
        .iter()
        .any(|value| value.trim().is_empty() || value.len() > 128)
            || self.requirement_version == 0
            || self.feedback_round == 0
            || self.quote_revision_no == 0
            || self.quote_version == 0
            || self.unit_cost_gross.to_decimal() <= Decimal::ZERO
            || self.input_tax_rate.to_decimal() < Decimal::ZERO
            || self.input_tax_rate.to_decimal() > Decimal::ONE
            || self.minimum_order_quantity.to_decimal() <= Decimal::ZERO
            || self.available_quantity < self.minimum_order_quantity
            || self.valid_from >= self.valid_to
        {
            return Err(Error::from("采购专项报价身份或商业条件非法"));
        }
        validate_addresses(&self.address_limits, self.available_quantity)
    }

    /// 按入仓采购承担量选择内部需求范围，客户地址不写入采购交付资料。
    /// # 参数
    /// `bindings`为销售冻结需求，`held`为报价已占地址，`own`为本采购原占用，`quantity`为新目标。
    /// # 返回
    /// 返回数量守恒的稳定地址采用范围。
    /// # 错误
    /// 账损坏或合法剩余地址不足时拒绝。
    pub fn warehouse_allocations(
        &self,
        bindings: &[PurchaseInquiryAddress],
        held: &[PurchaseInquiryAddress],
        own: &[PurchaseInquiryAddress],
        quantity: Quantity,
    ) -> Result<Vec<PurchaseInquiryAddress>> {
        let mut remaining = quantity.to_decimal();
        let mut rows = Vec::new();
        for binding in bindings {
            let Some(limit) = self.address_limits.iter().find(|limit| {
                limit.allocation_id == binding.allocation_id
                    && limit.address_fingerprint == binding.address_fingerprint
            }) else {
                continue;
            };
            let used = address_quantity(held, &binding.allocation_id)
                .checked_sub(address_quantity(own, &binding.allocation_id))
                .filter(|value| *value >= Decimal::ZERO)
                .ok_or("专项报价地址采用余额损坏")?;
            let available = limit
                .quantity
                .to_decimal()
                .checked_sub(used)
                .ok_or("专项报价地址采用超出范围")?
                .max(Decimal::ZERO)
                .min(binding.quantity.to_decimal())
                .min(remaining);
            if available > Decimal::ZERO {
                rows.push(PurchaseInquiryAddress {
                    allocation_id: binding.allocation_id.clone(),
                    address_fingerprint: binding.address_fingerprint.clone(),
                    quantity: Quantity::try_from(available)?,
                });
                remaining = remaining.checked_sub(available).ok_or("专项报价地址数量超出范围")?;
            }
            if remaining.is_zero() {
                break;
            }
        }
        if !remaining.is_zero() {
            return Err(Error::from("专项报价剩余配送范围不足本次入仓采购需求"));
        }
        Ok(rows)
    }

    /// 按本次采购事实检验专项条件；分批不自动满足整单起订量。
    /// # 参数
    /// 本次数量、承担地址、交付日、业务日和服务器核验的累计承诺/消耗。
    /// # 返回
    /// 原报价当前适用时成功。
    /// # 错误
    /// 过期、低于MOQ、超出供量、地址或交期不符时拒绝。
    pub fn ensure_usage(
        &self,
        quantity: Quantity,
        addresses: &[PurchaseInquiryAddress],
        delivery_date: BusinessDate,
        on_date: BusinessDate,
        committed: Quantity,
        consumed: Quantity,
    ) -> Result<()> {
        self.validate()?;
        if !on_date.is_within(self.valid_from, Some(self.valid_to))
            || delivery_date < self.expected_delivery_date
            || quantity.to_decimal() <= Decimal::ZERO
            || consumed.to_decimal() < Decimal::ZERO
        {
            return Err(Error::from("采购专项报价已失效或交期、数量不适用，请重新确认"));
        }
        let total =
            quantity.to_decimal().checked_add(consumed.to_decimal()).ok_or("专项采购累计数量超出范围")?;
        let batch = if self.allow_cumulative_batches { committed } else { quantity };
        if total > self.available_quantity.to_decimal()
            || batch < self.minimum_order_quantity
            || (self.allow_cumulative_batches
                && (committed < quantity || committed > self.available_quantity))
        {
            return Err(Error::from("采购专项报价可供量或起订量不满足，请重新确认"));
        }
        validate_addresses(addresses, quantity)?;
        if self.address_limits.is_empty() != addresses.is_empty()
            || addresses.iter().any(|address| {
                !self.address_limits.iter().any(|limit| {
                    limit.allocation_id == address.allocation_id
                        && limit.address_fingerprint == address.address_fingerprint
                        && limit.quantity >= address.quantity
                })
            })
        {
            return Err(Error::from("采购专项报价地址范围或数量已变化，请重新确认"));
        }
        Ok(())
    }
}
fn address_quantity(rows: &[PurchaseInquiryAddress], id: &str) -> Decimal {
    rows.iter()
        .find(|row| row.allocation_id == id)
        .map(|row| row.quantity.to_decimal())
        .unwrap_or(Decimal::ZERO)
}

fn validate_addresses(addresses: &[PurchaseInquiryAddress], quantity: Quantity) -> Result<()> {
    let mut seen = BTreeSet::new();
    let mut total = Decimal::ZERO;
    if addresses.len() > 100 {
        return Err(Error::from("采购专项报价每行最多100地址"));
    }
    for row in addresses {
        if row.allocation_id.trim().is_empty()
            || row.address_fingerprint.len() != 64
            || !row.address_fingerprint.bytes().all(|value| value.is_ascii_hexdigit())
            || row.quantity.to_decimal() <= Decimal::ZERO
            || !seen.insert(&row.allocation_id)
        {
            return Err(Error::from("采购专项报价配送身份或数量非法"));
        }
        total = total.checked_add(row.quantity.to_decimal()).ok_or("专项采购地址数量超出范围")?;
    }
    if !addresses.is_empty() && total != quantity.to_decimal() {
        return Err(Error::from("采购专项报价地址数量不守恒"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::slice::from_ref;

    use super::*;
    fn source() -> PurchaseInquiryQuoteSource {
        PurchaseInquiryQuoteSource {
            inquiry_id: "iq".into(),
            requirement_version: 1,
            feedback_round: 2,
            inquiry_line_id: "line".into(),
            quote_id: "quote".into(),
            quote_revision_id: "quote-v1".into(),
            quote_revision_no: 1,
            quote_version: 1,
            unit_cost_gross: "8.00".parse().unwrap(),
            input_tax_rate: "0.13".parse().unwrap(),
            minimum_order_quantity: "10".parse().unwrap(),
            available_quantity: "20".parse().unwrap(),
            address_limits: vec![],
            expected_delivery_date: "2026-10-22".parse().unwrap(),
            valid_from: "2026-10-01".parse().unwrap(),
            valid_to: "2026-11-01".parse().unwrap(),
            payment_term_code: "NET-30".into(),
            allow_cumulative_batches: false,
        }
    }
    #[test]
    fn exact_source_rejects_expiry_minimum_and_total_supply() {
        let source = source();
        let day = "2026-10-23".parse().unwrap();
        let zero = "0".parse().unwrap();
        source.ensure_usage("10".parse().unwrap(), &[], day, day, zero, zero).unwrap();
        assert!(
            source.ensure_usage("5".parse().unwrap(), &[], day, day, "20".parse().unwrap(), zero).is_err()
        );
        assert!(
            source.ensure_usage("10".parse().unwrap(), &[], day, day, zero, "11".parse().unwrap()).is_err()
        );
        assert!(
            source
                .ensure_usage("10".parse().unwrap(), &[], day, "2026-11-01".parse().unwrap(), zero, zero)
                .is_err()
        );
    }
    #[test]
    fn cumulative_moq_needs_explicit_terms_and_actual_commitment() {
        let mut source = source();
        source.allow_cumulative_batches = true;
        let day = "2026-10-23".parse().unwrap();
        let zero = "0".parse().unwrap();
        assert!(source.ensure_usage("5".parse().unwrap(), &[], day, day, zero, zero).is_err());
        source.ensure_usage("5".parse().unwrap(), &[], day, day, "10".parse().unwrap(), zero).unwrap();
    }
    #[test]
    fn warehouse_quote_allocation_excludes_own_hold_but_respects_other_purchases() {
        let mut source = source();
        let limit = PurchaseInquiryAddress {
            allocation_id: "a".into(),
            address_fingerprint: "a".repeat(64),
            quantity: "20".parse().unwrap(),
        };
        source.address_limits = vec![limit.clone()];
        let mut own = limit.clone();
        own.quantity = "10".parse().unwrap();
        let rows = source
            .warehouse_allocations(from_ref(&limit), from_ref(&own), from_ref(&own), "10".parse().unwrap())
            .unwrap();
        assert_eq!(rows[0].quantity.to_string(), "10");
        assert!(
            source
                .warehouse_allocations(from_ref(&limit), from_ref(&own), &[], "15".parse().unwrap())
                .is_err()
        );
    }
}
