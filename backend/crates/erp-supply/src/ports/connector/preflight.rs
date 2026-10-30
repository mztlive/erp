//! 使用原冻结行、实际数量和已确认地址的只读下单预检。

use std::collections::BTreeMap;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::{Amount, Quantity, UnitPrice, round_to_cent};

use super::common::{ConnectorResult, Snapshot, SupplierSku};
use super::offer::DeliveryContext;
use super::order::CreateOrder;
use crate::{Error, Result};

/// 来源对本次请求数量的验证；不得把该数量存为全局库存。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreflightLine {
    pub line_id: String,
    pub sku: SupplierSku,
    pub quantity: Quantity,
    pub unit_price: UnitPrice,
    /// 本次来源详情中的订购单位原文；缺失时不能证明原采购单位仍有效。
    pub source_unit: Option<String>,
    /// 来源明确证明本次数量可满足，不承诺后续库存预占。
    pub quantity_satisfied: bool,
    /// 可售、起购、倍数、地区及附加服务规则已全部核实。
    pub purchase_allowed: bool,
}

/// 本次完整校验结果；地址上下文属于敏感资料，禁止 Debug。
#[derive(Clone, PartialEq, Eq)]
pub struct PreflightProof {
    pub delivery_context: DeliveryContext,
    pub lines: Vec<PreflightLine>,
    pub goods_amount: Amount,
    pub shipping_amount: Amount,
    pub payable_amount: Amount,
    /// 只代表供应商可执行条件，不代替 ERP 付款批准及独占资金保障。
    pub remote_payment_available: bool,
    pub valid_until: Option<Instant>,
}

impl PreflightProof {
    /// 核对预检覆盖原行、数量、地址和批准金额。
    /// # 参数
    /// `request` 为原冻结创建请求；`now` 为检查时刻。
    /// # 返回
    /// 事实完整且未超出批准范围时允许后续资格核验。
    /// # 错误
    /// 缺行、重复、身份或地址变化、过期、不可采或超批时阻断。
    pub fn validate(&self, request: &CreateOrder, now: Instant) -> Result<()> {
        if self.delivery_context != request.delivery_context
            || self.valid_until.is_some_and(|until| until <= now)
            || !self.remote_payment_available
        {
            return Err(Error::BusinessLogicError("地址或支付条件需要重新核验".into()));
        }
        let expected: BTreeMap<_, _> =
            request.lines.iter().map(|line| (line.line_id.as_str(), line)).collect();
        if expected.is_empty() || expected.len() != request.lines.len() || self.lines.len() != expected.len()
        {
            return Err(Error::ValidationError("下单预检明细未完整覆盖冻结采购行".into()));
        }
        let mut observed = BTreeMap::new();
        for line in &self.lines {
            let frozen = expected
                .get(line.line_id.as_str())
                .ok_or_else(|| Error::ValidationError("下单预检存在无归属明细".into()))?;
            if observed.insert(line.line_id.as_str(), ()).is_some()
                || line.sku != frozen.sku
                || line.quantity != frozen.quantity
                || line.unit_price != frozen.approved_unit_price
                || line.quantity.to_decimal() <= Amount::zero().to_decimal()
                || line.unit_price.to_decimal() < Amount::zero().to_decimal()
                || line.source_unit.as_deref().is_none_or(|unit| unit.trim().is_empty())
                || !line.quantity_satisfied
                || !line.purchase_allowed
            {
                return Err(Error::BusinessLogicError("商品数量、采购限制或报价未通过下单核验".into()));
            }
        }
        self.validate_amounts(request)
    }

    fn validate_amounts(&self, request: &CreateOrder) -> Result<()> {
        let goods = self.lines.iter().try_fold(Amount::zero().to_decimal(), |total, line| {
            line.unit_price
                .to_decimal()
                .checked_mul(line.quantity.to_decimal())
                .and_then(|value| total.checked_add(round_to_cent(value)))
                .ok_or_else(|| Error::ValidationError("下单预检金额溢出".into()))
        })?;
        let total = self
            .goods_amount
            .to_decimal()
            .checked_add(self.shipping_amount.to_decimal())
            .ok_or_else(|| Error::ValidationError("下单预检总额溢出".into()))?;
        if goods < Amount::zero().to_decimal()
            || self.shipping_amount < Amount::zero()
            || self.goods_amount.to_decimal() != goods
            || self.payable_amount.to_decimal() != total
            || self.shipping_amount > request.delivery.shipping_fee
            || self.payable_amount > request.approved_total
        {
            return Err(Error::BusinessLogicError("供应商商品金额或运费超出已批准执行范围".into()));
        }
        Ok(())
    }
}

#[async_trait]
pub trait OrderPreflight: Send + Sync {
    /// 以本次完整冻结明细、数量和确认地址只读核验下单条件。
    /// # 参数
    /// `request` 包含批准含税价格、费用、配送及原动作身份；不得修改请求。
    /// # 返回
    /// 完整预检证据，未知的价格、库存、资格或费用必须失败关闭。
    /// # 错误
    /// 资料缺失、采购限制、金额超批或外部读取失败时返回分类错误。
    async fn inspect(&self, request: &CreateOrder) -> ConnectorResult<Snapshot<PreflightProof>>;
}

#[cfg(test)]
mod tests {
    use super::super::common::ActionKey;
    use super::super::order::{DeliveryChoice, DeliveryMethod, OrderLine, Recipient};
    use super::*;

    fn request() -> CreateOrder {
        CreateOrder {
            action: ActionKey { id: "create".into(), payload_hash: "hash".into() },
            merchant_order_no: "original".into(),
            lines: vec![OrderLine {
                source_purchase_terms: None,
                line_id: "one".into(),
                sku: SupplierSku { product_id: None, spec_id: "sku".into() },
                quantity: "3".parse().unwrap(),
                approved_unit_price: "10".parse().unwrap(),
                attributes: vec![],
            }],
            recipient: Recipient {
                name: "name".into(),
                phone: "phone".into(),
                region: "region".into(),
                address: "address".into(),
                geocoded_address: None,
                supplier_address: None,
            },
            delivery_context: DeliveryContext {
                reference: "confirmed-address-version".into(),
                supplier_address: None,
            },
            delivery: DeliveryChoice {
                method: DeliveryMethod::Courier,
                shipping_fee: "2".parse().unwrap(),
                reference: "delivery".into(),
            },
            approved_total: "32".parse().unwrap(),
        }
    }

    fn proof(request: &CreateOrder) -> PreflightProof {
        PreflightProof {
            delivery_context: request.delivery_context.clone(),
            lines: request
                .lines
                .iter()
                .map(|line| PreflightLine {
                    line_id: line.line_id.clone(),
                    sku: line.sku.clone(),
                    quantity: line.quantity,
                    unit_price: line.approved_unit_price,
                    source_unit: Some("件".into()),
                    quantity_satisfied: true,
                    purchase_allowed: true,
                })
                .collect(),
            goods_amount: "30".parse().unwrap(),
            shipping_amount: "2".parse().unwrap(),
            payable_amount: "32".parse().unwrap(),
            remote_payment_available: true,
            valid_until: None,
        }
    }

    #[test]
    fn preflight_is_bound_to_actual_quantity_and_address() {
        let request = request();
        let checked = proof(&request);
        checked.validate(&request, Instant::now()).unwrap();
        let mut changed = request.clone();
        changed.lines[0].quantity = "30".parse().unwrap();
        assert!(checked.validate(&changed, Instant::now()).is_err());
        changed = request;
        changed.delivery_context.reference = "changed-address".into();
        assert!(checked.validate(&changed, Instant::now()).is_err());
    }

    #[test]
    fn missing_duplicate_unknown_or_expired_facts_block_writes() {
        let request = request();
        let now = Instant::now();
        let mut value = proof(&request);
        value.lines.clear();
        assert!(value.validate(&request, now).is_err());
        value = proof(&request);
        value.lines[0].purchase_allowed = false;
        assert!(value.validate(&request, now).is_err());
        value = proof(&request);
        value.valid_until = Some(now);
        assert!(value.validate(&request, now).is_err());
        value = proof(&request);
        value.lines.push(value.lines[0].clone());
        assert!(value.validate(&request, now).is_err());
    }

    #[test]
    fn unapproved_shipping_or_price_increase_cannot_expand_budget() {
        let request = request();
        let mut value = proof(&request);
        value.shipping_amount = "3".parse().unwrap();
        value.payable_amount = "33".parse().unwrap();
        assert!(value.validate(&request, Instant::now()).is_err());
        value = proof(&request);
        value.lines[0].unit_price = "11".parse().unwrap();
        assert!(value.validate(&request, Instant::now()).is_err());
    }

    #[test]
    fn missing_or_empty_source_unit_cannot_pass_new_preflight() {
        let request = request();
        for unit in [None, Some(String::new()), Some("  ".into())] {
            let mut value = proof(&request);
            value.lines[0].source_unit = unit;
            assert!(matches!(value.validate(&request, Instant::now()), Err(Error::BusinessLogicError(_))));
        }
    }
}
