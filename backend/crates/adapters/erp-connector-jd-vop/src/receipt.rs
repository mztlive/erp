//! 京东厂直订单真实收到后的通知；查询未收到不能成为重发依据。

use std::collections::{BTreeMap, BTreeSet};

use async_trait::async_trait;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ConnectorResult, Lookup, Snapshot};
use erp_supply::ports::connector::order::OrderReference;
use erp_supply::ports::connector::receipt::{
    ConfirmReceipt, OrderReceiptConfirmation, ReceiptConfirmationState, ReceiptLine,
};
use rust_decimal::Decimal;

use super::delivery::observed;
use super::order_payload::{identifier, valid_order_no};
use super::transport::{JdVopReadRequest, JdVopWriteRequest};
use super::wire::WireText;
use super::wire_order::JdOrder;
use super::{JdVopConnector, error, mapping, protocol_required};

#[async_trait]
impl OrderReceiptConfirmation for JdVopConnector {
    async fn confirm_receipt(
        &self,
        request: &ConfirmReceipt,
    ) -> ConnectorResult<Snapshot<ReceiptConfirmationState>> {
        validate_request(request)?;
        let Lookup::Found(order) = self.receipt_node(&request.order).await? else {
            return Err(mapping("京东原收货确认子单暂不可见，请继续查询原结果"));
        };
        let state = receipt_state(&order)?;
        if state == ReceiptConfirmationState::NotRequired {
            return Ok(observed(state));
        }
        validate_received_lines(&order, &request.lines)?;
        if state == ReceiptConfirmationState::Confirmed {
            return Ok(observed(state));
        }
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        let writer = self.writer.as_ref().ok_or_else(protocol_required)?;
        let result: WireText = writer
            .send(JdVopWriteRequest::ConfirmReceipt {
                merchant_order_no: request.order.merchant_order_no.clone(),
                external_order_no: request.order.external_order_no.clone(),
                source_type: profile.source_type,
            })
            .await?;
        if result.0 != "true" {
            return Err(error(
                SupplierFailureClass::ResultUnknown,
                "JD_VOP_RECEIPT_RESULT_UNKNOWN",
                "京东原收货确认结果待核实，请继续查询原结果",
            ));
        }
        Ok(observed(ReceiptConfirmationState::Confirmed))
    }

    async fn receipt_confirmation(
        &self,
        reference: &OrderReference,
    ) -> ConnectorResult<Lookup<Snapshot<ReceiptConfirmationState>>> {
        match self.receipt_node(reference).await? {
            Lookup::Found(order) => Ok(Lookup::Found(observed(receipt_state(&order)?))),
            Lookup::NotVisible => Ok(Lookup::NotVisible),
        }
    }
}

impl JdVopConnector {
    async fn receipt_node(&self, reference: &OrderReference) -> ConnectorResult<Lookup<JdOrder>> {
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
        let Some(order) = matches.next() else {
            return Ok(Lookup::NotVisible);
        };
        if matches.next().is_some()
            || order.third_order_id != reference.merchant_order_no
            || order.parent_type.0 != "0"
            || !order.child_order_ids().is_empty()
        {
            return Err(mapping("京东收货确认目标不是原订单当前有效子单，请核对拆单结果"));
        }
        Ok(Lookup::Found(order))
    }
}

fn receipt_state(order: &JdOrder) -> ConnectorResult<ReceiptConfirmationState> {
    // 官方5836明确要求厂家直送通知，订单FAQ将orderType=5定义为厂家直送。
    // 本期普通商品(orderType=1)无需自动通知；此业务策略不表示京东禁止该接口。
    match order.order_type.as_ref().map(|value| value.0.as_str()) {
        Some("1") => return Ok(ReceiptConfirmationState::NotRequired),
        Some("5") => {},
        _ => {
            return Err(error(
                SupplierFailureClass::CapabilityGap,
                "JD_VOP_RECEIPT_ORDER_TYPE_UNVERIFIED",
                "当前京东订单类型的收货通知规则需人工核实，请先核对原单",
            ));
        },
    }
    let state = order.order_state.as_ref().ok_or_else(|| mapping("京东原子单收货状态尚未取得"))?;
    if state.cancel_order_state.as_ref().map(|value| value.0.as_str()) != Some("0") {
        return Err(mapping("京东原子单取消状态未核实或已经取消，已阻断收货通知"));
    }
    if state.confirm_state.as_ref().map(|value| value.0.as_str()) != Some("1") {
        return Err(mapping("京东原子单尚未核实预占确认，已阻断收货通知"));
    }
    match state.delivery_state.as_ref().map(|value| value.0.as_str()) {
        Some("1") => Ok(ReceiptConfirmationState::Confirmed),
        Some("0") => Ok(ReceiptConfirmationState::Unverified),
        _ => Err(mapping("京东原子单收货状态缺失、拒收或未识别，请核对原结果")),
    }
}

fn validate_request(request: &ConfirmReceipt) -> ConnectorResult<()> {
    let hashes = [&request.action.payload_hash, &request.evidence_fingerprint];
    if request.topology_version == 0
        || request.action.id.trim().is_empty()
        || request.action.id.len() > 256
        || hashes.iter().any(|value| value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()))
        || request.lines.is_empty()
        || request.lines.len() > 100
    {
        return Err(mapping("京东收货通知缺少原动作、当前子单或完整ERP收到证据"));
    }
    Ok(())
}

fn validate_received_lines(order: &JdOrder, lines: &[ReceiptLine]) -> ConnectorResult<()> {
    let mut received = BTreeMap::<String, Decimal>::new();
    let mut ids = BTreeSet::new();
    for line in lines {
        if line.line_id.trim().is_empty() || !ids.insert(&line.line_id) {
            return Err(mapping("ERP收到证据的原冻结明细缺失或重复"));
        }
        identifier(&line.sku.spec_id)?;
        add_quantity(&mut received, &line.sku.spec_id, line.quantity.to_decimal())?;
    }
    let mut original = BTreeMap::<String, Decimal>::new();
    for line in &order.sku_info_list {
        match line.line_type.as_ref().map(|kind| kind.0.as_str()) {
            Some("1") => continue,
            Some(kind) if kind != "0" => return Err(mapping("京东原子单含未支持的附加履约商品")),
            _ => {},
        }
        let quantity = line.sku_num.0.parse().map_err(|_| mapping("京东原子单商品数量格式无效"))?;
        add_quantity(&mut original, &line.sku_id.0, quantity)?;
    }
    if received != original || original.is_empty() {
        return Err(mapping("ERP实际收到范围与京东当前完整子单不一致，已阻断整单收货通知"));
    }
    Ok(())
}

fn add_quantity(totals: &mut BTreeMap<String, Decimal>, sku: &str, value: Decimal) -> ConnectorResult<()> {
    if value <= Decimal::ZERO || !value.fract().is_zero() || value > Decimal::from(i32::MAX) {
        return Err(mapping("京东收货商品数量须为接口范围内的正整数"));
    }
    let total = totals.entry(sku.into()).or_default();
    *total = total.checked_add(value).ok_or_else(|| mapping("京东收货商品数量合计超限"))?;
    if *total > Decimal::from(i32::MAX) {
        return Err(mapping("京东收货商品数量合计超出接口范围"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex};

    use erp_core::ids::SupplierApiConnectionId;
    use erp_supply::ports::connector::common::{ActionKey, SupplierSku};
    use serde_json::{Value, json};

    use super::super::JdVopOfferMapper;
    use super::super::order_payload::{JdVopExecutionProfile, JdVopInvoiceProfile};
    use super::super::transport::{JdVopReadTransport, JdVopWriteTransport, ReadClient, WriteClient};
    use super::*;

    struct Transport {
        order: Mutex<Value>,
        reads: AtomicUsize,
        writes: Mutex<Vec<JdVopWriteRequest>>,
        lose_response: AtomicBool,
        apply_before_loss: AtomicBool,
    }

    impl Default for Transport {
        fn default() -> Self {
            Self {
                order: Mutex::new(json!({"jdOrderId":"201","thirdOrderId":"merchant",
                    "orderType":"5","parentType":"0","parentJdOrderId":"100",
                    "childJdOrderIdList":[],"skuInfoList":[
                        {"skuId":"123","skuNum":"3","type":"0"},
                        {"skuId":"999","skuNum":"1","type":"1"}],
                    "orderState":{"confirmState":"1","cancelOrderState":"0",
                        "jdOrderState":"16","deliveryState":"0"}})),
                reads: AtomicUsize::new(0),
                writes: Mutex::new(vec![]),
                lose_response: AtomicBool::new(false),
                apply_before_loss: AtomicBool::new(false),
            }
        }
    }

    fn envelope(method: &str, value: Value) -> Vec<u8> {
        let key = format!("{}_responce", method.replace('.', "_"));
        serde_json::to_vec(&json!({key:{"openRpcResult":{"success":true,"resultCode":"0000",
            "result":value}}}))
        .unwrap()
    }

    #[async_trait]
    impl JdVopReadTransport for Transport {
        async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            let JdVopReadRequest::Order { merchant_order_no, external_order_no } = request else {
                panic!("receipt investigation only reads original order")
            };
            assert!(merchant_order_no.is_none());
            assert_eq!(external_order_no.as_deref(), Some("201"));
            self.reads.fetch_add(1, Ordering::SeqCst);
            Ok(envelope(request.method(), json!([self.order.lock().unwrap().clone()])))
        }
    }

    #[async_trait]
    impl JdVopWriteTransport for Transport {
        async fn write(&self, request: &JdVopWriteRequest) -> ConnectorResult<Vec<u8>> {
            let JdVopWriteRequest::ConfirmReceipt { merchant_order_no, external_order_no, source_type } =
                request
            else {
                panic!("receipt confirmation is independent of preoccupation confirmation")
            };
            assert_eq!(merchant_order_no, "merchant");
            assert_eq!(external_order_no, "201");
            assert_eq!(*source_type, 3);
            assert_eq!(request.method(), "jingdong.vop.order.confirmReceiveByOrder");
            self.writes.lock().unwrap().push(request.clone());
            if self.lose_response.load(Ordering::SeqCst) {
                if self.apply_before_loss.load(Ordering::SeqCst) {
                    self.order.lock().unwrap()["orderState"]["deliveryState"] = json!("1");
                }
                return Err(error(SupplierFailureClass::TransientFailure, "NETWORK_LOST", "lost"));
            }
            self.order.lock().unwrap()["orderState"]["deliveryState"] = json!("1");
            Ok(envelope(request.method(), json!(true)))
        }
    }

    fn profile() -> JdVopExecutionProfile {
        JdVopExecutionProfile {
            source_type: 3,
            evidence_reference: "verified-g0-account".into(),
            verified_external_funds_account_ref: "funds-account".into(),
            confirmation_funds_policy: None,
            preflight_validity_seconds: 60,
            ordinary_skus: BTreeSet::from(["123".into()]),
            verified_message_types: None,
            verified_funds_profile: None,
            invoice: JdVopInvoiceProfile {
                invoice_type: 23,
                invoice_put_type: 2,
                invoice_selected_title: 5,
                invoice_content_type: 1,
                invoice_company_name: "Buyer".into(),
                invoice_reg_code: Some("TAX".into()),
                invoice_reg_company_name: None,
                invoice_reg_address: None,
                invoice_reg_phone: None,
                invoice_reg_bank: None,
                invoice_reg_bank_account: None,
                recipient: None,
            },
        }
    }

    fn fixture() -> (JdVopConnector, Arc<Transport>, ConfirmReceipt) {
        let transport = Arc::new(Transport::default());
        let reader = Arc::new(ReadClient::new(transport.clone(), 2, 100, 5));
        let connector = JdVopConnector {
            connection_id: SupplierApiConnectionId::new("connection"),
            account_scope: "account".into(),
            binding: "binding".into(),
            cursor_secret: "secret".into(),
            reader: Some(reader.clone()),
            mapper: JdVopOfferMapper::default(),
            writer: Some(Arc::new(WriteClient { transport: transport.clone(), reader })),
            profile: Some(Arc::new(profile())),
            evidence_codec: None,
            message_runtime: None,
        };
        let request = ConfirmReceipt {
            action: ActionKey { id: "receipt-action".into(), payload_hash: "a".repeat(64) },
            order: OrderReference { merchant_order_no: "merchant".into(), external_order_no: "201".into() },
            topology_version: 1,
            lines: vec![ReceiptLine {
                line_id: "erp-line".into(),
                sku: SupplierSku { product_id: None, spec_id: "123".into() },
                quantity: "3".parse().unwrap(),
            }],
            evidence_fingerprint: "b".repeat(64),
        };
        (connector, transport, request)
    }

    #[tokio::test]
    async fn actual_received_factory_direct_leaf_is_notified_once_and_queries_do_not_create_erp_facts() {
        let (connector, transport, request) = fixture();
        assert_eq!(
            connector.confirm_receipt(&request).await.unwrap().value,
            ReceiptConfirmationState::Confirmed
        );
        assert_eq!(
            connector.confirm_receipt(&request).await.unwrap().value,
            ReceiptConfirmationState::Confirmed
        );
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
        let Lookup::Found(result) = connector.receipt_confirmation(&request.order).await.unwrap() else {
            panic!("original factory direct order remains visible")
        };
        assert_eq!(result.value, ReceiptConfirmationState::Confirmed);
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn ordinary_goods_queries_and_confirmation_are_not_required_and_never_notify() {
        for delivery_state in [Value::Null, json!("0"), json!("1")] {
            let (connector, transport, request) = fixture();
            {
                let mut order = transport.order.lock().unwrap();
                order["orderType"] = json!("1");
                order["orderState"]["deliveryState"] = delivery_state;
            }
            let Lookup::Found(result) = connector.receipt_confirmation(&request.order).await.unwrap() else {
                panic!("original ordinary goods leaf remains visible")
            };
            assert_eq!(result.value, ReceiptConfirmationState::NotRequired);
            assert_eq!(
                connector.confirm_receipt(&request).await.unwrap().value,
                ReceiptConfirmationState::NotRequired
            );
            assert!(transport.writes.lock().unwrap().is_empty());
            assert_eq!(transport.reads.load(Ordering::SeqCst), 2);
        }
    }

    #[tokio::test]
    async fn original_result_after_lost_response_is_queried_without_resending_or_using_jd_completion() {
        for applied in [false, true] {
            let (connector, transport, request) = fixture();
            transport.lose_response.store(true, Ordering::SeqCst);
            transport.apply_before_loss.store(applied, Ordering::SeqCst);
            assert_eq!(
                connector.confirm_receipt(&request).await.err().unwrap().class,
                SupplierFailureClass::ResultUnknown
            );
            transport.order.lock().unwrap()["orderState"]["jdOrderState"] = json!("19");
            for _ in 0..2 {
                let Lookup::Found(result) = connector.receipt_confirmation(&request.order).await.unwrap()
                else {
                    panic!("original leaf remains visible")
                };
                assert_eq!(
                    result.value,
                    if applied {
                        ReceiptConfirmationState::Confirmed
                    } else {
                        ReceiptConfirmationState::Unverified
                    }
                );
            }
            assert_eq!(transport.writes.lock().unwrap().len(), 1);
        }
    }

    #[tokio::test]
    async fn missing_evidence_partial_received_or_changed_leaf_stop_before_any_notification() {
        let (connector, transport, mut request) = fixture();
        request.evidence_fingerprint.clear();
        assert!(connector.confirm_receipt(&request).await.is_err());
        assert_eq!(transport.reads.load(Ordering::SeqCst), 0);
        for quantity in ["2", "4", "1.5"] {
            let (connector, transport, mut request) = fixture();
            request.lines[0].quantity = quantity.parse().unwrap();
            assert!(connector.confirm_receipt(&request).await.is_err());
            assert!(transport.writes.lock().unwrap().is_empty());
        }
        let (connector, transport, request) = fixture();
        transport.order.lock().unwrap()["childJdOrderIdList"] = json!(["301", "302"]);
        assert!(connector.confirm_receipt(&request).await.is_err());
        assert!(transport.writes.lock().unwrap().is_empty());
        let (connector, transport, request) = fixture();
        transport.order.lock().unwrap()["thirdOrderId"] = json!("other-merchant");
        assert!(connector.confirm_receipt(&request).await.is_err());
        assert!(transport.writes.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unknown_order_types_missing_states_and_returned_goods_are_never_implicitly_not_required() {
        for order_type in [Value::Null, json!("9"), json!("999")] {
            let (connector, transport, request) = fixture();
            transport.order.lock().unwrap()["orderType"] = order_type;
            assert_eq!(
                connector.receipt_confirmation(&request.order).await.err().unwrap().class,
                SupplierFailureClass::CapabilityGap
            );
            assert_eq!(
                connector.confirm_receipt(&request).await.err().unwrap().class,
                SupplierFailureClass::CapabilityGap
            );
            assert!(transport.writes.lock().unwrap().is_empty());
        }
        for state in [Value::Null, json!("2"), json!("999")] {
            let (connector, transport, request) = fixture();
            transport.order.lock().unwrap()["orderState"]["deliveryState"] = state;
            assert!(connector.receipt_confirmation(&request.order).await.is_err());
            assert!(transport.writes.lock().unwrap().is_empty());
        }
    }
}
