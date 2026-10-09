use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ActionKey, ConnectorResult, Lookup, Snapshot};
use erp_supply::ports::connector::offer::{
    DeliveryContext, DeliveryGroup, DeliveryPlan, DeliveryQuery, DeliverySource,
};
use erp_supply::ports::connector::order::{
    CoordinateSystem, DeliveryChoice, DeliveryMethod, GeocodedAddress, OrderLine, Recipient,
};
use rust_decimal::Decimal;
use serde_json::{Value, from_str, to_string};

use super::catalog::spec_value;
use super::parsing::{field, fixed, flag, id, mapping, snapshot, text, valid_id};
use super::proof::{self, AddressProof, ChoiceProof};
use super::transport::Request;
use super::{DangaoshushuConnector, error};

type DistributionGroups = BTreeMap<String, (String, Vec<OrderLine>)>;
const MAX_DELIVERY_CHOICES: usize = 5000;

impl DangaoshushuConnector {
    pub(super) fn address_proof(&self, context: &DeliveryContext) -> ConnectorResult<AddressProof> {
        let value: AddressProof = proof::decode(&self.settings.private_key, &context.reference)?;
        if value.connection_id != self.connection_id.as_ref()
            || value.user_id != self.settings.user_id
            || value.configuration_hash != self.binding
        {
            return Err(mapping());
        }
        Ok(value)
    }

    fn address_parameters(
        &self,
        key: &ActionKey,
        recipient: &Recipient,
    ) -> ConnectorResult<Vec<(String, String)>> {
        valid_id(&key.id)?;
        valid_id(&key.payload_hash)?;
        valid_id(&self.settings.user_id)?;
        let address = self.recipient_geocode(recipient)?;
        let fields = [
            ("user_id", &self.settings.user_id),
            ("city_name", &address.city_name),
            ("area", &address.district),
            ("addr", &recipient.address),
            ("lat", &address.latitude),
            ("lng", &address.longitude),
            ("name", &recipient.name),
            ("phone", &recipient.phone),
        ];
        if fields
            .iter()
            .any(|(_, value)| value.trim().is_empty() || value.trim() != value.as_str() || value.len() > 512)
        {
            return Err(mapping());
        }
        Ok(fields.into_iter().map(|(key, value)| (key.into(), value.clone())).collect())
    }

    fn recipient_geocode<'a>(&self, recipient: &'a Recipient) -> ConnectorResult<&'a GeocodedAddress> {
        let address = recipient.geocoded_address.as_ref().ok_or_else(|| {
            error(
                SupplierFailureClass::CapabilityGap,
                "DGSS_GEOCODE_REQUIRED",
                "蛋糕叔叔下单需要明确的城市、区县与百度坐标",
            )
        })?;
        if address.coordinate_system != CoordinateSystem::Bd09 {
            return Err(mapping());
        }
        let latitude: Decimal = address.latitude.parse().map_err(|_| mapping())?;
        let longitude: Decimal = address.longitude.parse().map_err(|_| mapping())?;
        if latitude < Decimal::from(-90)
            || latitude > Decimal::from(90)
            || longitude < Decimal::from(-180)
            || longitude > Decimal::from(180)
        {
            return Err(mapping());
        }
        if !self.settings.city_regions.values().any(|region| region == &recipient.region) {
            return Err(mapping());
        }
        Ok(address)
    }

    pub(super) fn valid_lines(lines: &[OrderLine]) -> ConnectorResult<()> {
        let mut identities = BTreeSet::new();
        let mut specs = BTreeSet::new();
        if lines.is_empty() || lines.len() > 50 {
            return Err(mapping());
        }
        for line in lines {
            valid_id(&line.line_id)?;
            valid_id(&line.sku.spec_id)?;
            let quantity = line.quantity.to_decimal();
            if !identities.insert(&line.line_id)
                || !specs.insert(&line.sku.spec_id)
                || quantity <= Decimal::ZERO
                || !quantity.fract().is_zero()
                || line.approved_unit_price.to_decimal().is_sign_negative()
            {
                return Err(mapping());
            }
            if line.attributes.iter().any(|attribute| {
                attribute.name != "口味" || attribute.value.contains(',') || attribute.value.trim().is_empty()
            }) || line.attributes.len() > 1
            {
                return Err(mapping());
            }
        }
        Ok(())
    }

    async fn grouped_lines(&self, lines: &[OrderLine], city_id: &str) -> ConnectorResult<DistributionGroups> {
        let mut groups = BTreeMap::new();
        for line in lines {
            let product = self.product(&line.sku, Some(city_id)).await?;
            if spec_value(&product, &line.sku)?.is_none() {
                return Err(error(
                    SupplierFailureClass::BusinessRejected,
                    "DGSS_SPEC_NOT_VISIBLE",
                    "供应商当前商品未包含所选规格",
                ));
            }
            let rule = distribution_rule(&product)?;
            let brand = id(&product["brand"]["id"])?;
            let group: &mut (String, Vec<OrderLine>) =
                groups.entry(rule).or_insert_with(|| (brand.clone(), vec![]));
            if group.0 != brand {
                return Err(mapping());
            }
            group.1.push(line.clone());
        }
        Ok(groups)
    }

    async fn group_choices(
        &self,
        context: &DeliveryContext,
        address: &AddressProof,
        rule: &str,
        brand: &str,
        lines: &[OrderLine],
    ) -> ConnectorResult<Vec<DeliveryChoice>> {
        let parameters = distribution_parameters(address, lines);
        let value = self
            .transport
            .send(Request::Multipart {
                path: "/dsapi/order/get_distribution_rules",
                parameters,
                write: false,
                not_after: None,
            })
            .await?;
        if !flag(&value["is_distribution"])? {
            return Err(error(
                SupplierFailureClass::BusinessRejected,
                "DGSS_DELIVERY_REJECTED",
                "供应商不配送此地址或商品",
            ));
        }
        let mut choices = self.delivery_choices(&value)?;
        if flag(&value["can_take"])? {
            append_choices(&mut choices, self.pickup_choices(&value, rule, brand, &address.city_id).await?)?;
        }
        if choices.is_empty() {
            return Err(error(
                SupplierFailureClass::BusinessRejected,
                "DGSS_NO_DELIVERY_SLOT",
                "供应商没有合法配送选项",
            ));
        }
        for choice in &mut choices {
            choice.reference = self.choice_reference(context, rule, lines, &value, choice)?;
        }
        Ok(choices)
    }

    fn choice_reference(
        &self,
        context: &DeliveryContext,
        rule: &str,
        lines: &[OrderLine],
        value: &Value,
        choice: &DeliveryChoice,
    ) -> ConnectorResult<String> {
        let shop = if matches!(choice.method, DeliveryMethod::Pickup { .. }) {
            Some(from_str::<Value>(&choice.reference).map_err(|_| mapping())?)
        } else {
            None
        };
        proof::encode(
            &self.settings.private_key,
            &ChoiceProof {
                connection_id: self.connection_id.to_string(),
                configuration_hash: self.binding.clone(),
                context_hash: proof::hash(&self.settings.private_key, context.reference.as_bytes()),
                lines_hash: proof::lines_hash(&self.settings.private_key, lines)?,
                rule_id: rule.into(),
                method: proof::method_key(&choice.method),
                shipping_fee: choice.shipping_fee.to_string(),
                ship_time_text: if matches!(choice.method, DeliveryMethod::Courier) {
                    field(value, "delivery_text")?
                } else {
                    String::new()
                },
                shop_name: shop.as_ref().map(|shop| field(shop, "shop_name")).transpose()?,
                shop_detail: shop.as_ref().map(|shop| field(shop, "address")).transpose()?,
                expires_at: Instant::now().unix_secs().checked_add(60).ok_or_else(mapping)?,
            },
        )
    }

    fn delivery_choices(&self, value: &Value) -> ConnectorResult<Vec<DeliveryChoice>> {
        let mut choices = Vec::new();
        if flag(&value["can_same"])? {
            let shipping_fee: Amount = fixed(&value["validate_same_row"]["delivery_amount"])?;
            if shipping_fee.to_decimal().is_sign_negative() {
                return Err(mapping());
            }
            push_choice(
                &mut choices,
                DeliveryChoice { method: DeliveryMethod::Courier, shipping_fee, reference: String::new() },
            )?;
        }
        if flag(&value["can_ship"])? {
            for day in value["validate_delivery_dates"].as_array().ok_or_else(mapping)? {
                let date = field(day, "date")?.parse().map_err(|_| mapping())?;
                let shipping_fee: Amount = fixed(&day["delivery_amount"])?;
                if shipping_fee.to_decimal().is_sign_negative() {
                    return Err(mapping());
                }
                for slot in day["validate_delivery_times"].as_array().ok_or_else(mapping)? {
                    push_choice(
                        &mut choices,
                        DeliveryChoice {
                            method: DeliveryMethod::LocalDelivery { date, slot: text(slot)? },
                            shipping_fee,
                            reference: String::new(),
                        },
                    )?;
                }
            }
        }
        Ok(choices)
    }

    async fn pickup_choices(
        &self,
        value: &Value,
        rule: &str,
        brand: &str,
        city: &str,
    ) -> ConnectorResult<Vec<DeliveryChoice>> {
        let stores = self.shops(brand, city).await?;
        if id(&stores["rule_id"])? != rule
            || id(&stores["brand_id"])? != brand
            || id(&stores["city"]["id"])? != city
        {
            return Err(mapping());
        }
        let mut choices = Vec::new();
        for day in value["validate_take_dates"].as_array().ok_or_else(mapping)? {
            let date = field(day, "date")?.parse().map_err(|_| mapping())?;
            for slot in day["validate_take_times"].as_array().ok_or_else(mapping)? {
                for store in stores["shops"].as_array().ok_or_else(mapping)? {
                    push_choice(
                        &mut choices,
                        DeliveryChoice {
                            method: DeliveryMethod::Pickup {
                                store_id: id(&store["shop_id"])?,
                                date,
                                slot: text(slot)?,
                            },
                            shipping_fee: "0".parse().map_err(|_| mapping())?,
                            reference: to_string(store).map_err(|_| mapping())?,
                        },
                    )?;
                }
            }
        }
        Ok(choices)
    }
}

fn push_choice(choices: &mut Vec<DeliveryChoice>, choice: DeliveryChoice) -> ConnectorResult<()> {
    if choices.len() >= MAX_DELIVERY_CHOICES {
        return Err(mapping());
    }
    choices.push(choice);
    Ok(())
}

fn append_choices(choices: &mut Vec<DeliveryChoice>, additional: Vec<DeliveryChoice>) -> ConnectorResult<()> {
    if choices.len().checked_add(additional.len()).ok_or_else(mapping)? > MAX_DELIVERY_CHOICES {
        return Err(mapping());
    }
    choices.extend(additional);
    Ok(())
}

fn distribution_rule(product: &Value) -> ConnectorResult<String> {
    let value = &product["product"]["distribution_rule_id"];
    let rule = if value.is_null() || value.as_str() == Some("") || text(value)? == "0" {
        id(&product["brand"]["distribution_rule_id"])?
    } else {
        id(value)?
    };
    if rule == "0" {
        return Err(mapping());
    }
    Ok(rule)
}

fn distribution_parameters(address: &AddressProof, lines: &[OrderLine]) -> Vec<(String, String)> {
    vec![
        ("city_id".into(), address.city_id.clone()),
        ("addr_id".into(), address.addr_id.clone()),
        ("spec_id".into(), lines.iter().map(|line| line.sku.spec_id.as_str()).collect::<Vec<_>>().join(",")),
        (
            "quantitys".into(),
            lines
                .iter()
                .map(|line| line.quantity.to_decimal().normalize().to_string())
                .collect::<Vec<_>>()
                .join(","),
        ),
    ]
}

#[async_trait]
impl DeliverySource for DangaoshushuConnector {
    async fn prepare(&self, key: &ActionKey, recipient: &Recipient) -> ConnectorResult<DeliveryContext> {
        let parameters = self.address_parameters(key, recipient)?;
        let value = self
            .transport
            .send(Request::Form { path: "/dsapi/addr/oprate_addr", parameters, write: true })
            .await?;
        let build = || {
            let city_id = id(&value["city_id"])?;
            if self.settings.city_regions.get(&city_id) != Some(&recipient.region)
                || id(&value["user_id"])? != self.settings.user_id
            {
                return Err(mapping());
            }
            proof::encode(
                &self.settings.private_key,
                &AddressProof {
                    connection_id: self.connection_id.to_string(),
                    configuration_hash: self.binding.clone(),
                    action_id: key.id.clone(),
                    payload_hash: key.payload_hash.clone(),
                    user_id: self.settings.user_id.clone(),
                    addr_id: id(&value["id"])?,
                    city_id,
                    recipient_hash: proof::recipient_hash(&self.settings.private_key, recipient)?,
                },
            )
        };
        let reference = build().map_err(|_| {
            error(
                SupplierFailureClass::ResultUnknown,
                "DGSS_ADDRESS_RESULT_UNKNOWN",
                "供应商可能已创建地址，须调查原地址准备动作",
            )
        })?;
        Ok(DeliveryContext { reference })
    }

    async fn context(&self, _key: &ActionKey) -> ConnectorResult<Lookup<DeliveryContext>> {
        Err(error(
            SupplierFailureClass::CapabilityGap,
            "DGSS_ADDRESS_RECOVERY_UNSUPPORTED",
            "供应商未提供按原动作恢复地址的能力，须人工核验",
        ))
    }

    async fn options(&self, query: &DeliveryQuery) -> ConnectorResult<Snapshot<DeliveryPlan>> {
        Self::valid_lines(&query.lines)?;
        let address = self.address_proof(&query.context)?;
        let mut groups = Vec::new();
        for (rule, (brand, lines)) in self.grouped_lines(&query.lines, &address.city_id).await? {
            let choices = self.group_choices(&query.context, &address, &rule, &brand, &lines).await?;
            groups.push(DeliveryGroup {
                line_ids: lines.iter().map(|line| line.line_id.clone()).collect(),
                choices,
            });
        }
        Ok(snapshot(DeliveryPlan { groups, valid_until: None }, None))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use erp_supply::entity::supplier_api::ConnectionEnvironment;
    use erp_supply::ports::connector::common::SupplierSku;
    use serde_json::json;

    use super::super::test_support::{FakeTransport, connector, settings};
    use super::*;

    fn recipient() -> Recipient {
        Recipient {
            name: "张三".into(),
            phone: "13800000000".into(),
            region: "110100".into(),
            address: "玉泉路1号".into(),
            geocoded_address: Some(GeocodedAddress {
                city_name: "北京市".into(),
                district: "海淀区".into(),
                latitude: "39.91".into(),
                longitude: "116.4".into(),
                coordinate_system: CoordinateSystem::Bd09,
            }),
        }
    }

    fn line(index: u32, quantity: &str) -> OrderLine {
        OrderLine {
            line_id: format!("line-{index}"),
            sku: SupplierSku {
                product_id: Some(format!("product-{index}")),
                spec_id: format!("spec-{index}"),
            },
            quantity: quantity.parse().unwrap(),
            approved_unit_price: "10".parse().unwrap(),
            attributes: vec![],
        }
    }

    fn product(index: u32, brand: &str, rule: Value) -> Value {
        json!({
            "product": {"id": format!("product-{index}"), "distribution_rule_id": rule},
            "brand": {"id": brand, "distribution_rule_id": "fallback-rule"},
            "specs": [{"id": format!("spec-{index}")}]
        })
    }

    fn context() -> DeliveryContext {
        let settings = settings();
        DeliveryContext {
            reference: proof::encode(
                &settings.private_key,
                &AddressProof {
                    configuration_hash: proof::binding_hash(&settings, &super::super::test_support::target())
                        .unwrap(),
                    connection_id: "connection-1".into(),
                    user_id: settings.user_id,
                    action_id: "address-action".into(),
                    payload_hash: "hash".into(),
                    addr_id: "address-1".into(),
                    city_id: "2".into(),
                    recipient_hash: "recipient-hash".into(),
                },
            )
            .unwrap(),
        }
    }

    fn courier_rules() -> Value {
        json!({"is_distribution": "1", "can_take": "0", "can_ship": "0", "can_same": "1",
            "validate_same_row": {"delivery_amount": "8"}, "delivery_text": "快递配送"})
    }

    #[test]
    fn rejects_missing_address_geocode_before_side_effects() {
        let connector = connector(vec![]);
        let key = ActionKey { id: "address-action".into(), payload_hash: "hash".into() };
        let recipient = Recipient {
            name: "张三".into(),
            phone: "13800000000".into(),
            region: "110100".into(),
            address: "玉泉路1号".into(),
            geocoded_address: None,
        };
        assert_eq!(
            connector.address_parameters(&key, &recipient).unwrap_err().class,
            SupplierFailureClass::CapabilityGap
        );
    }
    #[test]
    fn delivery_has_no_empty_or_negative_fee_success() {
        let connector = connector(vec![]);
        let value = json!({"can_same":"1","can_ship":"0","validate_same_row":{"delivery_amount":"-1"}});
        assert!(connector.delivery_choices(&value).is_err());
        let value = json!({"can_same":"1","can_ship":"0","validate_same_row":{}});
        assert!(connector.delivery_choices(&value).is_err());
        for amount in [Value::Null, json!("-0.01")] {
            let value = json!({"can_same":"0","can_ship":"1","validate_delivery_dates":[{
                "date":"2026-10-10","delivery_amount":amount,"validate_delivery_times":["10:00-12:00"]}]});
            assert!(connector.delivery_choices(&value).is_err());
        }
        let value = json!({"can_same":"0","can_ship":"1","validate_delivery_dates":[{
            "date":"2026-10-10","delivery_amount":"0","validate_delivery_times":[""]}]});
        assert!(connector.delivery_choices(&value).is_err());
    }

    #[test]
    fn local_and_merged_delivery_choices_share_bounded_cartesian_limit() {
        let connector = connector(vec![]);
        let value = json!({"can_same":"0","can_ship":"1","validate_delivery_dates":[{
            "date":"2026-10-10","delivery_amount":"0",
            "validate_delivery_times": vec!["10:00-12:00"; MAX_DELIVERY_CHOICES + 1]}]});
        assert!(connector.delivery_choices(&value).is_err());
        let choice = DeliveryChoice {
            method: DeliveryMethod::Courier,
            shipping_fee: "0".parse().unwrap(),
            reference: String::new(),
        };
        let mut choices = vec![choice.clone()];
        assert!(append_choices(&mut choices, vec![choice.clone(); MAX_DELIVERY_CHOICES]).is_err());
        assert_eq!(choices.len(), 1);
        append_choices(&mut choices, vec![choice; MAX_DELIVERY_CHOICES - 1]).unwrap();
        assert_eq!(choices.len(), MAX_DELIVERY_CHOICES);
    }

    #[test]
    fn old_address_context_is_rejected_after_host_or_backend_environment_change() {
        let context = context();
        for changed in ["host", "environment"] {
            let mut changed_target = super::super::test_support::target();
            let mut changed_settings = settings();
            if changed == "host" {
                changed_settings.base_url = "https://other.dangaoss.cn".into();
            } else {
                changed_target.environment = ConnectionEnvironment::Production;
            }
            let connector = DangaoshushuConnector::bind(
                changed_settings,
                &changed_target,
                Arc::new(FakeTransport { responses: Mutex::new(vec![]), requests: Mutex::new(vec![]) }),
            )
            .unwrap();
            assert!(connector.address_proof(&context).is_err());
        }
    }

    #[tokio::test]
    async fn invalid_geocode_and_unmapped_region_do_not_create_address() {
        let transport =
            Arc::new(FakeTransport { responses: Mutex::new(vec![]), requests: Mutex::new(vec![]) });
        let connector = DangaoshushuConnector::with_transport(settings(), transport.clone());
        let key = ActionKey { id: "address-action".into(), payload_hash: "hash".into() };
        for (latitude, longitude) in [("90.0001", "0"), ("0", "-180.0001"), ("abc", "0")] {
            let mut recipient = recipient();
            let geo = recipient.geocoded_address.as_mut().unwrap();
            geo.latitude = latitude.into();
            geo.longitude = longitude.into();
            assert!(connector.prepare(&key, &recipient).await.is_err());
        }
        let mut wrong_system = recipient();
        wrong_system.geocoded_address.as_mut().unwrap().coordinate_system = CoordinateSystem::Gcj02;
        assert!(connector.prepare(&key, &wrong_system).await.is_err());
        let mut unknown_region = recipient();
        unknown_region.region = "310100".into();
        assert!(connector.prepare(&key, &unknown_region).await.is_err());
        let mut boundary = recipient();
        let geo = boundary.geocoded_address.as_mut().unwrap();
        geo.latitude = "-90".into();
        geo.longitude = "180".into();
        assert!(connector.address_parameters(&key, &boundary).is_ok());
        assert!(transport.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn invalid_created_address_response_is_unknown_after_one_write() {
        let transport = Arc::new(FakeTransport {
            responses: Mutex::new(vec![Ok(
                json!({"id": "addr-1", "city_id": "2", "user_id": "foreign-user"}),
            )]),
            requests: Mutex::new(vec![]),
        });
        let connector = DangaoshushuConnector::with_transport(settings(), transport.clone());
        let key = ActionKey { id: "address-action".into(), payload_hash: "hash".into() };
        let error = connector.prepare(&key, &recipient()).await.err().unwrap();
        assert_eq!(error.class, SupplierFailureClass::ResultUnknown);
        let requests = transport.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        assert!(matches!(&requests[0], Request::Form { write: true, .. }));
    }

    #[test]
    fn validates_whole_units_unique_specs_and_product_rule_priority() {
        for quantity in ["0", "-1", "1.5"] {
            assert!(DangaoshushuConnector::valid_lines(&[line(1, quantity)]).is_err());
        }
        assert!(DangaoshushuConnector::valid_lines(&[]).is_err());
        let mut duplicate = line(2, "1");
        duplicate.sku.spec_id = "spec-1".into();
        assert!(DangaoshushuConnector::valid_lines(&[line(1, "1"), duplicate]).is_err());
        assert!(DangaoshushuConnector::valid_lines(&[line(1, "1"), line(2, "2")]).is_ok());
        assert_eq!(distribution_rule(&product(1, "brand-1", json!(123))).unwrap(), "123");
        for value in [Value::Null, json!(""), json!("0"), json!(0)] {
            assert_eq!(distribution_rule(&product(1, "brand-1", value)).unwrap(), "fallback-rule");
        }
        let mut value = product(1, "brand-1", json!(0));
        value["brand"]["distribution_rule_id"] = json!("0");
        assert!(distribution_rule(&value).is_err());
    }

    #[tokio::test]
    async fn groups_bind_spec_quantities_and_sign_exact_lines_fee_and_rule() {
        let transport = Arc::new(FakeTransport {
            responses: Mutex::new(vec![
                Ok(product(1, "brand-1", json!("rule-1"))),
                Ok(product(2, "brand-1", json!("rule-1"))),
                Ok(courier_rules()),
            ]),
            requests: Mutex::new(vec![]),
        });
        let connector = DangaoshushuConnector::with_transport(settings(), transport.clone());
        let query = DeliveryQuery { context: context(), lines: vec![line(1, "1"), line(2, "2")] };
        let plan = connector.options(&query).await.unwrap().value;
        assert_eq!(plan.groups.len(), 1);
        assert_eq!(plan.groups[0].line_ids, vec!["line-1", "line-2"]);
        let choice = &plan.groups[0].choices[0];
        assert_eq!(choice.shipping_fee.to_string(), "8");
        let proof: ChoiceProof = proof::decode(&settings().private_key, &choice.reference).unwrap();
        assert_eq!(proof.rule_id, "rule-1");
        assert_eq!(proof.lines_hash, proof::lines_hash(&settings().private_key, &query.lines).unwrap());
        assert_eq!(proof.method, "same");
        let requests = transport.requests.lock().unwrap();
        let Request::Multipart { parameters, write, .. } = &requests[2] else {
            panic!("配送规则只读 multipart")
        };
        assert!(!write);
        assert!(parameters.contains(&("spec_id".into(), "spec-1,spec-2".into())));
        assert!(parameters.contains(&("quantitys".into(), "1,2".into())));
    }

    #[tokio::test]
    async fn rejects_conflicting_brand_and_missing_spec_before_delivery_query() {
        let connector =
            connector(vec![product(1, "brand-1", json!("rule-1")), product(2, "brand-2", json!("rule-1"))]);
        assert_eq!(
            connector.grouped_lines(&[line(1, "1"), line(2, "1")], "2").await.err().unwrap().class,
            SupplierFailureClass::MappingError
        );
        let mut missing = product(1, "brand-1", json!("rule-1"));
        missing["specs"] = json!([]);
        let error = super::super::test_support::connector(vec![missing])
            .grouped_lines(&[line(1, "1")], "2")
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "DGSS_SPEC_NOT_VISIBLE");
    }

    #[tokio::test]
    async fn no_slots_are_business_rejection_and_pickup_store_must_match_group() {
        let mut value = courier_rules();
        value["can_same"] = json!("0");
        let error = connector(vec![value.clone()])
            .group_choices(
                &context(),
                &proof::decode(&settings().private_key, &context().reference).unwrap(),
                "rule-1",
                "brand-1",
                &[line(1, "1")],
            )
            .await
            .err()
            .unwrap();
        assert_eq!(error.code, "DGSS_NO_DELIVERY_SLOT");
        value["can_take"] = json!("1");
        value["validate_take_dates"] = json!([{"date":"2026-10-10","validate_take_times":["10:00-12:00"]}]);
        let stores = json!({"rule_id":"foreign-rule","brand_id":"brand-1","city":{"id":"2"},"shops":[]});
        let error =
            connector(vec![stores]).pickup_choices(&value, "rule-1", "brand-1", "2").await.err().unwrap();
        assert_eq!(error.class, SupplierFailureClass::MappingError);
    }
}
