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
    /// 为专项报价的地址匹配生成 64 位十六进制 SHA-256 摘要，按收件人、联系电话、地区代码、详细地址的固定次序编码。
    /// 采购数量与地区名称不参与地址身份。
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
    /// 直送采用专项报价时，按本次交付地址的收件指纹匹配服务端的稳定询价地址，保留请求行顺序及数量。
    /// 任一地址不在专项范围内或身份不唯一时拒绝，不接受客户端自行指定分配身份。
    pub fn from_delivery(rows: &[PurchaseDeliveryAllocation], limits: &[Self]) -> Result<Vec<Self>> {
        rows.iter()
            .map(|row| {
                let fingerprint =
                    Self::fingerprint(&row.recipient, &row.phone, row.region_code.as_str(), &row.address);
                let mut matches = limits.iter().filter(|limit| limit.address_fingerprint == fingerprint);
                let (Some(limit), None) = (matches.next(), matches.next()) else {
                    return Err(Error::from("采购收件地址不属于专项报价范围或身份不唯一"));
                };
                Ok(Self {
                    allocation_id: limit.allocation_id.clone(),
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
    /// 冻结或读取专项报价来源时，核验询价、需求版本、反馈轮次、报价和报价修订身份，以及原含税成本、进项税率和可供量。
    /// 有指定地址范围时，各地址须可追溯，且合计足以覆盖报价可供量；不以缺失版本的历史值当作有效专项来源。
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

    /// 入仓采购采用专项报价时，按销售冻结需求在合法地址范围内分配本次承担量，但不把客户地址写入采购交付资料。
    /// 计算可用量时扣除其他采购占用、保留本单原占用的可用份额；余额损坏或不能覆盖目标数量时拒绝。
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

    use erp_core::region::RegionCode;

    use super::*;

    fn delivery(recipient: &str, quantity: &str) -> PurchaseDeliveryAllocation {
        PurchaseDeliveryAllocation {
            recipient: recipient.into(),
            phone: "13800000000".into(),
            region_code: RegionCode::parse("310115").unwrap(),
            region_name: "上海市浦东新区".into(),
            address: "客户地址".into(),
            quantity: quantity.parse().unwrap(),
        }
    }

    fn address_limit(id: &str, row: &PurchaseDeliveryAllocation) -> PurchaseInquiryAddress {
        PurchaseInquiryAddress {
            allocation_id: id.into(),
            address_fingerprint: PurchaseInquiryAddress::fingerprint(
                &row.recipient,
                &row.phone,
                row.region_code.as_str(),
                &row.address,
            ),
            quantity: "20".parse().unwrap(),
        }
    }

    #[test]
    fn delivery_mapping_keeps_request_order_and_quantity() {
        let rows = [delivery("张三", "2"), delivery("李四", "3")];
        let limits = [address_limit("second", &rows[1]), address_limit("first", &rows[0])];

        let mapped = PurchaseInquiryAddress::from_delivery(&rows, &limits).unwrap();
        assert_eq!(mapped[0].allocation_id, "first");
        assert_eq!(mapped[1].allocation_id, "second");
        assert_eq!(mapped[0].quantity, rows[0].quantity);
        assert_eq!(mapped[1].quantity, rows[1].quantity);
        assert!(PurchaseInquiryAddress::from_delivery(&[], &limits).unwrap().is_empty());
    }

    #[test]
    fn delivery_mapping_rejects_missing_and_ambiguous_addresses() {
        let row = delivery("张三", "2");
        let limit = address_limit("address-1", &row);
        let mut duplicate = limit.clone();
        duplicate.allocation_id = "address-2".into();
        for limits in [vec![], vec![address_limit("other", &delivery("李四", "2"))], vec![limit, duplicate]]
        {
            assert!(matches!(
                PurchaseInquiryAddress::from_delivery(from_ref(&row), &limits),
                Err(Error::LogicError(message)) if message == "采购收件地址不属于专项报价范围或身份不唯一"
            ));
        }
    }

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
