//! 预占创建、叶子确认及取消分别一次发送；未知结果由原单号恢复。
use async_trait::async_trait;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_fulfillment::CancelStatus;
use erp_supply::ports::connector::common::{
    ConnectorError, ConnectorResult, Lookup, ReplayProtection, Snapshot,
};
use erp_supply::ports::connector::order::{
    ActionAccepted, CancelOrder, Cancellations, ConfirmOrder, ConfirmationState, CreateOrder, CreatedOrder,
    CreationNext, EncryptedOrderEvidence, OrderConfirmation, OrderKey, OrderLine, OrderReference,
    OrderSnapshot, Orders,
};
use erp_supply::ports::connector::preflight::OrderPreflight;

use super::delivery::observed;
use super::order_mapping::{confirmation, summary};
use super::order_payload::{identifier, submit, valid_order_no};
use super::transport::{JdVopReadRequest, JdVopWriteRequest};
use super::wire::WireText;
use super::wire_order::{CancelProgress, CancellationEligibility, JdOrder};
use super::{JdVopConnector, error, mapping, protocol_required};

#[async_trait]
impl Orders for JdVopConnector {
    fn supports_creation(&self) -> bool {
        self.writer.is_some()
            && self.profile.as_ref().is_some_and(|profile| profile.ensure_confirmation_funds().is_ok())
    }

    fn replay_protection(&self) -> ReplayProtection {
        ReplayProtection::Unverified
    }

    fn recovery_next(&self, order: &OrderSnapshot) -> ConnectorResult<CreationNext> {
        match order.confirmation {
            Some(ConfirmationState::Confirmed) => Ok(CreationNext::ObserveOrder),
            Some(ConfirmationState::Pending) => Ok(CreationNext::ConfirmOrder),
            _ => Err(mapping("京东原订单确认事实尚未核实，请继续查询原结果")),
        }
    }

    async fn create(&self, request: &CreateOrder) -> ConnectorResult<CreatedOrder> {
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        profile.ensure_confirmation_funds()?;
        let writer = self.writer.as_ref().ok_or_else(protocol_required)?;
        self.inspect(request).await?;
        self.validate_choice(request)?;
        let payload = submit(request, profile)?;
        let order: JdOrder = writer
            .send(JdVopWriteRequest::Submit { order: Box::new(payload), source_type: profile.source_type })
            .await?;
        if order.third_order_id != request.merchant_order_no {
            return Err(result_unknown());
        }
        let mut snapshot = summary(&order).map_err(|_| result_unknown())?;
        let codec = self.evidence_codec.as_ref().ok_or_else(result_unknown)?;
        let payload = serde_json::to_string(&order).map_err(|_| result_unknown())?;
        snapshot.source_evidence = Some(EncryptedOrderEvidence {
            encrypted_payload: codec.encrypt(&payload).map_err(|_| result_unknown())?,
            content_fingerprint: self.hash(&order).map_err(|_| result_unknown())?,
        });
        Ok(CreatedOrder { order: observed(snapshot), next: CreationNext::ConfirmOrder })
    }

    async fn order(&self, key: &OrderKey) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        self.query_order(key, &[]).await
    }

    async fn order_with_lines(
        &self,
        key: &OrderKey,
        frozen: &[OrderLine],
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        self.query_order(key, frozen).await
    }

    async fn cancellation_order(
        &self,
        key: &OrderKey,
        frozen: &[OrderLine],
    ) -> ConnectorResult<Lookup<Snapshot<OrderSnapshot>>> {
        self.query_cancellation_order(key, frozen).await
    }
}

#[async_trait]
impl OrderConfirmation for JdVopConnector {
    fn replay_protection(&self) -> ReplayProtection {
        ReplayProtection::Unverified
    }

    async fn confirm_order(&self, request: &ConfirmOrder) -> ConnectorResult<Snapshot<ConfirmationState>> {
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        profile.ensure_confirmation_funds()?;
        if request.topology_version == 0
            || request.action.id.is_empty()
            || request.action.payload_hash.is_empty()
        {
            return Err(mapping("京东原叶子确认动作资料缺失"));
        }
        let order = self.current_target(&request.order).await?;
        if order
            .order_state
            .as_ref()
            .and_then(|state| state.cancel_order_state.as_ref())
            .is_some_and(|state| state.0 == "1")
        {
            return Err(mapping("京东原子单已取消，不能继续确认"));
        }
        if confirmation(&order)? == Some(ConfirmationState::Confirmed) {
            return Ok(observed(ConfirmationState::Confirmed));
        }
        if confirmation(&order)? != Some(ConfirmationState::Pending) {
            return Err(mapping("京东原子单确认状态未知，请继续查询原结果"));
        }
        let writer = self.writer.as_ref().ok_or_else(protocol_required)?;
        let result: WireText = writer
            .send(JdVopWriteRequest::Confirm {
                merchant_order_no: request.order.merchant_order_no.clone(),
                external_order_no: request.order.external_order_no.clone(),
                source_type: profile.source_type,
            })
            .await?;
        if result.0 != "true" {
            return Err(result_unknown());
        }
        Ok(observed(ConfirmationState::Confirmed))
    }
}

#[async_trait]
impl Cancellations for JdVopConnector {
    async fn validate(&self, request: &CancelOrder) -> ConnectorResult<()> {
        if request.lines.is_some() {
            return Err(error(
                SupplierFailureClass::CapabilityGap,
                "JD_VOP_PARTIAL_CANCEL_UNSUPPORTED",
                "京东取消须按已核实的当前子单完整申请",
            ));
        }
        if request.action.id.is_empty()
            || request.action.payload_hash.is_empty()
            || request.reason_code.trim().is_empty()
            || request.reason_code.chars().count() > 200
        {
            return Err(mapping("京东取消原动作身份或原因无效"));
        }
        let order = self.current_target(&request.order).await?;
        if order
            .order_state
            .as_ref()
            .and_then(|state| state.cancel_order_state.as_ref())
            .is_none_or(|state| state.0 != "0")
        {
            return Err(mapping("京东原单已取消或取消状态尚未核实，请查询原结果"));
        }
        match confirmation(&order)? {
            Some(ConfirmationState::Pending) => Ok(()),
            Some(ConfirmationState::Confirmed) => self.verify_paid_cancellation(&request.order).await,
            _ => Err(mapping("京东原单确认状态尚未核实，不能申请取消")),
        }
    }

    async fn cancel(&self, request: &CancelOrder) -> ConnectorResult<ActionAccepted> {
        self.validate(request).await?;
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        let writer = self.writer.as_ref().ok_or_else(protocol_required)?;
        let result: WireText = writer
            .send(JdVopWriteRequest::Cancel {
                merchant_order_no: request.order.merchant_order_no.clone(),
                external_order_no: request.order.external_order_no.clone(),
                reason: request.reason_code.clone(),
                source_type: profile.source_type,
            })
            .await?;
        if result.0 != "true" {
            return Err(result_unknown());
        }
        Ok(ActionAccepted { action_id: request.action.id.clone(), external_request_id: None })
    }

    async fn progress(&self, order: &OrderReference) -> ConnectorResult<Snapshot<CancelStatus>> {
        identifier(&order.external_order_no)?;
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let progress: CancelProgress = reader
            .read(JdVopReadRequest::CancelProgress { external_order_no: order.external_order_no.clone() })
            .await?;
        if progress.jd_order_id.0 != order.external_order_no {
            return Err(mapping("京东取消进度不属于原子单"));
        }
        let status = match progress.cancel_process_code.0.as_str() {
            "1" | "2" => CancelStatus::CancelPending,
            "3" => CancelStatus::Canceled,
            "4" => CancelStatus::Failed,
            _ => return Err(mapping("京东取消进度码未核实")),
        };
        Ok(observed(status))
    }
}

impl JdVopConnector {
    async fn current_target(&self, reference: &OrderReference) -> ConnectorResult<JdOrder> {
        valid_order_no(&reference.merchant_order_no)?;
        identifier(&reference.external_order_no)?;
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let orders: Vec<JdOrder> = reader
            .read(JdVopReadRequest::Order {
                merchant_order_no: None,
                external_order_no: Some(reference.external_order_no.clone()),
            })
            .await?;
        let mut matches =
            orders.into_iter().filter(|order| order.jd_order_id.0 == reference.external_order_no);
        let order = matches.next().ok_or_else(|| mapping("京东原子单暂不可见，请继续查询原结果"))?;
        if matches.next().is_some() {
            return Err(mapping("京东原子单查询返回重复身份，请先核实原结果"));
        }
        if order.third_order_id != reference.merchant_order_no
            || !(order.is_reservation_root()
                || (order.parent_type.0 == "0" && order.child_order_ids().is_empty()))
            || order.requires_special_fulfillment()
        {
            return Err(mapping("京东子单身份、拆单关系或附加服务已变化，须重新核实"));
        }
        Ok(order)
    }

    async fn verify_paid_cancellation(&self, reference: &OrderReference) -> ConnectorResult<()> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let result: Vec<CancellationEligibility> = reader
            .read(JdVopReadRequest::CancellationEligibility {
                external_order_nos: vec![reference.external_order_no.clone()],
            })
            .await?;
        let target = result
            .first()
            .filter(|target| result.len() == 1 && target.jd_order_id.0 == reference.external_order_no)
            .ok_or_else(|| mapping("京东可取消校验未唯一覆盖原订单"))?;
        match target.cancel_flag.0.as_str() {
            "true" => Ok(()),
            "false" => {
                let reason = target.reason_code.as_deref().filter(|reason| {
                    !reason.is_empty()
                        && reason.len() <= 32
                        && reason.bytes().all(|byte| byte.is_ascii_alphanumeric())
                });
                Err(error(
                    SupplierFailureClass::BusinessRejected,
                    &reason.map_or_else(
                        || "JD_VOP_ORDER_NOT_CANCELABLE".into(),
                        |reason| format!("JD_VOP_CANCEL_{reason}"),
                    ),
                    "京东当前不允许取消该订单，请核对原单状态及取消权限",
                ))
            },
            _ => Err(mapping("京东原订单可取消状态尚未核实")),
        }
    }
}

fn result_unknown() -> ConnectorError {
    error(
        SupplierFailureClass::ResultUnknown,
        "JD_VOP_WRITE_RESULT_UNKNOWN",
        "京东原请求结果待核实，请查询原结果",
    )
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, VecDeque};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};

    use config::JdVopConfig;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_party::SensitiveDataCodec;
    use erp_supply::entity::supplier_api::{
        SupplierApiCapabilityCode, SupplierApiConnection, SupplierApiConnectionData,
        SupplierApiConnectionStatus, SupplierConnectorType,
    };
    use erp_supply::ports::connector::SupplierConnector;
    use erp_supply::ports::connector::address::{ConfirmedSupplierAddress, SupplierAddressLevel};
    use erp_supply::ports::connector::common::{ActionKey, SupplierSku};
    use erp_supply::ports::connector::offer::{DeliveryContext, DeliveryQuery, DeliverySource};
    use erp_supply::ports::connector::order::{DeliveryChoice, DeliveryMethod, Recipient};
    use erp_supply::ports::connector::order_tree::OrderNodeRole;
    use erp_supply::ports::supplier_reference_registry::{
        SupplierReferenceRegistry, SupplierReferenceTarget,
    };
    use serde_json::{Value, json, to_value};

    use super::super::JdVopRuntime;
    use super::super::order_payload::{
        JdVopConfirmationFundsPolicy, JdVopExecutionProfile, JdVopInvoiceProfile,
    };
    use super::super::transport::{JdVopReadTransport, JdVopWriteTransport};
    use super::*;

    #[derive(Default)]
    struct Transport {
        reads: Mutex<Vec<JdVopReadRequest>>,
        writes: Mutex<Vec<JdVopWriteRequest>>,
        fail_next: AtomicBool,
        deny_cancel: AtomicBool,
        confirmed: Mutex<BTreeSet<String>>,
        custom: Mutex<VecDeque<Value>>,
        write_responses: Mutex<VecDeque<ConnectorResult<Vec<u8>>>>,
    }

    fn order(id: &str, parent: Option<&str>, quantity: &str, freight: &str, confirmed: bool) -> Value {
        let total = match id {
            "100" => "32",
            "201" => "11",
            _ => "21",
        };
        json!({"orderType":"5","jdOrderId":id,"thirdOrderId":"stable-third-1","parentType":if parent.is_none(){"1"}else{"0"},"parentJdOrderId":parent,"childJdOrderIdList":if parent.is_none(){vec!["201","202"]}else{vec![]},"skuInfoList":[{"skuId":"123","skuNum":quantity,"skuPrice":"10","type":"0"},{"skuId":"999","skuNum":"1","skuPrice":"0","type":"1"}],"orderPrice":{"orderTotalPrice":total,"orderTotalFreight":freight},"orderState":{"confirmState":if confirmed{"1"}else{"0"},"cancelOrderState":"0","jdOrderState":"2"}})
    }

    fn reservation() -> Value {
        let mut root = order("100", None, "3", "2", false);
        root["childJdOrderIdList"] = json!([]);
        root
    }

    fn envelope(method: &str, rpc: &str, result: Value) -> Vec<u8> {
        let key = format!("{}_responce", method.replace('.', "_"));
        serde_json::to_vec(&json!({key:{rpc:{"success":true,"resultCode":"0000","result":result}}})).unwrap()
    }

    #[async_trait]
    impl JdVopReadTransport for Transport {
        async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            self.reads.lock().unwrap().push(request.clone());
            let result = match request {
                JdVopReadRequest::Areas { level, .. } => {
                    json!({"areaInfoList":[{"areaId":level.to_string(),"areaName":level.to_string()}]})
                },
                JdVopReadRequest::VerifyAddress { .. } => json!(true),
                JdVopReadRequest::Freight { payment_type, .. } => {
                    assert_eq!(*payment_type, 4);
                    json!({"totalFreight":"2"})
                },
                JdVopReadRequest::SaleEligibility { .. } => json!([{"skuId":"123","canPurchase":true}]),
                JdVopReadRequest::Stock { .. } => {
                    json!([{"skuId":"123","stockStateType":"33","remainNum":"1000"}])
                },
                JdVopReadRequest::Detail { .. } => {
                    json!({"skuId":"123","skuName":"Physical","lowestBuy":"1","saleUnit":"pack"})
                },
                JdVopReadRequest::Price { .. } => {
                    json!([{"skuId":"123","salePrice":"10","taxRatePercentage":"9"}])
                },
                JdVopReadRequest::Balance => json!({"balanceInfo":{"remainLimit":"1000"}}),
                JdVopReadRequest::CancellationEligibility { external_order_nos } => {
                    let allowed = !self.deny_cancel.load(Ordering::SeqCst);
                    json!(
                        external_order_nos
                            .iter()
                            .map(|id| json!({"jdOrderId":id,
                        "cancelFlag":allowed,"reasonCode":if allowed{None}else{Some("3204")}}))
                            .collect::<Vec<_>>()
                    )
                },
                JdVopReadRequest::Order { external_order_no, .. } => {
                    if let Some(custom) = self.custom.lock().unwrap().pop_front() {
                        custom
                    } else if let Some(id) = external_order_no.as_ref().filter(|id| id.as_str() != "100") {
                        json!([order(
                            id,
                            Some("100"),
                            if id == "201" { "1" } else { "2" },
                            "1",
                            self.confirmed.lock().unwrap().contains(id)
                        )])
                    } else if !self.confirmed.lock().unwrap().contains("100") {
                        json!([reservation()])
                    } else {
                        json!([
                            order("100", None, "3", "2", true),
                            order(
                                "201",
                                Some("100"),
                                "1",
                                "1",
                                self.confirmed.lock().unwrap().contains("201")
                            ),
                            order("202", Some("100"), "2", "1", false)
                        ])
                    }
                },
                JdVopReadRequest::Logistics { external_order_no } => {
                    json!({"ziyingShowResult":[],"thirdPsShowResult":[{"thirdId":format!("waybill-{external_order_no}"),"thirdName":"Carrier","thirdPsShowInfoList":[{"orderId":external_order_no,"msgTime":"2026-10-29 12:00:00","content":"Receiver phone 13800000000"}]}]})
                },
                JdVopReadRequest::DeliveryInfo { external_order_no, source_type } => {
                    assert_eq!(*source_type, 3);
                    json!({"thirdOrderId":"stable-third-1","jdOrderId":external_order_no,
                        "logisticInfoList":[{"jdOrderId":external_order_no,
                            "deliveryOrderId":format!("waybill-{external_order_no}"),
                            "deliveryCarrier":"Carrier"}],"trackInfoList":[]})
                },
                JdVopReadRequest::CancelProgress { external_order_no } => {
                    json!({"jdOrderId":external_order_no,"cancelProcessCode":"2"})
                },
                _ => return Err(mapping("unexpected test read")),
            };
            Ok(envelope(request.method(), "openRpcResult", result))
        }
    }

    #[async_trait]
    impl JdVopWriteTransport for Transport {
        async fn write(&self, request: &JdVopWriteRequest) -> ConnectorResult<Vec<u8>> {
            self.writes.lock().unwrap().push(request.clone());
            if let Some(response) = self.write_responses.lock().unwrap().pop_front() {
                return response;
            }
            if self.fail_next.swap(false, Ordering::SeqCst) {
                return Err(error(SupplierFailureClass::TransientFailure, "NETWORK_LOST", "network lost"));
            }
            let result = match request {
                JdVopWriteRequest::Submit { order: payload, source_type } => {
                    assert_eq!(*source_type, 3);
                    assert_eq!(payload.submit_state_type, 0);
                    assert_eq!(payload.payment_info.payment_type, 4);
                    assert_eq!(payload.third_order_id, "stable-third-1");
                    assert!(!payload.sku_info_list[0].sku_need_gift);
                    assert_eq!(
                        serde_json::to_value(payload).unwrap()["skuInfoList"][0]["skuUnitPrice"],
                        json!(10)
                    );
                    reservation()
                },
                JdVopWriteRequest::Confirm { external_order_no, .. } => {
                    self.confirmed.lock().unwrap().insert(external_order_no.clone());
                    json!(true)
                },
                JdVopWriteRequest::ConfirmReceipt { .. } => json!(true),
                JdVopWriteRequest::Cancel { .. } => json!(true),
                JdVopWriteRequest::DeleteMessages { .. } => {
                    return Err(mapping("unexpected delete in order test"));
                },
            };
            Ok(envelope(
                request.method(),
                if matches!(request, JdVopWriteRequest::Submit { .. }) {
                    "vopOrderRpcResult"
                } else {
                    "openRpcResult"
                },
                result,
            ))
        }
    }

    fn profile() -> JdVopExecutionProfile {
        JdVopExecutionProfile {
            source_type: 3,
            evidence_reference: "verified/g0/account-1".into(),
            verified_external_funds_account_ref: "external-funds-account-1".into(),
            confirmation_funds_policy: Some(JdVopConfirmationFundsPolicy::OriginalOrderReservation {
                evidence_reference: "verified/g0/original-order-funding".into(),
            }),
            preflight_validity_seconds: 300,
            ordinary_skus: BTreeSet::from(["123".into()]),
            verified_message_types: None,
            verified_funds_profile: None,
            invoice: JdVopInvoiceProfile {
                invoice_type: 23,
                invoice_put_type: 2,
                invoice_selected_title: 5,
                invoice_content_type: 1,
                invoice_company_name: "Buyer".into(),
                invoice_reg_code: Some("TAX1".into()),
                invoice_reg_company_name: None,
                invoice_reg_address: None,
                invoice_reg_phone: None,
                invoice_reg_bank: None,
                invoice_reg_bank_account: None,
                recipient: None,
            },
        }
    }

    #[test]
    fn confirmation_policy_preserves_read_identity_and_rejects_invalid_evidence() {
        let mut missing = profile();
        missing.confirmation_funds_policy = None;
        assert!(missing.validate().is_ok());
        assert_eq!(
            missing.ensure_confirmation_funds().unwrap_err().code,
            "JD_VOP_CONFIRMATION_FUNDS_POLICY_REQUIRED"
        );
        assert_eq!(missing.binding_content().unwrap(), profile().binding_content().unwrap());
        assert!(to_value(&missing).unwrap().get("confirmation_funds_policy").is_none());
        for evidence in ["", " ", " leading-space", "trailing-space ", "bad\nreference"] {
            missing.confirmation_funds_policy =
                Some(JdVopConfirmationFundsPolicy::OriginalOrderReservation {
                    evidence_reference: evidence.into(),
                });
            assert_eq!(missing.validate().unwrap_err().code, "JD_VOP_CONFIRMATION_FUNDS_POLICY_REQUIRED");
        }
        assert!(profile().validate().is_ok());
    }

    async fn prepared() -> (JdVopConnector, Arc<Transport>, CreateOrder, Arc<SensitiveDataCodec>) {
        let transport = Arc::new(Transport::default());
        let codec = Arc::new(SensitiveDataCodec::from_secret(b"application-test-key"));
        let settings:JdVopConfig=serde_json::from_value(json!({"enabled":true,"gateway_url":"https://gateway.example/router","app_key":"app","app_secret":"secret","credential_id":"test-jd-token","credential_encryption_key":"offline-test-only-independent-key-at-least-32","account_scope":"tech-namespace","requests_per_second":100,"source_type":3})).unwrap();
        let runtime = JdVopRuntime::with_verified_execution(
            settings,
            transport.clone(),
            transport.clone(),
            profile(),
            codec.clone(),
        )
        .unwrap();
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier"),
                connection_code: "jd".into(),
                connector_type: SupplierConnectorType::JdVop,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap();
        let references = runtime.deployment_config(&SupplierReferenceTarget::from(&connection)).unwrap();
        connection.endpoint_reference = references.endpoint;
        connection.credential_reference = Some(references.credential);
        connection.endpoint_reference_bound = true;
        connection.credential_reference_bound = true;
        connection.supplier_deposit_external_account_ref = Some("external-funds-account-1".into());
        let connector = runtime.tracking_connector(&connection).unwrap();
        let level = |id: &str| SupplierAddressLevel { code: id.into(), name: id.into() };
        let recipient = Recipient {
            name: "Buyer".into(),
            phone: "13800000000".into(),
            region: "erp-region".into(),
            address: "road 1".into(),
            geocoded_address: None,
            supplier_address: Some(ConfirmedSupplierAddress {
                connection_id: "connection".into(),
                snapshot_reference: "execution".into(),
                address_fingerprint: "a".repeat(64),
                version: 1,
                province: level("1"),
                city: level("2"),
                county: level("3"),
                town: level("4"),
            }),
        };
        let mut request = CreateOrder {
            action: ActionKey { id: "create-action".into(), payload_hash: "payload-hash".into() },
            merchant_order_no: "stable-third-1".into(),
            lines: vec![OrderLine {
                source_purchase_terms: None,
                line_id: "line1".into(),
                sku: SupplierSku { product_id: Some("sku:123".into()), spec_id: "123".into() },
                quantity: "3".parse().unwrap(),
                approved_unit_price: "10".parse().unwrap(),
                attributes: vec![],
            }],
            recipient,
            delivery_context: DeliveryContext { reference: String::new(), supplier_address: None },
            delivery: DeliveryChoice {
                method: DeliveryMethod::Courier,
                shipping_fee: "2".parse().unwrap(),
                reference: String::new(),
            },
            approved_total: "32".parse().unwrap(),
        };
        request.delivery_context = connector
            .prepare(
                &ActionKey { id: "address-action".into(), payload_hash: "address-hash".into() },
                &request.recipient,
            )
            .await
            .unwrap();
        request.delivery = connector
            .options(&DeliveryQuery {
                context: request.delivery_context.clone(),
                lines: request.lines.clone(),
            })
            .await
            .unwrap()
            .value
            .groups[0]
            .choices[0]
            .clone();
        (connector, transport, request, codec)
    }

    #[tokio::test]
    async fn missing_confirmation_funds_hides_capability_and_sends_no_request() {
        let (mut connector, transport, create, _) = prepared().await;
        let mut unsupported = profile();
        unsupported.confirmation_funds_policy = None;
        connector.profile = Some(Arc::new(unsupported));
        let reads = transport.reads.lock().unwrap().len();
        assert!(connector.order_confirmation().is_none());
        assert!(connector.orders().is_some());
        assert!(connector.cancellations().is_some());
        let capabilities = connector.capabilities();
        assert!(!capabilities.contains(&SupplierApiCapabilityCode::Order));
        assert!(!capabilities.contains(&SupplierApiCapabilityCode::OrderConfirmation));
        assert!(capabilities.contains(&SupplierApiCapabilityCode::Query));
        assert!(capabilities.contains(&SupplierApiCapabilityCode::Cancel));
        assert_eq!(
            connector.create(&create).await.err().unwrap().code,
            "JD_VOP_CONFIRMATION_FUNDS_POLICY_REQUIRED"
        );
        let request = ConfirmOrder {
            action: ActionKey { id: "original-confirm".into(), payload_hash: "original-payload".into() },
            order: OrderReference {
                merchant_order_no: "stable-third-1".into(),
                external_order_no: "201".into(),
            },
            topology_version: 1,
        };
        assert_eq!(
            OrderConfirmation::confirm_order(&connector, &request).await.unwrap_err().code,
            "JD_VOP_CONFIRMATION_FUNDS_POLICY_REQUIRED"
        );
        assert_eq!(transport.reads.lock().unwrap().len(), reads);
        assert!(transport.writes.lock().unwrap().is_empty());
        assert!(matches!(
            connector.order(&OrderKey::Merchant("stable-third-1".into())).await.unwrap(),
            Lookup::Found(_)
        ));
    }

    #[tokio::test]
    async fn create_uses_actual_quantities_and_preserves_stable_preoccupied_order() {
        let (connector, transport, request, _) = prepared().await;
        assert!(connector.capabilities().contains(&SupplierApiCapabilityCode::Order));
        assert!(connector.payment_confirmation().is_none());
        assert_eq!(connector.funds_account_reference(), Some("external-funds-account-1"));
        let created = connector.create(&request).await.unwrap();
        assert_eq!(created.order.value.reference.external_order_no, "100");
        assert_eq!(created.next, CreationNext::ConfirmOrder);
        assert!(created.order.value.payment.is_none());
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
        assert!(
            transport
                .reads
                .lock()
                .unwrap()
                .iter()
                .any(|read| matches!(read,JdVopReadRequest::Price{quantity,..}if quantity=="3"))
        );
    }

    #[tokio::test]
    async fn unknown_create_is_not_repeated_and_original_root_recovers_split_quantities() {
        let (connector, transport, request, codec) = prepared().await;
        transport.fail_next.store(true, Ordering::SeqCst);
        assert_eq!(
            connector.create(&request).await.err().unwrap().class,
            SupplierFailureClass::ResultUnknown
        );
        let Lookup::Found(snapshot) = connector
            .order_with_lines(&OrderKey::Merchant(request.merchant_order_no.clone()), &request.lines)
            .await
            .unwrap()
        else {
            panic!("original order visible")
        };
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
        let reserved_tree = snapshot.value.topology.as_ref().unwrap();
        assert!(reserved_tree.leaves().is_empty());
        assert_eq!(reserved_tree.action_targets().len(), 1);
        assert_eq!(reserved_tree.action_targets()[0].external_order_no, "100");
        assert_eq!(snapshot.value.cancellation, None);
        connector
            .confirm_order(&ConfirmOrder {
                action: ActionKey { id: "original-root-confirm".into(), payload_hash: "root-payload".into() },
                order: snapshot.value.reference.clone(),
                topology_version: reserved_tree.version,
            })
            .await
            .unwrap();
        let Lookup::Found(snapshot) = connector
            .order_with_lines(&OrderKey::Merchant(request.merchant_order_no.clone()), &request.lines)
            .await
            .unwrap()
        else {
            panic!("original root and current children visible")
        };
        assert_eq!(transport.writes.lock().unwrap().len(), 2);
        let tree = snapshot.value.topology.as_ref().unwrap();
        assert_eq!(tree.leaves().len(), 2);
        assert_eq!(tree.leaves()[0].lines[0].line_id, "line1");
        assert!(tree.leaves().iter().all(|leaf| leaf.lines.len() == 1));
        let evidence = snapshot.value.source_evidence.as_ref().unwrap();
        assert!(codec.decrypt(&evidence.encrypted_payload).unwrap().contains("999"));
        let shipments = snapshot.value.shipments.as_ref().unwrap();
        assert_eq!(shipments.len(), 2);
        assert!(shipments.iter().all(|shipment| shipment.quantity_lines.is_none()
            && !shipment.complete_line_coverage
            && shipment.line_ids == vec!["line1".to_string()]));
        assert_eq!(shipments[0].events[0].description, "物流信息已更新");
        assert!(!serde_json::to_string(&shipments[0].events).unwrap().contains("13800000000"));
    }

    #[tokio::test]
    async fn unverified_business_failure_after_send_preserves_original_create_identity() {
        for code in [
            "1001",
            "1002",
            "1003",
            "1004",
            "1005",
            "2001",
            "2002",
            "2003",
            "2010",
            "2011",
            "PARTIAL_OR_DUPLICATE",
            "0000",
        ] {
            let (connector, transport, request, _) = prepared().await;
            let body=serde_json::to_vec(&json!({"jingdong_vop_order_submitOrder_responce":{"vopOrderRpcResult":{"success":false,"resultCode":code,"resultMessage":"unverified provider result"}}})).unwrap();
            transport.write_responses.lock().unwrap().push_back(Ok(body));
            let failure =
                connector.create(&request).await.err().expect("unverified response is not a safe rejection");
            assert_eq!(failure.class, SupplierFailureClass::ResultUnknown, "{code}");
            assert_eq!(failure.code, format!("JD_VOP_{code}"));
            let Lookup::Found(original) = connector
                .order_with_lines(&OrderKey::Merchant(request.merchant_order_no.clone()), &request.lines)
                .await
                .unwrap()
            else {
                panic!("original order must be investigated")
            };
            assert_eq!(original.value.reference.merchant_order_no, request.merchant_order_no);
            let writes = transport.writes.lock().unwrap();
            assert_eq!(writes.len(), 1, "{code}");
            let JdVopWriteRequest::Submit { order, .. } = &writes[0] else { panic!("one original create") };
            assert_eq!(order.third_order_id, request.merchant_order_no);
        }
    }

    #[tokio::test]
    async fn sender_error_classes_do_not_prove_the_order_was_not_created() {
        for class in [
            SupplierFailureClass::BusinessRejected,
            SupplierFailureClass::AuthSignature,
            SupplierFailureClass::RateLimited,
        ] {
            let (connector, transport, request, _) = prepared().await;
            transport.write_responses.lock().unwrap().push_back(Err(error(
                class,
                "JD_VOP_1004",
                "unverified sender classification",
            )));
            assert_eq!(
                connector.create(&request).await.err().unwrap().class,
                SupplierFailureClass::ResultUnknown
            );
            assert_eq!(transport.writes.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn leaf_confirmation_is_independent_and_cancel_acceptance_is_not_completion() {
        let (connector, transport, _, _) = prepared().await;
        let reads_before_confirmation = transport.reads.lock().unwrap().len();
        let reference =
            OrderReference { merchant_order_no: "stable-third-1".into(), external_order_no: "201".into() };
        let action = ActionKey { id: "confirm-action".into(), payload_hash: "confirm-hash".into() };
        let request = ConfirmOrder { action: action.clone(), order: reference.clone(), topology_version: 1 };
        assert_eq!(connector.confirm_order(&request).await.unwrap().value, ConfirmationState::Confirmed);
        assert_eq!(connector.confirm_order(&request).await.unwrap().value, ConfirmationState::Confirmed);
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
        {
            let reads = transport.reads.lock().unwrap();
            let confirmation_reads = &reads[reads_before_confirmation..];
            assert!(!confirmation_reads.is_empty());
            assert!(
                confirmation_reads.iter().all(|request| matches!(request, JdVopReadRequest::Order { .. }))
            );
        }
        connector
            .cancel(&CancelOrder {
                action,
                order: reference.clone(),
                lines: None,
                reason_code: "customer-request".into(),
            })
            .await
            .unwrap();
        assert_eq!(connector.progress(&reference).await.unwrap().value, CancelStatus::CancelPending);
        assert_eq!(transport.writes.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn changed_address_or_over_budget_stops_before_write() {
        let (connector, transport, mut request, _) = prepared().await;
        request.recipient.address = "changed address".into();
        assert!(connector.create(&request).await.is_err());
        assert!(transport.writes.lock().unwrap().is_empty());
        let (connector, transport, mut request, _) = prepared().await;
        request.approved_total = "31".parse().unwrap();
        assert!(connector.create(&request).await.is_err());
        assert!(transport.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn preoccupied_parent_cancels_its_original_identity_without_paid_eligibility_query() {
        let (connector, transport, request, _) = prepared().await;
        let Lookup::Found(observation) = connector
            .cancellation_order(&OrderKey::Merchant(request.merchant_order_no.clone()), &request.lines)
            .await
            .unwrap()
        else {
            panic!("original reservation visible")
        };
        let targets = observation.value.topology.as_ref().unwrap().action_targets();
        assert_eq!(targets.len(), 1);
        assert_eq!(targets[0].role, OrderNodeRole::ReservationRoot);
        connector
            .cancel(&CancelOrder {
                action: ActionKey { id: "cancel-original-root".into(), payload_hash: "frozen-root".into() },
                order: observation.value.reference,
                lines: None,
                reason_code: "006".into(),
            })
            .await
            .unwrap();
        assert!(
            transport
                .reads
                .lock()
                .unwrap()
                .iter()
                .all(|read| !matches!(read, JdVopReadRequest::CancellationEligibility { .. }))
        );
        let writes = transport.writes.lock().unwrap();
        assert_eq!(writes.len(), 1);
        assert!(
            matches!(&writes[0], JdVopWriteRequest::Cancel { external_order_no, .. } if external_order_no == "100")
        );
    }

    #[tokio::test]
    async fn paid_order_eligibility_denial_stops_before_any_cancel_write() {
        let (connector, transport, _, _) = prepared().await;
        transport.confirmed.lock().unwrap().insert("201".into());
        transport.deny_cancel.store(true, Ordering::SeqCst);
        let result = connector
            .cancel(&CancelOrder {
                action: ActionKey { id: "cancel-paid-child".into(), payload_hash: "paid-child".into() },
                order: OrderReference {
                    merchant_order_no: "stable-third-1".into(),
                    external_order_no: "201".into(),
                },
                lines: None,
                reason_code: "001".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(result.class, SupplierFailureClass::BusinessRejected);
        assert_eq!(result.code, "JD_VOP_CANCEL_3204");
        assert!(transport.reads.lock().unwrap().iter().any(|read| matches!(read, JdVopReadRequest::CancellationEligibility{external_order_nos} if external_order_nos == &["201"])));
        assert!(transport.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn changed_order_price_blocks_confirmation_but_keeps_original_cancel_investigation() {
        let (connector, transport, request, _) = prepared().await;
        let mut changed = order("100", None, "3", "2", true);
        changed["parentType"] = json!("0");
        changed["childJdOrderIdList"] = json!([]);
        changed["skuInfoList"][0]["skuPrice"] = json!("11");
        changed["orderPrice"]["orderTotalPrice"] = json!("35");
        transport.custom.lock().unwrap().push_back(json!([changed.clone()]));
        assert!(
            connector
                .order_with_lines(&OrderKey::Merchant(request.merchant_order_no.clone()), &request.lines)
                .await
                .is_err()
        );
        transport.custom.lock().unwrap().push_back(json!([changed.clone()]));
        let Lookup::Found(original) = connector
            .cancellation_order(&OrderKey::Merchant(request.merchant_order_no.clone()), &request.lines)
            .await
            .unwrap()
        else {
            panic!("abnormal original order preserved")
        };
        assert_eq!(original.value.amounts.as_ref().unwrap().payable, "35".parse().unwrap());
        transport.custom.lock().unwrap().push_back(json!([changed]));
        connector
            .cancel(&CancelOrder {
                action: ActionKey {
                    id: "cancel-abnormal-original".into(),
                    payload_hash: "abnormal-original".into(),
                },
                order: original.value.reference,
                lines: None,
                reason_code: "005".into(),
            })
            .await
            .unwrap();
        assert!(transport.reads.lock().unwrap().iter().any(|read| matches!(read, JdVopReadRequest::CancellationEligibility{external_order_nos} if external_order_nos == &["100"])));
        assert!(
            matches!(&transport.writes.lock().unwrap()[0], JdVopWriteRequest::Cancel { external_order_no, .. } if external_order_no == "100")
        );
    }

    #[tokio::test]
    async fn completed_leaf_uses_validated_child_quantities_without_waybill_quantities() {
        use erp_supply::entity::supplier_fulfillment::FulfillmentStatus;
        use erp_supply::order_tracking::{OrderObservation, OrderTracking, TrackingSourceLine};

        let (connector, transport, request, _) = prepared().await;
        let mut completed = order("100", None, "3", "2", true);
        completed["parentType"] = json!("0");
        completed["childJdOrderIdList"] = json!([]);
        completed["orderState"]["jdOrderState"] = json!("19");
        let tracking = OrderTracking::new(
            "execution".into(),
            "connection".into(),
            "purchase".into(),
            vec![TrackingSourceLine {
                fulfillment_item_id: request.lines[0].line_id.clone(),
                purchase_order_revision_line_id: "purchase-line".into(),
                sales_order_line_id: Some("sales-line".into()),
                delivery_id: Some("delivery".into()),
                quantity: request.lines[0].quantity,
            }],
        )
        .unwrap();
        for frozen in [request.lines.as_slice(), &[]] {
            transport.custom.lock().unwrap().push_back(json!([completed]));
            let Lookup::Found(snapshot) = connector
                .order_with_lines(&OrderKey::Merchant(request.merchant_order_no.clone()), frozen)
                .await
                .unwrap()
            else {
                panic!("original completed leaf must remain visible")
            };
            assert_eq!(snapshot.value.fulfillment, Some(FulfillmentStatus::Completed));
            assert!(!snapshot.value.complete_line_coverage);
            let observation =
                OrderObservation::from_snapshot("observation".into(), &tracking, snapshot).unwrap();
            assert!(!observation.proves_complete_coverage());
            let pending = tracking.pending_shipments(&observation).unwrap();
            if frozen.is_empty() {
                assert!(pending.is_empty());
            } else {
                assert_eq!(pending.len(), 1);
                assert_eq!(pending[0].source_order_no, "100");
                assert_eq!(pending[0].lines[0].line_id, request.lines[0].line_id);
                assert_eq!(pending[0].lines[0].quantity, request.lines[0].quantity);
                assert!(pending[0].tracking_no.is_none());
                let mut applied = tracking.clone();
                applied.record_shipment(&pending[0], vec!["real-delivery".into()]).unwrap();
                assert!(applied.pending_shipments(&observation).unwrap().is_empty());
                applied.ensure_dispatch_quantities(&observation).unwrap();
            }
            assert!(tracking.ensure_dispatch_quantities(&observation).is_err());
            assert!(tracking.applied_shipments.is_empty());
        }
        assert!(transport.writes.lock().unwrap().is_empty());
    }
}
