//! 按原冻结行实际数量、四级地址、明确单价与批准运费读取下单资格。
use std::collections::BTreeMap;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::{Amount, UnitPrice, round_to_cent};
use erp_supply::ports::connector::address::SupplierAddressSource;
use erp_supply::ports::connector::common::{ConnectorResult, Snapshot};
use erp_supply::ports::connector::order::{CreateOrder, OrderLine};
use erp_supply::ports::connector::preflight::{OrderPreflight, PreflightLine, PreflightProof};
use rust_decimal::Decimal;

use super::delivery::{area, line_quantities, observed};
use super::order_payload::submit;
use super::transform::{percentage, purchase_limits};
use super::transport::{JdVopReadRequest, ReadClient};
use super::wire::{SellPrice, SkuDetail};
use super::wire_order::{Balance, SaleEligibility, Stock};
use super::{JdVopConnector, mapping, protocol_required};

#[async_trait]
impl OrderPreflight for JdVopConnector {
    async fn inspect(&self, request: &CreateOrder) -> ConnectorResult<Snapshot<PreflightProof>> {
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        submit(request, profile)?;
        self.validate_choice(request)?;
        let address = request
            .delivery_context
            .supplier_address
            .as_ref()
            .ok_or_else(|| mapping("京东四级收货地址未确认"))?;
        if !SupplierAddressSource::validate(self, address).await?.value {
            return Err(mapping("京东收货地址已失效，请重新确认"));
        }
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let lines = line_quantities(&request.lines)?;
        let area = area(&request.delivery_context)?;
        let eligibility: Vec<SaleEligibility> = reader
            .read(JdVopReadRequest::SaleEligibility { lines: lines.clone(), area: area.clone() })
            .await?;
        let stock: Vec<Stock> = reader.read(JdVopReadRequest::Stock { lines, area }).await?;
        verify_eligibility(request, &eligibility, &stock)?;
        let result = self.verify_line_prices(request).await?;
        let shipping = self.freight(&request.delivery_context, &request.lines).await?;
        let goods = goods_amount(&result)?;
        let payable = goods
            .to_decimal()
            .checked_add(shipping.to_decimal())
            .ok_or_else(|| mapping("京东预检总额溢出"))?;
        let payable: Amount = payable.to_string().parse().map_err(|_| mapping("京东预检总额精度无效"))?;
        let balance: Balance = reader.read(JdVopReadRequest::Balance).await?;
        let available: Decimal = balance
            .balance_info
            .ok_or_else(|| mapping("京东未返回预存款余额"))?
            .remain_limit
            .0
            .parse()
            .map_err(|_| mapping("京东预存款余额格式无效"))?;
        let proof = PreflightProof {
            delivery_context: request.delivery_context.clone(),
            lines: result,
            goods_amount: goods,
            shipping_amount: shipping,
            payable_amount: payable,
            remote_payment_available: available >= payable.to_decimal(),
            valid_until: None,
        };
        proof
            .validate(request, Instant::now())
            .map_err(|_| mapping("京东价格、运费、数量或可执行余额不符合批准范围"))?;
        Ok(observed(proof))
    }
}

impl JdVopConnector {
    async fn verify_line_prices(&self, request: &CreateOrder) -> ConnectorResult<Vec<PreflightLine>> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let mut result = Vec::new();
        for line in &request.lines {
            result.push(Self::verify_line_price(reader, line).await?);
        }
        Ok(result)
    }

    async fn verify_line_price(reader: &ReadClient, line: &OrderLine) -> ConnectorResult<PreflightLine> {
        if let Some(terms) = &line.source_purchase_terms {
            terms.ensure_quantity(line.quantity).map_err(|_| mapping("京东数量不符合原冻结采购限制"))?;
            terms.ensure_automatic_order().map_err(|_| mapping("京东商品包含尚未支持的附加履约流程"))?;
        }
        let detail: SkuDetail = reader
            .read(JdVopReadRequest::Detail { sku_id: line.sku.spec_id.clone(), query_ext_set: vec![13] })
            .await?;
        if detail.sku_id.0 != line.sku.spec_id {
            return Err(mapping("京东预检商品资料与冻结编号不一致"));
        }
        purchase_limits(&detail)?
            .ensure_quantity(line.quantity)
            .map_err(|_| mapping("京东采购数量不满足当前起购或整倍数要求"))?;
        let source_unit = detail
            .sale_unit
            .as_deref()
            .map(str::trim)
            .filter(|unit| !unit.is_empty())
            .ok_or_else(|| mapping("京东订购单位未取得，请先核实来源资料"))?;
        let unit_price = Self::current_price(reader, line).await?;
        Ok(PreflightLine {
            line_id: line.line_id.clone(),
            sku: line.sku.clone(),
            quantity: line.quantity,
            unit_price,
            source_unit: Some(source_unit.into()),
            quantity_satisfied: true,
            purchase_allowed: true,
        })
    }

    async fn current_price(reader: &ReadClient, line: &OrderLine) -> ConnectorResult<UnitPrice> {
        let price: Vec<SellPrice> = reader
            .read(JdVopReadRequest::Price {
                sku_id: line.sku.spec_id.clone(),
                quantity: line.quantity.to_string(),
            })
            .await?;
        let price = price
            .first()
            .filter(|_| price.len() == 1)
            .ok_or_else(|| mapping("京东未返回本次数量的唯一采购报价"))?;
        if !price.purchase_restrictions()?.is_empty() {
            return Err(mapping("京东当前报价需要用户资格或促销限购核验，暂不支持自动采购"));
        }
        let unit_price: UnitPrice = price
            .sale_price
            .as_ref()
            .ok_or_else(|| mapping("京东本次数量暂无采购报价"))?
            .0
            .parse()
            .map_err(|_| mapping("京东预检单价格式无效"))?;
        if price.sku_id.0 != line.sku.spec_id || unit_price != line.approved_unit_price {
            return Err(mapping("京东采购报价与已批准单价不一致"));
        }
        verify_tax(line, price)?;
        Ok(unit_price)
    }
}

fn verify_eligibility(
    request: &CreateOrder,
    eligibility: &[SaleEligibility],
    stock: &[Stock],
) -> ConnectorResult<()> {
    let original_count = eligibility.len();
    let eligibility =
        eligibility.iter().map(|value| (value.sku_id.0.as_str(), value)).collect::<BTreeMap<_, _>>();
    let inventory = stock.iter().map(|value| (value.sku_id.0.as_str(), value)).collect::<BTreeMap<_, _>>();
    if eligibility.len() != request.lines.len()
        || eligibility.len() != original_count
        || inventory.len() != request.lines.len()
        || stock.len() != inventory.len()
    {
        return Err(mapping("京东可采或库存核验未完整覆盖采购行"));
    }
    for line in &request.lines {
        let eligibility =
            eligibility.get(line.sku.spec_id.as_str()).ok_or_else(|| mapping("京东可采核验缺少商品"))?;
        let stock =
            inventory.get(line.sku.spec_id.as_str()).ok_or_else(|| mapping("京东库存核验缺少商品"))?;
        if eligibility.can_purchase.0 != "true" {
            return Err(mapping("京东商品不可采、库存不足或要求预订流程"));
        }
        match stock.stock_state_type.0.as_str() {
            "33" => {},
            "39" | "40" => return Err(mapping("京东商品为在途或需调拨，尚未取得批准交付条件")),
            _ => return Err(mapping("京东商品无现货或库存状态未核实，暂不能自动采购")),
        }
        let remain: i32 = stock.remain_num.0.parse().map_err(|_| mapping("京东库存数量格式无效"))?;
        if remain < -1 || (remain >= 0 && Decimal::from(remain) < line.quantity.to_decimal()) {
            return Err(mapping("京东本次采购数量无法满足"));
        }
    }
    Ok(())
}

fn goods_amount(lines: &[PreflightLine]) -> ConnectorResult<Amount> {
    let sum = lines
        .iter()
        .try_fold(Decimal::ZERO, |total, line| {
            line.unit_price
                .to_decimal()
                .checked_mul(line.quantity.to_decimal())
                .map(round_to_cent)
                .and_then(|value| total.checked_add(value))
        })
        .ok_or_else(|| mapping("京东预检商品金额溢出"))?;
    sum.to_string().parse().map_err(|_| mapping("京东预检商品金额精度无效"))
}

fn verify_tax(line: &OrderLine, price: &SellPrice) -> ConnectorResult<()> {
    let original = line
        .source_purchase_terms
        .as_ref()
        .and_then(|terms| terms.quote_context.as_ref())
        .and_then(|context| context.tax_rate);
    if original.is_some() && percentage(price.tax_rate_percentage.as_ref())? != original {
        return Err(mapping("京东来源税率已变化，请重新核实供给和采购批准范围"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use erp_supply::entity::source_purchase::SourcePurchaseTerms;
    use erp_supply::ports::connector::common::{ActionKey, SupplierSku};
    use erp_supply::ports::connector::offer::DeliveryContext;
    use erp_supply::ports::connector::order::{DeliveryChoice, DeliveryMethod, Recipient};
    use serde_json::{Value, json};

    use super::super::transport::JdVopReadTransport;
    use super::*;

    struct Transport {
        detail: Value,
        price: Value,
        requests: Mutex<Vec<JdVopReadRequest>>,
    }

    #[async_trait]
    impl JdVopReadTransport for Transport {
        async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            self.requests.lock().unwrap().push(request.clone());
            let result = match request {
                JdVopReadRequest::Detail { query_ext_set, .. } => {
                    assert_eq!(query_ext_set, &[13]);
                    self.detail.clone()
                },
                JdVopReadRequest::Price { quantity, .. } => {
                    assert_eq!(quantity, "8");
                    self.price.clone()
                },
                _ => panic!("unexpected preflight request"),
            };
            let key = format!("{}_responce", request.method().replace('.', "_"));
            Ok(serde_json::to_vec(
                &json!({key:{"openRpcResult":{"success":true,"resultCode":"0000","result":result}}}),
            )
            .unwrap())
        }
    }

    fn line(quantity: &str) -> OrderLine {
        OrderLine {
            line_id: "original-line".into(),
            sku: SupplierSku { product_id: None, spec_id: "123".into() },
            quantity: quantity.parse().unwrap(),
            approved_unit_price: "10".parse().unwrap(),
            source_purchase_terms: Some(SourcePurchaseTerms {
                quantity_multiple: Some("2".parse().unwrap()),
                ..Default::default()
            }),
            attributes: vec![],
        }
    }

    fn reader(detail: Value) -> (ReadClient, Arc<Transport>) {
        let transport = Arc::new(Transport {
            detail,
            price: json!([{"skuId":"123","salePrice":"10"}]),
            requests: Mutex::new(vec![]),
        });
        (ReadClient::new(transport.clone(), 2, 100, 5), transport)
    }

    #[tokio::test]
    async fn actual_preflight_returns_latest_unit_and_rechecks_current_multiple() {
        let detail =
            json!({"skuId":"123","skuName":"x","lowestBuy":"2","saleUnit":"pack","skuExtMap":{"sfbsgm":"4"}});
        let (reader, transport) = reader(detail);
        let checked = JdVopConnector::verify_line_price(&reader, &line("8")).await.unwrap();
        assert_eq!(checked.source_unit.as_deref(), Some("pack"));
        assert_eq!(checked.quantity, "8".parse().unwrap());
        assert_eq!(transport.requests.lock().unwrap().len(), 2);
        let error = JdVopConnector::verify_line_price(&reader, &line("6")).await.unwrap_err();
        assert!(error.summary.contains("整倍数"));
        assert_eq!(transport.requests.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn missing_latest_unit_blocks_before_price_or_any_write() {
        for unit in [Value::Null, json!(" ")] {
            let (reader, transport) = reader(json!({"skuId":"123","skuName":"x","saleUnit":unit}));
            let error = JdVopConnector::verify_line_price(&reader, &line("8")).await.unwrap_err();
            assert!(error.summary.contains("订购单位未取得"));
            assert_eq!(transport.requests.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn current_promotional_quote_is_blocked_even_if_old_frozen_price_matches() {
        for flags in [
            json!({"hasPromotion":true,"promotionType":2}),
            json!({"hasPromotion":true,"promotionType":1}),
            json!({"promotionType":99}),
        ] {
            let mut price = json!({"skuId":"123","salePrice":"10"});
            price.as_object_mut().unwrap().extend(flags.as_object().unwrap().clone());
            let transport = Arc::new(Transport {
                detail: json!({"skuId":"123","skuName":"x","saleUnit":"pack"}),
                price: json!([price]),
                requests: Mutex::new(vec![]),
            });
            let reader = ReadClient::new(transport.clone(), 2, 100, 5);
            let error = JdVopConnector::verify_line_price(&reader, &line("8")).await.unwrap_err();
            assert!(error.summary.contains("用户资格或促销限购核验"));
            assert_eq!(transport.requests.lock().unwrap().len(), 2);
        }
    }

    #[test]
    fn inventory_requires_spot_stock_and_never_accepts_transit_or_fractional_counts() {
        let request = CreateOrder {
            action: ActionKey { id: "create".into(), payload_hash: "hash".into() },
            merchant_order_no: "original".into(),
            lines: vec![line("8")],
            recipient: Recipient {
                name: "Buyer".into(),
                phone: "phone".into(),
                region: "region".into(),
                address: "road".into(),
                geocoded_address: None,
                supplier_address: None,
            },
            delivery_context: DeliveryContext { reference: "context".into(), supplier_address: None },
            delivery: DeliveryChoice {
                method: DeliveryMethod::Courier,
                shipping_fee: Amount::zero(),
                reference: "choice".into(),
            },
            approved_total: "80".parse().unwrap(),
        };
        let eligible = vec![serde_json::from_value(json!({"skuId":"123","canPurchase":true})).unwrap()];
        for (state, remain, expected) in [
            ("33", "8", true),
            ("33", "-1", true),
            ("33", "7", false),
            ("39", "8", false),
            ("40", "8", false),
            ("36", "8", false),
            ("33", "-0.5", false),
            ("33", "100.5", false),
        ] {
            let stock = vec![
                serde_json::from_value(json!({"skuId":"123","stockStateType":state,"remainNum":remain}))
                    .unwrap(),
            ];
            assert_eq!(
                verify_eligibility(&request, &eligible, &stock).is_ok(),
                expected,
                "state={state}, remain={remain}"
            );
        }
    }
}
