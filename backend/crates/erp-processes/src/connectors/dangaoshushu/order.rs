use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus};
use erp_supply::ports::connector::common::{
    ConnectorError, ConnectorResult, Lookup, ReplayProtection, Snapshot,
};
use erp_supply::ports::connector::offer::OfferSource;
use erp_supply::ports::connector::order::{
    ConfirmPayment, CreateOrder, CreatedOrder, CreationNext, DeliveryMethod, OrderAmounts, OrderKey,
    OrderReference, OrderSnapshot, Orders, PaymentConfirmation, PaymentState, Shipment,
};
use rust_decimal::Decimal;
use serde_json::{Map, Value, json, to_string};

use super::parsing::{field, fixed, flag, id, mapping, snapshot, source_time, valid_id};
use super::proof::{self, AddressProof, ChoiceProof};
use super::transport::Request;
use super::{DangaoshushuConnector, error};

impl DangaoshushuConnector {
    fn order_payload(&self, request: &CreateOrder) -> ConnectorResult<Value> {
        self.order_payload_at(request, Instant::now())
    }

    fn order_payload_at(&self, request: &CreateOrder, now: Instant) -> ConnectorResult<Value> {
        Self::valid_lines(&request.lines)?;
        valid_id(&request.action.id)?;
        valid_id(&request.action.payload_hash)?;
        valid_id(&request.merchant_order_no)?;
        let address = self.address_proof(&request.delivery_context)?;
        let choice: ChoiceProof = proof::decode(&self.settings.private_key, &request.delivery.reference)?;
        if choice.connection_id != self.connection_id.as_ref()
            || choice.configuration_hash != self.binding
            || choice.context_hash
                != proof::hash(&self.settings.private_key, request.delivery_context.reference.as_bytes())
            || choice.lines_hash != proof::lines_hash(&self.settings.private_key, &request.lines)?
            || choice.method != proof::method_key(&request.delivery.method)
            || choice.shipping_fee != request.delivery.shipping_fee.to_string()
            || choice.expires_at <= now.unix_secs()
            || address.recipient_hash
                != proof::recipient_hash(&self.settings.private_key, &request.recipient)?
        {
            return Err(mapping());
        }
        approved_amount(request)?;
        self.payload_for_choice(request, address, choice)
    }

    fn payload_for_choice(
        &self,
        request: &CreateOrder,
        address: AddressProof,
        choice: ChoiceProof,
    ) -> ConnectorResult<Value> {
        let group = selected_group(&request.delivery.method, &choice)?;
        let quantitys = request
            .lines
            .iter()
            .map(|line| line.quantity.to_decimal().normalize().to_string())
            .collect::<Vec<_>>()
            .join(",");
        let tastes = request
            .lines
            .iter()
            .map(|line| line.attributes.first().map(|attribute| attribute.value.as_str()).unwrap_or(""))
            .collect::<Vec<_>>()
            .join(",");
        let mut groups = Map::new();
        groups.insert(choice.rule_id.clone(), group);
        Ok(json!({
            "out_order_no":request.merchant_order_no,
            "addr":{"id":address.addr_id,"name":request.recipient.name,"phone":request.recipient.phone,"detail":request.recipient.address},
            "rule_ids":vec![choice.rule_id;request.lines.len()].join(","),
            "spec_ids":request.lines.iter().map(|line|line.sku.spec_id.as_str()).collect::<Vec<_>>().join(","),
            "quantitys":quantitys,"tastes_name":tastes,"group":groups,
            "pay_type":self.settings.channel_no,"buyer_phone":request.recipient.phone,
            "user_id":address.user_id,"city_id":address.city_id
        }))
    }

    async fn verify_prices(&self, request: &CreateOrder) -> ConnectorResult<()> {
        if !self.settings.clearing_price_is_tax_inclusive_cny {
            return Err(error(
                SupplierFailureClass::CapabilityGap,
                "DGSS_PRICE_BASIS_UNCONFIRMED",
                "蛋糕叔叔结算价的人民币含税口径尚未确认",
            ));
        }
        for line in &request.lines {
            let Lookup::Found(offer) = self.offer(&line.sku).await? else {
                return Err(price_rejected());
            };
            let quote = offer.value.quote.and_then(|quote| quote.dropship).ok_or_else(mapping)?;
            if quote.unit_price != line.approved_unit_price {
                return Err(price_rejected());
            }
        }
        Ok(())
    }
}

fn approved_amount(request: &CreateOrder) -> ConnectorResult<()> {
    let mut total = request.delivery.shipping_fee.to_decimal();
    if total.is_sign_negative() || request.approved_total.to_decimal() <= Decimal::ZERO {
        return Err(mapping());
    }
    for line in &request.lines {
        total = total
            .checked_add(
                line.quantity
                    .to_decimal()
                    .checked_mul(line.approved_unit_price.to_decimal())
                    .ok_or_else(mapping)?,
            )
            .ok_or_else(mapping)?;
    }
    if total > request.approved_total.to_decimal() {
        return Err(price_rejected());
    }
    Ok(())
}

fn price_rejected() -> ConnectorError {
    error(
        SupplierFailureClass::BusinessRejected,
        "DGSS_APPROVED_PRICE_MISMATCH",
        "供应商报价超出或不符合已批准成本",
    )
}
fn selected_group(method: &DeliveryMethod, choice: &ChoiceProof) -> ConnectorResult<Value> {
    match method {
        DeliveryMethod::Courier => {
            Ok(json!({"ship_type":"same","ship_date":false,"ship_time_text":choice.ship_time_text}))
        },
        DeliveryMethod::LocalDelivery { date, slot } => {
            Ok(json!({"ship_type":"delivery","ship_date":date.to_string(),"ship_time_text":slot}))
        },
        DeliveryMethod::Pickup { store_id, date, slot } => Ok(
            json!({"ship_type":"shop","ship_date":date.to_string(),"ship_time_text":slot,"shop":{"id":store_id,"name":choice.shop_name.as_ref().ok_or_else(mapping)?,"detail":choice.shop_detail.as_ref().ok_or_else(mapping)?}}),
        ),
    }
}

#[async_trait]
impl Orders for DangaoshushuConnector {
    fn replay_protection(&self) -> ReplayProtection {
        ReplayProtection::Unverified
    }

    async fn create(&self, request: &CreateOrder) -> ConnectorResult<CreatedOrder> {
        self.order_payload(request)?;
        self.verify_prices(request).await?;
        // 读价可能耗时；发送前再次核对地址、明细、成本与配送有效期。
        let payload = self.order_payload(request)?;
        let choice: ChoiceProof = proof::decode(&self.settings.private_key, &request.delivery.reference)?;
        let value = self
            .transport
            .send(Request::Multipart {
                path: "/dsapi/order/submit_order",
                parameters: vec![("order_data".into(), to_string(&payload).map_err(|_| mapping())?)],
                write: true,
                not_after: Some(Instant::from_unix_secs(choice.expires_at)),
            })
            .await?;
        // 即使实际金额超批准，保留已创建外部订单；由 Process 阻断支付和登记异常。
        created_order(&value, &request.merchant_order_no).map_err(|_| {
            error(
                SupplierFailureClass::ResultUnknown,
                "DGSS_CREATE_RESULT_UNKNOWN",
                "供应商可能已创建订单，须按原我方单号调查",
            )
        })
    }

    async fn order(&self, key: &OrderKey) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        let (parameter, number) = match key {
            OrderKey::Merchant(number) => ("out_order_no", number),
            OrderKey::External(number) => ("order_no", number),
        };
        valid_id(number)?;
        let value = self
            .transport
            .send(Request::Read {
                path: "/dsapi/order/order_details",
                parameters: vec![(parameter.into(), number.clone())],
            })
            .await?;
        if value.is_null() {
            return Ok(Lookup::NotVisible);
        }
        let order = order_snapshot(&value, &self.settings.channel_no)?;
        let actual = match key {
            OrderKey::Merchant(_) => &order.reference.merchant_order_no,
            OrderKey::External(_) => &order.reference.external_order_no,
        };
        if actual != number {
            return Err(mapping());
        }
        Ok(Lookup::Found(snapshot(order, source_time(&value["updated_at"])?)))
    }
}

fn created_order(value: &Value, merchant_order_no: &str) -> ConnectorResult<CreatedOrder> {
    let order = &value["order"];
    let reference = OrderReference {
        merchant_order_no: merchant_order_no.into(),
        external_order_no: id(&order["order_no"])?,
    };
    let payable: Amount = fixed(&order["final_amount"])?;
    if payable.to_decimal().is_sign_negative() {
        return Err(mapping());
    }
    let payment = if flag(&order["pay_status"])? { PaymentState::Confirmed } else { PaymentState::Pending };
    Ok(CreatedOrder {
        order: snapshot(
            OrderSnapshot {
                reference,
                amounts: Some(OrderAmounts {
                    payable,
                    goods_cost: None,
                    shipping_cost: None,
                    service_cost: None,
                }),
                fulfillment: None,
                cancellation: None,
                refund: None,
                payment: Some(payment),
                shipments: None,
            },
            None,
        ),
        next: if payment == PaymentState::Pending {
            CreationNext::ConfirmPayment
        } else {
            CreationNext::ObserveOrder
        },
    })
}

fn order_snapshot(value: &Value, channel: &str) -> ConnectorResult<OrderSnapshot> {
    if value["channel_no"].as_str() != Some(channel) {
        return Err(mapping());
    }
    let reference = OrderReference {
        merchant_order_no: id(&value["out_order_no"])?,
        external_order_no: id(&value["order_no"])?,
    };
    let status = field(value, "status")?;
    let (fulfillment, cancellation) = match status.as_str() {
        "0" => (None, None),
        "1" => (Some(FulfillmentStatus::Accepted), None),
        "2" => (Some(FulfillmentStatus::Completed), None),
        "3" => (None, Some(CancelStatus::Canceled)),
        _ => return Err(mapping()),
    };
    let payment = if value["pay_status"].is_null() {
        None
    } else {
        Some(if flag(&value["pay_status"])? { PaymentState::Confirmed } else { PaymentState::Pending })
    };
    let amounts = if value["final_amount"].is_null() {
        None
    } else {
        let payable: Amount = fixed(&value["final_amount"])?;
        if payable.to_decimal().is_sign_negative() {
            return Err(mapping());
        }
        Some(OrderAmounts { payable, goods_cost: None, shipping_cost: None, service_cost: None })
    };
    Ok(OrderSnapshot {
        reference,
        amounts,
        fulfillment,
        cancellation,
        refund: None,
        payment,
        shipments: shipments(value)?,
    })
}

fn shipments(value: &Value) -> ConnectorResult<Option<Vec<Shipment>>> {
    if value["express_no"].is_null() || value["express_no"].as_str() == Some("") {
        return Ok(None);
    }
    let number = field(value, "express_no")?;
    let courier = field(value, "ship_type")? != "same";
    Ok(Some(vec![Shipment {
        external_shipment_id: None,
        line_ids: vec![],
        carrier: Some(field(value, "express_cp")?),
        tracking_no: if courier { None } else { Some(number.clone()) },
        courier_phone: if courier { Some(number) } else { None },
    }]))
}

#[async_trait]
impl PaymentConfirmation for DangaoshushuConnector {
    fn replay_protection(&self) -> ReplayProtection {
        ReplayProtection::Unverified
    }

    async fn confirm(&self, request: &ConfirmPayment) -> ConnectorResult<Snapshot<PaymentState>> {
        valid_id(&request.action.id)?;
        valid_id(&request.action.payload_hash)?;
        valid_id(&request.order.merchant_order_no)?;
        valid_id(&request.order.external_order_no)?;
        valid_id(&request.transaction_no)?;
        if request.amount.to_decimal() <= Decimal::ZERO {
            return Err(mapping());
        }
        let value=self.transport.send(Request::Json {path:"/dsapi/order/order_pay_result",payload:json!({"result_status":"2","order_sn":request.order.external_order_no,"out_order_sn":request.order.merchant_order_no,"order_price":request.amount.to_string(),"transaction_sn":request.transaction_no})}).await?;
        if value.as_str() != Some("支付成功") {
            return Err(error(
                SupplierFailureClass::ResultUnknown,
                "DGSS_PAYMENT_RESULT_UNKNOWN",
                "供应商支付确认响应无法核实，须查询原订单",
            ));
        }
        Ok(snapshot(PaymentState::Confirmed, None))
    }
}

#[cfg(test)]
mod tests {
    use erp_supply::ports::connector::common::{ActionKey, SupplierSku};
    use erp_supply::ports::connector::offer::DeliveryContext;
    use erp_supply::ports::connector::order::{
        CoordinateSystem, DeliveryChoice, GeocodedAddress, OrderLine, Recipient,
    };

    use super::super::test_support::{connector, recording_connector};
    use super::*;

    fn request() -> CreateOrder {
        let recipient = Recipient {
            name: "收件人".into(),
            phone: "13800000000".into(),
            region: "110100".into(),
            address: "测试地址".into(),
            geocoded_address: Some(GeocodedAddress {
                city_name: "北京市".into(),
                district: "朝阳区".into(),
                latitude: "39.90".into(),
                longitude: "116.40".into(),
                coordinate_system: CoordinateSystem::Bd09,
            }),
        };
        let lines = vec![OrderLine {
            line_id: "line-1".into(),
            sku: SupplierSku { product_id: Some("product-1".into()), spec_id: "spec-1".into() },
            quantity: "2".parse().unwrap(),
            approved_unit_price: "10.00".parse().unwrap(),
            attributes: vec![],
        }];
        let address = AddressProof {
            connection_id: "connection-1".into(),
            configuration_hash: proof::binding_hash(
                &super::super::test_support::settings(),
                &super::super::test_support::target(),
            )
            .unwrap(),
            action_id: "addr-action-1".into(),
            payload_hash: "hash".into(),
            user_id: "user-1".into(),
            addr_id: "addr-1".into(),
            city_id: "2".into(),
            recipient_hash: proof::recipient_hash("test-key", &recipient).unwrap(),
        };
        let context = DeliveryContext { reference: proof::encode("test-key", &address).unwrap() };
        let method = DeliveryMethod::Courier;
        let fee = "3.00".parse::<Amount>().unwrap();
        let choice = ChoiceProof {
            connection_id: "connection-1".into(),
            configuration_hash: proof::binding_hash(
                &super::super::test_support::settings(),
                &super::super::test_support::target(),
            )
            .unwrap(),
            context_hash: proof::hash("test-key", context.reference.as_bytes()),
            lines_hash: proof::lines_hash("test-key", &lines).unwrap(),
            rule_id: "rule-1".into(),
            method: proof::method_key(&method),
            shipping_fee: fee.to_string(),
            ship_time_text: "快递配送".into(),
            shop_name: None,
            shop_detail: None,
            expires_at: Instant::now().unix_secs() + 60,
        };
        CreateOrder {
            action: ActionKey { id: "create-action-1".into(), payload_hash: "hash".into() },
            merchant_order_no: "merchant-1".into(),
            lines,
            recipient,
            delivery_context: context,
            delivery: DeliveryChoice {
                method,
                shipping_fee: fee,
                reference: proof::encode("test-key", &choice).unwrap(),
            },
            approved_total: "23.00".parse().unwrap(),
        }
    }
    fn product(price: &str) -> Value {
        json!({"product":{"id":"product-1","title":"测试蛋糕"}, "specs":[{"id":"spec-1","name":"6寸","clearing_price":price}]})
    }

    #[tokio::test]
    async fn create_preserves_external_identity_and_actual_cost_without_payment() {
        // 来源创建后返还超批准金额也必须保留订单身份，不能返回伪造的未创建。
        let (connector, calls) = recording_connector(vec![
            Ok(product("10.00")),
            Ok(json!({"order":{"order_no":"external-1","final_amount":"24.00","pay_status":"0"}})),
        ]);
        let created = connector.create(&request()).await.unwrap();
        assert_eq!(created.order.value.reference.external_order_no, "external-1");
        assert_eq!(created.order.value.amounts.unwrap().payable.to_string(), "24.00");
        assert_eq!(created.order.value.fulfillment, None);
        assert_eq!(created.next, CreationNext::ConfirmPayment);
        let requests = calls.requests.lock().unwrap();
        assert_eq!(requests.len(), 2);
        let Request::Multipart { path, parameters, write, not_after } = &requests[1] else {
            panic!("expected create")
        };
        assert_eq!(*path, "/dsapi/order/submit_order");
        assert!(*write);
        assert!(not_after.is_some());
        let payload: Value = serde_json::from_str(&parameters[0].1).unwrap();
        assert_eq!(payload["out_order_no"], "merchant-1");
        assert_eq!(payload["quantitys"], "2");
        assert_eq!(payload["group"]["rule-1"]["ship_type"], "same");
    }

    #[tokio::test]
    async fn changed_price_stops_before_write_and_unknown_write_is_never_retried() {
        let (connector, calls) = recording_connector(vec![Ok(product("10.01"))]);
        assert_eq!(
            connector.create(&request()).await.err().unwrap().class,
            SupplierFailureClass::BusinessRejected
        );
        assert_eq!(calls.requests.lock().unwrap().len(), 1);
        let failure = error(SupplierFailureClass::ResultUnknown, "TEST_UNKNOWN", "unknown");
        let (connector, calls) = recording_connector(vec![Ok(product("10.00")), Err(failure)]);
        assert_eq!(
            connector.create(&request()).await.err().unwrap().class,
            SupplierFailureClass::ResultUnknown
        );
        assert_eq!(calls.requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn invalid_delivery_recipient_and_approved_total_make_no_calls() {
        let (connector, calls) = recording_connector(vec![]);
        let mut changed = request();
        changed.recipient.phone = "13900000000".into();
        assert!(connector.create(&changed).await.is_err());
        let mut changed = request();
        changed.recipient.geocoded_address.as_mut().unwrap().coordinate_system = CoordinateSystem::Gcj02;
        assert!(connector.create(&changed).await.is_err());
        let mut changed = request();
        changed.approved_total = "22.99".parse().unwrap();
        assert_eq!(
            connector.create(&changed).await.err().unwrap().class,
            SupplierFailureClass::BusinessRejected
        );
        let mut changed = request();
        changed.delivery.reference.push('x');
        assert!(connector.create(&changed).await.is_err());
        assert!(calls.requests.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn recovery_queries_original_merchant_number_and_rejects_other_order() {
        let order = json!({"channel_no":"test-channel","out_order_no":"merchant-1","order_no":"external-1","status":"1","pay_status":"1"});
        let (connector, calls) = recording_connector(vec![Ok(order.clone()), Ok(order), Ok(Value::Null)]);
        let found = connector.order(&OrderKey::Merchant("merchant-1".into())).await.unwrap();
        assert!(
            matches!(found, Lookup::Found(value) if value.value.fulfillment == Some(FulfillmentStatus::Accepted))
        );
        {
            let requests = calls.requests.lock().unwrap();
            let Request::Read { parameters, .. } = &requests[0] else { panic!("expected read") };
            assert_eq!(parameters, &vec![("out_order_no".into(), "merchant-1".into())]);
        }
        assert!(connector.order(&OrderKey::Merchant("other-order".into())).await.is_err());
        assert!(matches!(
            connector.order(&OrderKey::Merchant("not-visible".into())).await.unwrap(),
            Lookup::NotVisible
        ));
    }

    #[test]
    fn confirmed_create_observes_order_and_shipping_tracks_do_not_guess() {
        let created = created_order(
            &json!({"order":{"order_no":"external-1","final_amount":"23.00","pay_status":"1"}}),
            "merchant-1",
        )
        .unwrap();
        assert_eq!(created.next, CreationNext::ObserveOrder);
        let courier = shipments(&json!({"ship_type":"same","express_no":"tracking-1","express_cp":"快递"}))
            .unwrap()
            .unwrap();
        assert_eq!(courier[0].tracking_no.as_deref(), Some("tracking-1"));
        assert_eq!(courier[0].courier_phone, None);
        let delivery =
            shipments(&json!({"ship_type":"delivery","express_no":"13800000000","express_cp":"配送员"}))
                .unwrap()
                .unwrap();
        assert_eq!(delivery[0].courier_phone.as_deref(), Some("13800000000"));
    }
    #[test]
    fn delivery_expiration_is_rechecked_after_preflight() {
        let connector = connector(vec![]);
        let request = request();
        let choice: ChoiceProof = proof::decode("test-key", &request.delivery.reference).unwrap();
        assert!(connector.order_payload_at(&request, Instant::from_unix_secs(choice.expires_at - 1)).is_ok());
        assert!(connector.order_payload_at(&request, Instant::from_unix_secs(choice.expires_at)).is_err());
    }
    #[test]
    fn canceled_order_does_not_fabricate_refund_or_rejection() {
        let value = json!({"channel_no":"channel","out_order_no":"merchant-1","order_no":"external-1","status":"3","pay_status":"1","refund_amount":"99.00"});
        let order = order_snapshot(&value, "channel").unwrap();
        assert_eq!(order.fulfillment, None);
        assert_eq!(order.cancellation, Some(CancelStatus::Canceled));
        assert_eq!(order.refund, None);
        assert!(order_snapshot(&value, "other").is_err());
    }
    #[tokio::test]
    async fn payment_is_a_separate_single_call_with_original_order_identity() {
        let (connector, calls) = recording_connector(vec![Ok(json!("支付成功"))]);
        let request = ConfirmPayment {
            action: ActionKey { id: "payment-1".into(), payload_hash: "hash".into() },
            order: OrderReference {
                merchant_order_no: "merchant-1".into(),
                external_order_no: "external-1".into(),
            },
            transaction_no: "transaction-1".into(),
            amount: "12.30".parse().unwrap(),
        };
        assert_eq!(connector.confirm(&request).await.unwrap().value, PaymentState::Confirmed);
        let requests = calls.requests.lock().unwrap();
        assert_eq!(requests.len(), 1);
        let Request::Json { path, payload } = &requests[0] else { panic!("expected payment") };
        assert_eq!(*path, "/dsapi/order/order_pay_result");
        assert_eq!(payload["order_sn"], "external-1");
        assert_eq!(payload["out_order_sn"], "merchant-1");
        assert_eq!(payload["transaction_sn"], "transaction-1");
        assert_eq!(payload["order_price"], request.amount.to_string());
        assert_eq!(Orders::replay_protection(&connector), ReplayProtection::Unverified);
        assert_eq!(PaymentConfirmation::replay_protection(&connector), ReplayProtection::Unverified);
    }
}
