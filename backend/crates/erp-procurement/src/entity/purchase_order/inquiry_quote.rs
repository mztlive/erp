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
    pub available_quantity: Quantity,
    pub address_limits: Vec<PurchaseInquiryAddress>,
    pub expected_delivery_date: BusinessDate,
    #[serde(default, skip_serializing)]
    pub valid_to: Option<BusinessDate>,
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
        if [&self.inquiry_id, &self.inquiry_line_id, &self.quote_id, &self.quote_revision_id]
            .iter()
            .any(|value| value.trim().is_empty() || value.len() > 128)
            || self.requirement_version == 0
            || self.feedback_round == 0
            || self.quote_revision_no == 0
            || self.quote_version == 0
            || self.unit_cost_gross.to_decimal() <= Decimal::ZERO
            || self.input_tax_rate.to_decimal() < Decimal::ZERO
            || self.input_tax_rate.to_decimal() > Decimal::ONE
            || self.available_quantity.to_decimal() <= Decimal::ZERO
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
    if !addresses.is_empty() && total < quantity.to_decimal() {
        return Err(Error::from("采购专项报价配送范围数量不足"));
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
            available_quantity: "20".parse().unwrap(),
            address_limits: vec![],
            expected_delivery_date: "2026-10-22".parse().unwrap(),
            valid_to: Some("2026-11-01".parse().unwrap()),
        }
    }
    #[test]
    fn fixed_scope_may_exceed_partial_supply_but_must_cover_it() {
        let mut source = source();
        source.available_quantity = "5".parse().unwrap();
        source.address_limits = vec![PurchaseInquiryAddress {
            allocation_id: "a".into(),
            address_fingerprint: "a".repeat(64),
            quantity: "20".parse().unwrap(),
        }];
        source.validate().unwrap();
        source.address_limits[0].quantity = "4".parse().unwrap();
        assert!(source.validate().is_err());
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
