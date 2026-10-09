use std::collections::BTreeSet;
use std::num::NonZeroU32;

use async_trait::async_trait;
use chrono::{DateTime, FixedOffset};
use erp_core::common::time::{BUSINESS_TZ_OFFSET_SECS, Instant};
use erp_core::money::{Quantity, UnitPrice};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{
    ConnectorResult, Lookup, Page, Scan, ScanWindow, Snapshot, SupplierSku,
};
use erp_supply::ports::connector::offer::{
    Availability, AvailabilityBlock, AvailabilityQuery, AvailabilitySource, AvailabilityState,
    AvailableQuantity, IncrementalSupport, Offer, OfferChange, OfferSource, PriceTier, RegionCoverage,
    ServiceAreas, SupplyQuote,
};
use rust_decimal::Decimal;
use serde_json::Value;

use super::parsing::{field, fixed, flag, id, mapping, snapshot, source_time, valid_id};
use super::transport::Request;
use super::{DangaoshushuConnector, error};

type Parameters = Vec<(String, String)>;

impl DangaoshushuConnector {
    /// 查询新版品牌及开通城市，保留字符串品牌 ID 与供应商原始事实。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 供应商完整品牌列表；不修改公司品牌主档。
    /// # 错误
    /// 外部调用失败或响应非数组时返回分类错误。
    pub async fn brands(&self) -> ConnectorResult<Value> {
        let value = self
            .transport
            .send(Request::Read { path: "/dsapi/brand/brand_city_lists", parameters: vec![] })
            .await?;
        if !value.is_array() {
            return Err(mapping());
        }
        Ok(value)
    }

    /// 查询新版门店资料，供冻结自提门店选项。
    ///
    /// # 参数
    /// `brand_id` 为本连接的品牌号；`city_id` 为供应商城市号。
    /// # 返回
    /// 原始门店事实，不作为公司地区字典。
    /// # 错误
    /// 身份非法或查询失败时返回分类错误。
    pub async fn shops(&self, brand_id: &str, city_id: &str) -> ConnectorResult<Value> {
        valid_id(brand_id)?;
        valid_id(city_id)?;
        self.transport
            .send(Request::Read {
                path: "/dsapi/brand/get_shop_lists",
                parameters: vec![("brand_id".into(), brand_id.into()), ("city_id".into(), city_id.into())],
            })
            .await
    }

    pub(super) async fn product(&self, sku: &SupplierSku, city_id: Option<&str>) -> ConnectorResult<Value> {
        valid_id(&sku.spec_id)?;
        let product_id = sku.product_id.as_deref().ok_or_else(mapping)?;
        valid_id(product_id)?;
        let mut parameters = vec![("product_id".into(), product_id.into())];
        if let Some(city) = city_id {
            parameters.push(("city_id".into(), city.into()));
        }
        let product = self
            .transport
            .send(Request::Read { path: "/dsapi/product/get_product_details", parameters })
            .await?;
        if id(&product["product"]["id"])? != product_id {
            return Err(mapping());
        }
        Ok(product)
    }

    fn offer_value(&self, product: &Value, spec: &Value, detail: bool) -> ConnectorResult<Offer> {
        let spec_id = id(&spec[if detail { "id" } else { "spec_id" }])?;
        let unit = self.settings.spec_units.get(&spec_id).cloned();
        let quote = if self.settings.clearing_price_is_tax_inclusive_cny && unit.is_some() {
            let unit_price: UnitPrice = fixed(&spec["clearing_price"])?;
            if unit_price.to_decimal() < Decimal::ZERO {
                return Err(mapping());
            }
            Some(SupplyQuote {
                dropship: Some(PriceTier {
                    unit_price,
                    minimum_quantity: "1".parse().map_err(|_| mapping())?,
                }),
                bulk: None,
                tax_rate: None,
                valid_until: None,
            })
        } else {
            None
        };
        Ok(Offer {
            sku: SupplierSku { product_id: Some(id(&product["id"])?), spec_id },
            name: field(product, "title")?,
            specification: field(spec, "name")?,
            unit,
            quote,
        })
    }

    fn query_city(&self, region: Option<&String>) -> ConnectorResult<Option<&str>> {
        region
            .map(|region| {
                self.settings
                    .city_regions
                    .iter()
                    .find(|(_, mapped)| *mapped == region)
                    .map(|(city, _)| city.as_str())
                    .ok_or_else(mapping)
            })
            .transpose()
    }
}

fn scan_parameters(scan: &Scan) -> ConnectorResult<(u32, Parameters)> {
    if scan.limit.get() > 50 {
        return Err(mapping());
    }
    let page: u32 = scan.cursor.as_deref().unwrap_or("1").parse().map_err(|_| mapping())?;
    if page == 0 {
        return Err(mapping());
    }
    let mut parameters = vec![
        ("sort_price_type".into(), "1".into()),
        ("page".into(), page.to_string()),
        ("size".into(), scan.limit.to_string()),
    ];
    if let ScanWindow::Changes { since, through } = scan.window {
        if since > through {
            return Err(mapping());
        }
        let zone = FixedOffset::east_opt(BUSINESS_TZ_OFFSET_SECS).expect("业务时区合法");
        let day = |instant: Instant| {
            DateTime::from_timestamp(instant.unix_secs(), 0)
                .map(|date| date.with_timezone(&zone).date_naive())
                .ok_or_else(mapping)
        };
        // 文档只接受日期且未声明结束日包含关系，向后扩展一天覆盖 through 当日全部变化。
        let end = day(through)?.succ_opt().ok_or_else(mapping)?;
        parameters.push(("start_time".into(), day(since)?.format("%Y-%m-%d").to_string()));
        parameters.push(("end_time".into(), end.format("%Y-%m-%d").to_string()));
    }
    Ok((page, parameters))
}

/// 定位商品响应中的请求规格，异常身份不得当作正常缺项。
///
/// # 参数
/// `product` 为已验证商品响应；`sku` 为请求规格。
/// # 返回
/// 当前可见规格或明确的缺项。
/// # 错误
/// 规格集合或身份无法解析时返回映射错误。
pub(super) fn spec_value<'a>(product: &'a Value, sku: &SupplierSku) -> ConnectorResult<Option<&'a Value>> {
    let mut found = None;
    for spec in product["specs"].as_array().ok_or_else(mapping)? {
        if id(&spec["id"])? == sku.spec_id {
            if found.is_some()
                || (!spec["product_id"].is_null() && Some(id(&spec["product_id"])?) != sku.product_id)
            {
                return Err(mapping());
            }
            found = Some(spec);
        }
    }
    Ok(found)
}

fn availability_value(query: &AvailabilityQuery, product: &Value) -> ConnectorResult<Snapshot<Availability>> {
    let sku = &query.skus[0];
    let Some(spec) = spec_value(product, sku)? else {
        return Ok(snapshot(
            Availability {
                sku: sku.clone(),
                state: AvailabilityState::Unknown,
                quantity: AvailableQuantity::Unreported,
                region: query.region.clone(),
            },
            None,
        ));
    };
    let quantity = stock(&spec["stock"])?;
    let state = availability_state(product, spec, &quantity)?;
    Ok(snapshot(
        Availability { sku: sku.clone(), state, quantity, region: query.region.clone() },
        source_time(&spec["updated_at"])?,
    ))
}

fn availability_state(
    product: &Value,
    spec: &Value,
    quantity: &AvailableQuantity,
) -> ConnectorResult<AvailabilityState> {
    let mut reasons = Vec::new();
    for (enabled, reason) in [
        (flag(&product["brand"]["status"])?, AvailabilityBlock::Brand),
        (
            flag(&product["product"]["status"])? && flag(&product["product"]["can_buy"])?,
            AvailabilityBlock::Product,
        ),
        (
            !flag(&spec["deleted"])? && flag(&spec["is_up"])? && flag(&spec["can_buy"])?,
            AvailabilityBlock::Specification,
        ),
    ] {
        if !enabled {
            reasons.push(reason);
        }
    }
    if matches!(quantity, AvailableQuantity::Exact(quantity) if quantity.to_decimal().is_zero()) {
        reasons.push(AvailabilityBlock::Quantity);
    }
    Ok(if reasons.is_empty() {
        AvailabilityState::Available
    } else {
        AvailabilityState::Unavailable { reasons }
    })
}

#[async_trait]
impl OfferSource for DangaoshushuConnector {
    fn incremental_support(&self) -> IncrementalSupport {
        IncrementalSupport::Partial
    }

    async fn scan(&self, scan: &Scan) -> ConnectorResult<Page<Snapshot<OfferChange>>> {
        let (page, parameters) = scan_parameters(scan)?;
        let value = self
            .transport
            .send(Request::Read { path: "/dsapi/product/get_product_hot_lists", parameters })
            .await?;
        let products = value["product_list"].as_array().ok_or_else(mapping)?;
        let mut items = Vec::new();
        for product in products {
            for spec in product["specs"].as_array().ok_or_else(mapping)? {
                items.push(snapshot(OfferChange::Upsert(self.offer_value(product, spec, false)?), None));
            }
        }
        let next = if products.len() >= usize::try_from(scan.limit.get()).map_err(|_| mapping())? {
            Some(page.checked_add(1).ok_or_else(mapping)?.to_string())
        } else {
            None
        };
        Ok(Page { items, next })
    }

    async fn offer(&self, sku: &SupplierSku) -> ConnectorResult<Lookup<Snapshot<Offer>>> {
        let value = self.product(sku, None).await?;
        let Some(spec) = spec_value(&value, sku)? else {
            return Ok(Lookup::NotVisible);
        };
        Ok(Lookup::Found(snapshot(
            self.offer_value(&value["product"], spec, true)?,
            source_time(&spec["updated_at"])?,
        )))
    }
}

#[async_trait]
impl AvailabilitySource for DangaoshushuConnector {
    fn batch_limit(&self) -> NonZeroU32 {
        NonZeroU32::new(1).expect("1 非零")
    }

    async fn availability(&self, query: &AvailabilityQuery) -> ConnectorResult<Vec<Snapshot<Availability>>> {
        if query.skus.len() != 1 {
            return Err(mapping());
        }
        let sku = &query.skus[0];
        let city = self.query_city(query.region.as_ref())?;
        let product = self.product(sku, city).await?;
        Ok(vec![availability_value(query, &product)?])
    }
}

fn stock(value: &Value) -> ConnectorResult<AvailableQuantity> {
    if value.is_null() {
        return Ok(AvailableQuantity::Unreported);
    }
    let quantity: Quantity = fixed(value)?;
    if quantity.to_decimal() == Decimal::from(-9999999) {
        return Ok(AvailableQuantity::Unreported);
    }
    if quantity.to_decimal().is_sign_negative() {
        return Err(mapping());
    }
    Ok(AvailableQuantity::Exact(quantity))
}

#[async_trait]
impl ServiceAreas for DangaoshushuConnector {
    async fn regions(&self, sku: &SupplierSku) -> ConnectorResult<Snapshot<RegionCoverage>> {
        valid_id(&sku.spec_id)?;
        let product = sku.product_id.as_deref().ok_or_else(mapping)?;
        valid_id(product)?;
        let value = self
            .transport
            .send(Request::Read {
                path: "/dsapi/product/get_product_cities_info",
                parameters: vec![("product_id".into(), product.into())],
            })
            .await?;
        let mut regions = BTreeSet::new();
        for city in value.as_array().ok_or_else(mapping)? {
            let id = id(&city["city_id"])?;
            let region = self.settings.city_regions.get(&id).ok_or_else(|| {
                error(
                    SupplierFailureClass::MappingError,
                    "DGSS_REGION_UNMAPPED",
                    "供应商城市尚未绑定公司标准地区",
                )
            })?;
            regions.insert(region.clone());
        }
        Ok(snapshot(RegionCoverage::Included(regions.into_iter().collect()), None))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use serde_json::json;

    use super::super::test_support::{FakeTransport, connector, settings};
    use super::*;

    fn sku() -> SupplierSku {
        SupplierSku { product_id: Some("product-1".into()), spec_id: "spec-1".into() }
    }

    fn product() -> Value {
        json!({
            "brand": {"status": "1"},
            "product": {"id": "product-1", "status": "1", "can_buy": "1"},
            "specs": [{"id": "spec-1", "deleted": "0", "is_up": "1", "can_buy": "1", "stock": "-9999999"}]
        })
    }

    #[test]
    fn sentinel_is_unknown_and_other_negative_stock_is_rejected() {
        assert_eq!(stock(&json!("-9999999")).unwrap(), AvailableQuantity::Unreported);
        assert_eq!(stock(&Value::Null).unwrap(), AvailableQuantity::Unreported);
        assert!(stock(&json!("-1")).is_err());
        assert!(
            matches!(stock(&json!("0")).unwrap(), AvailableQuantity::Exact(quantity) if quantity.to_decimal().is_zero())
        );
    }

    #[tokio::test]
    async fn availability_combines_parent_and_spec_flags_without_inventing_inventory() {
        let query = AvailabilityQuery { skus: vec![sku()], region: Some("110100".into()) };
        let result = connector(vec![product()]).availability(&query).await.unwrap().remove(0).value;
        assert_eq!(result.state, AvailabilityState::Available);
        assert_eq!(result.quantity, AvailableQuantity::Unreported);
        for (parent, key, value, expected) in [
            ("brand", "status", "0", AvailabilityBlock::Brand),
            ("product", "status", "0", AvailabilityBlock::Product),
            ("product", "can_buy", "0", AvailabilityBlock::Product),
            ("spec", "deleted", "1", AvailabilityBlock::Specification),
            ("spec", "is_up", "0", AvailabilityBlock::Specification),
            ("spec", "can_buy", "0", AvailabilityBlock::Specification),
            ("spec", "stock", "0", AvailabilityBlock::Quantity),
        ] {
            let mut value_json = product();
            if parent == "spec" {
                value_json["specs"][0][key] = json!(value);
            } else {
                value_json[parent][key] = json!(value);
            }
            let result = connector(vec![value_json]).availability(&query).await.unwrap().remove(0).value;
            assert_eq!(result.state, AvailabilityState::Unavailable { reasons: vec![expected] });
        }
    }

    #[tokio::test]
    async fn missing_spec_is_unknown_and_unmapped_query_does_not_send_http() {
        let mut value = product();
        value["specs"] = json!([]);
        let query = AvailabilityQuery { skus: vec![sku()], region: None };
        let result = connector(vec![value]).availability(&query).await.unwrap().remove(0).value;
        assert_eq!(result.sku, sku());
        assert_eq!(result.state, AvailabilityState::Unknown);
        let transport =
            Arc::new(FakeTransport { responses: Mutex::new(vec![]), requests: Mutex::new(vec![]) });
        let connector = DangaoshushuConnector::with_transport(settings(), transport.clone());
        let query = AvailabilityQuery { skus: vec![sku()], region: Some("310100".into()) };
        assert_eq!(
            connector.availability(&query).await.unwrap_err().class,
            SupplierFailureClass::MappingError
        );
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[test]
    fn duplicate_spec_and_conflicting_product_identity_are_mapping_errors() {
        let mut value = product();
        value["specs"][0]["product_id"] = json!("foreign-product");
        assert!(spec_value(&value, &sku()).is_err());
        let mut value = product();
        let duplicate = value["specs"][0].clone();
        value["specs"].as_array_mut().unwrap().push(duplicate);
        assert!(spec_value(&value, &sku()).is_err());
    }

    #[tokio::test]
    async fn service_areas_reject_partial_mapping_and_preserve_empty_coverage() {
        let error = connector(vec![json!([{ "city_id": "2" }, { "city_id": "3" }])])
            .regions(&sku())
            .await
            .unwrap_err();
        assert_eq!(error.code, "DGSS_REGION_UNMAPPED");
        let result = connector(vec![json!([])]).regions(&sku()).await.unwrap();
        assert_eq!(result.value, RegionCoverage::Included(vec![]));
        let result =
            connector(vec![json!([{ "city_id": "2" }, { "city_id": 2 }])]).regions(&sku()).await.unwrap();
        assert_eq!(result.value, RegionCoverage::Included(vec!["110100".into()]));
    }

    #[tokio::test]
    async fn incremental_scan_expands_local_day_end_and_keeps_pagination() {
        let transport = Arc::new(FakeTransport {
            responses: Mutex::new(vec![Ok(json!({"product_list": [{"id": "product-1", "specs": []}]}))]),
            requests: Mutex::new(vec![]),
        });
        let connector = DangaoshushuConnector::with_transport(settings(), transport.clone());
        let scan = Scan {
            window: ScanWindow::Changes {
                since: Instant::from_unix_secs(1704124799),
                through: Instant::from_unix_secs(1704124800),
            },
            cursor: None,
            limit: NonZeroU32::new(1).unwrap(),
        };
        assert_eq!(connector.scan(&scan).await.unwrap().next, Some("2".into()));
        let requests = transport.requests.lock().unwrap();
        let Request::Read { parameters, .. } = &requests[0] else { panic!("只读目录请求") };
        assert!(parameters.contains(&("start_time".into(), "2024-01-01".into())));
        assert!(parameters.contains(&("end_time".into(), "2024-01-03".into())));
        let reversed = Scan {
            window: ScanWindow::Changes {
                since: Instant::from_unix_secs(2),
                through: Instant::from_unix_secs(1),
            },
            ..scan
        };
        assert!(scan_parameters(&reversed).is_err());
    }

    #[test]
    fn quotes_require_confirmed_currency_tax_basis_and_explicit_spec_unit() {
        let mut connector = connector(vec![]);
        let product = json!({"id": "product-1", "title": "蛋糕"});
        let spec = json!({"id": "spec-1", "name": "6寸", "clearing_price": "100.25"});
        connector.settings.clearing_price_is_tax_inclusive_cny = false;
        assert!(connector.offer_value(&product, &spec, true).unwrap().quote.is_none());
        connector.settings.clearing_price_is_tax_inclusive_cny = true;
        connector.settings.spec_units.clear();
        let offer = connector.offer_value(&product, &spec, true).unwrap();
        assert_eq!(offer.unit, None);
        assert_eq!(offer.quote, None);
        connector.settings.spec_units.insert("spec-1".into(), "个".into());
        let quote = connector.offer_value(&product, &spec, true).unwrap().quote.unwrap();
        assert_eq!(quote.dropship.unwrap().unit_price.to_string(), "100.25");
        assert_eq!(quote.bulk, None);
        assert_eq!(quote.tax_rate, None);
    }
}
