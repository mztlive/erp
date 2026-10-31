//! 逐叶子合并配送单元数据与多运单轨迹，不由运单推算商品数量。

use std::collections::{BTreeMap, BTreeSet};

use chrono::NaiveDateTime;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::order::{Shipment, ShipmentEvent};
use serde::Serialize;

use super::transport::JdVopReadRequest;
use super::wire_logistics::DeliveryInfo;
use super::wire_order::{JdOrder, Logistics, TrackEvent};
use super::{JdVopConnector, mapping, protocol_required};

#[derive(Serialize)]
pub(super) struct LogisticsEvidence {
    source_order_no: String,
    tracks: Logistics,
    delivery: DeliveryInfo,
}

pub(super) struct LogisticsObservation {
    pub shipments: Vec<Shipment>,
    pub evidence: LogisticsEvidence,
}

impl JdVopConnector {
    /// 查询当前子单的配送单与物流轨迹，保持来源证据与商品数量相互独立。
    /// # 参数
    /// `order` 为服务器已经核验归属的原订单节点。
    /// # 返回
    /// 合并后的运单、订单级轨迹及可加密归档的原始类型化观察。
    /// # 错误
    /// 账号资料缺失、来源归属变化、冲突运单或远端查询失败时拒绝应用。
    pub(super) async fn logistics(&self, order: &JdOrder) -> ConnectorResult<LogisticsObservation> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        let tracks: Logistics = reader
            .read(JdVopReadRequest::Logistics { external_order_no: order.jd_order_id.0.clone() })
            .await?;
        let delivery: DeliveryInfo = reader
            .read(JdVopReadRequest::DeliveryInfo {
                external_order_no: order.jd_order_id.0.clone(),
                source_type: profile.source_type,
            })
            .await?;
        validate_delivery(order, &delivery)?;
        let shipments = self.merge_logistics(order, &tracks, &delivery)?;
        Ok(LogisticsObservation {
            shipments,
            evidence: LogisticsEvidence { source_order_no: order.jd_order_id.0.clone(), tracks, delivery },
        })
    }

    fn merge_logistics(
        &self,
        order: &JdOrder,
        tracks: &Logistics,
        delivery: &DeliveryInfo,
    ) -> ConnectorResult<Vec<Shipment>> {
        let external = &order.jd_order_id.0;
        let mut shipments = delivery_shipments(external, delivery)?;
        let mut seen = BTreeSet::new();
        for track in &tracks.third_ps_show_result {
            validate_waybill(&track.third_id)?;
            if !seen.insert(&track.third_id) {
                return Err(mapping("京东同一子单返回重复三方运单，请核对物流资料"));
            }
            let shipment = shipments
                .entry(track.third_id.clone())
                .or_insert_with(|| shipment(external, Some(track.third_id.clone()), None));
            let track_carrier = carrier(&track.third_name)?;
            if shipment.carrier.is_some() && track_carrier.is_some() && shipment.carrier != track_carrier {
                return Err(mapping("京东同一运单返回冲突承运信息，请核对原结果"));
            }
            if shipment.carrier.is_none() {
                shipment.carrier = track_carrier;
            }
            shipment.events = self.track_events(external, track.third_ps_show_info_list.clone())?;
        }
        let mut result = shipments.into_values().collect::<Vec<_>>();
        let mut order_events = self.track_events(external, tracks.ziying_show_result.clone())?;
        merge_events(&mut order_events, self.delivery_events(delivery)?);
        if !order_events.is_empty() {
            let mut trajectory = shipment(external, None, None);
            trajectory.events = order_events;
            result.push(trajectory);
        }
        Ok(result)
    }

    fn delivery_events(&self, delivery: &DeliveryInfo) -> ConnectorResult<Vec<ShipmentEvent>> {
        let events = delivery
            .track_info_list
            .iter()
            .map(|event| TrackEvent {
                order_id: delivery.jd_order_id.clone(),
                msg_time: event.track_msg_time.clone(),
                content: event.track_content.clone(),
            })
            .collect();
        self.track_events(&delivery.jd_order_id.0, events)
    }

    fn track_events(&self, external: &str, events: Vec<TrackEvent>) -> ConnectorResult<Vec<ShipmentEvent>> {
        let codec = self.evidence_codec.as_ref().ok_or_else(protocol_required)?;
        events
            .into_iter()
            .map(|event| {
                if event.order_id.0 != external || event.content.len() > 16384 || event.msg_time.len() > 64 {
                    return Err(mapping("京东物流轨迹身份或长度无效"));
                }
                if NaiveDateTime::parse_from_str(&event.msg_time, "%Y-%m-%d %H:%M:%S").is_err() {
                    return Err(mapping("京东物流轨迹来源时间格式无效"));
                }
                let encrypted = codec.encrypt(&event.content).map_err(|_| mapping("京东物流轨迹加密失败"))?;
                Ok(ShipmentEvent {
                    source_time_text: event.msg_time,
                    description: "物流信息已更新".into(),
                    content_fingerprint: Some(self.hash(&event.content)?),
                    encrypted_description: Some(encrypted),
                })
            })
            .collect()
    }
}

fn validate_delivery(order: &JdOrder, delivery: &DeliveryInfo) -> ConnectorResult<()> {
    if delivery.jd_order_id != order.jd_order_id || delivery.third_order_id != order.third_order_id {
        return Err(mapping("京东配送信息与原子单或第三方订单归属不一致"));
    }
    for waybill in &delivery.logistic_info_list {
        if waybill.jd_order_id != order.jd_order_id
            || waybill
                .parent_jd_order_id
                .as_ref()
                .is_some_and(|parent| parent.0 != "0" && order.parent_jd_order_id.as_ref() != Some(parent))
        {
            return Err(mapping("京东运单不属于当前已核实子单，请核对拆单后的物流资料"));
        }
    }
    Ok(())
}

fn delivery_shipments(
    external: &str,
    delivery: &DeliveryInfo,
) -> ConnectorResult<BTreeMap<String, Shipment>> {
    let mut shipments = BTreeMap::<String, Shipment>::new();
    for waybill in &delivery.logistic_info_list {
        validate_waybill(&waybill.delivery_order_id)?;
        let carrier = waybill.delivery_carrier.as_deref().map(carrier).transpose()?.flatten();
        if let Some(previous) = shipments.get(&waybill.delivery_order_id) {
            if previous.carrier != carrier {
                return Err(mapping("京东同一运单返回冲突承运信息，请核对原结果"));
            }
        } else {
            shipments.insert(
                waybill.delivery_order_id.clone(),
                shipment(external, Some(waybill.delivery_order_id.clone()), carrier),
            );
        }
    }
    Ok(shipments)
}

fn shipment(external: &str, waybill: Option<String>, carrier: Option<String>) -> Shipment {
    Shipment {
        external_shipment_id: waybill.clone(),
        line_ids: vec![],
        source_order_no: Some(external.into()),
        quantity_lines: None,
        complete_line_coverage: false,
        carrier,
        tracking_no: waybill,
        courier_phone: None,
        events: vec![],
    }
}

fn validate_waybill(value: &str) -> ConnectorResult<()> {
    if value.trim().is_empty()
        || value != value.trim()
        || value.len() > 256
        || value.chars().any(char::is_control)
    {
        return Err(mapping("京东运单号缺失或格式无效"));
    }
    Ok(())
}

fn carrier(value: &str) -> ConnectorResult<Option<String>> {
    if value.len() > 256 || value.chars().any(char::is_control) {
        return Err(mapping("京东承运信息格式无效"));
    }
    Ok((!value.trim().is_empty()).then(|| value.trim().into()))
}

fn merge_events(target: &mut Vec<ShipmentEvent>, events: Vec<ShipmentEvent>) {
    let mut seen = target
        .iter()
        .map(|event| (event.source_time_text.clone(), event.content_fingerprint.clone()))
        .collect::<BTreeSet<_>>();
    for event in events {
        if seen.insert((event.source_time_text.clone(), event.content_fingerprint.clone())) {
            target.push(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use erp_core::ids::SupplierApiConnectionId;
    use erp_party::SensitiveDataCodec;
    use serde_json::{Value, from_value, json};

    use super::super::JdVopOfferMapper;
    use super::*;

    fn connector() -> JdVopConnector {
        JdVopConnector {
            connection_id: SupplierApiConnectionId::new("connection"),
            account_scope: "account".into(),
            binding: "binding".into(),
            cursor_secret: "secret".into(),
            reader: None,
            mapper: JdVopOfferMapper::default(),
            writer: None,
            profile: None,
            evidence_codec: Some(Arc::new(SensitiveDataCodec::from_secret(b"logistics-tests"))),
            message_runtime: None,
        }
    }

    fn order() -> JdOrder {
        from_value(json!({"jdOrderId":"201","thirdOrderId":"merchant","parentType":"0",
            "parentJdOrderId":"100","childJdOrderIdList":[],"skuInfoList":[]}))
        .unwrap()
    }

    fn metadata(waybills: Value) -> DeliveryInfo {
        from_value(json!({"jdOrderId":"201","thirdOrderId":"merchant",
            "logisticInfoList":waybills,"trackInfoList":[]}))
        .unwrap()
    }

    fn waybill(number: &str, name: &str) -> Value {
        json!({"deliveryOrderId":number,"deliveryCarrier":name,"jdOrderId":"201",
            "parentJdOrderId":"100"})
    }

    fn tracks() -> Logistics {
        from_value(json!({"ziyingShowResult":[],"thirdPsShowResult":[]})).unwrap()
    }

    #[test]
    fn self_operated_multi_waybills_keep_identity_without_allocating_order_trace_or_quantity() {
        let connector = connector();
        let mut delivery = metadata(json!([waybill("JD-1", "京东快递"), waybill("JD-2", "京东快递")]));
        delivery.track_info_list = from_value(json!([{"trackMsgTime":"2026-10-29 12:00:00",
            "trackContent":"Receiver 13800000000","trackOperator":"operator"}]))
        .unwrap();
        let mut tracks = tracks();
        tracks.ziying_show_result = from_value(json!([{"orderId":"201","msgTime":"2026-10-29 12:00:00",
            "content":"Receiver 13800000000"}]))
        .unwrap();
        validate_delivery(&order(), &delivery).unwrap();
        let shipments = connector.merge_logistics(&order(), &tracks, &delivery).unwrap();
        assert_eq!(shipments.len(), 3);
        assert_eq!(shipments[0].tracking_no.as_deref(), Some("JD-1"));
        assert_eq!(shipments[1].tracking_no.as_deref(), Some("JD-2"));
        assert!(shipments[..2].iter().all(|shipment| shipment.carrier.as_deref() == Some("京东快递")
            && shipment.events.is_empty()));
        assert!(shipments[2].tracking_no.is_none());
        assert_eq!(shipments[2].events.len(), 1);
        assert!(shipments.iter().all(|shipment| shipment.source_order_no.as_deref() == Some("201")
            && shipment.quantity_lines.is_none()
            && shipment.line_ids.is_empty()
            && !shipment.complete_line_coverage));
        assert!(!serde_json::to_string(&shipments[2].events).unwrap().contains("13800000000"));
    }

    #[test]
    fn metadata_and_third_party_tracks_merge_once_by_the_same_leaf_waybill() {
        let connector = connector();
        let delivery = metadata(json!([waybill("SF-1", "顺丰快递"), waybill("SF-1", "顺丰快递")]));
        let tracks = from_value(json!({"ziyingShowResult":[],"thirdPsShowResult":[{
            "thirdId":"SF-1","thirdName":"顺丰快递","thirdPsShowInfoList":[{
                "orderId":"201","msgTime":"2026-10-29 12:00:00","content":"运输中"}]}]}))
        .unwrap();
        let shipments = connector.merge_logistics(&order(), &tracks, &delivery).unwrap();
        assert_eq!(shipments.len(), 1);
        assert_eq!(shipments[0].tracking_no.as_deref(), Some("SF-1"));
        assert_eq!(shipments[0].carrier.as_deref(), Some("顺丰快递"));
        assert_eq!(shipments[0].events.len(), 1);
        assert!(shipments[0].quantity_lines.is_none());
    }

    #[test]
    fn foreign_order_parent_and_conflicting_or_invalid_waybill_never_become_shipments() {
        let mut delivery = metadata(json!([waybill("JD-1", "京东快递")]));
        delivery.third_order_id = "other-merchant".into();
        assert!(validate_delivery(&order(), &delivery).is_err());
        delivery.third_order_id = "merchant".into();
        delivery.logistic_info_list[0].jd_order_id.0 = "202".into();
        assert!(validate_delivery(&order(), &delivery).is_err());
        delivery.logistic_info_list[0].jd_order_id.0 = "201".into();
        delivery.logistic_info_list[0].parent_jd_order_id.as_mut().unwrap().0 = "999".into();
        assert!(validate_delivery(&order(), &delivery).is_err());
        let conflicts = metadata(json!([waybill("JD-1", "京东快递"), waybill("JD-1", "other")]));
        assert!(delivery_shipments("201", &conflicts).is_err());
        let conflicting_track: Logistics = from_value(json!({"ziyingShowResult":[],
            "thirdPsShowResult":[{"thirdId":"JD-1","thirdName":"other","thirdPsShowInfoList":[]}]}))
        .unwrap();
        assert!(
            connector()
                .merge_logistics(
                    &order(),
                    &conflicting_track,
                    &metadata(json!([waybill("JD-1", "京东快递")]))
                )
                .is_err()
        );
        for id in ["", " JD-1", "JD\n1"] {
            assert!(delivery_shipments("201", &metadata(json!([waybill(id, "京东快递")]))).is_err());
        }
        assert!(connector().merge_logistics(&order(), &tracks(), &metadata(json!([]))).unwrap().is_empty());
    }
}
