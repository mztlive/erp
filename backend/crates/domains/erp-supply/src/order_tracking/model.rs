use std::collections::{BTreeSet, HashSet};

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Quantity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::entity::supplier_fulfillment::{
    CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentOrder,
};
use crate::ports::connector::common::{Snapshot, SourceRevision};
use crate::ports::connector::order::{
    EncryptedOrderEvidence, OrderSnapshot, PaymentState, Shipment, ShipmentEvent, ShipmentLine,
};
use crate::ports::connector::order_tree::OrderTree;
use crate::{Error, Result};

/// 外部订货行与已批准采购、销售行的冻结关联。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrackingSourceLine {
    pub fulfillment_item_id: String,
    pub purchase_order_revision_line_id: String,
    pub sales_order_line_id: Option<String>,
    #[serde(default)]
    pub delivery_id: Option<String>,
    pub quantity: Quantity,
}

/// 跟踪记录的应用进度；完成来源观察仍可等待内部履约处理。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum TrackingState {
    Polling,
    Manual,
    Complete,
}

/// 无个人地址、手机号或原始报文的包裹事实。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OrderShipment {
    pub external_shipment_id: Option<String>,
    pub line_ids: Vec<String>,
    #[serde(default)]
    pub source_order_no: Option<String>,
    #[serde(default)]
    pub quantity_lines: Option<Vec<ShipmentLine>>,
    #[serde(default)]
    pub complete_line_coverage: bool,
    pub carrier: Option<String>,
    pub tracking_no: Option<String>,
    /// 来源提供了配送员联系方式；本投影不保存号码，不能作为物流单号展示。
    pub has_courier_contact: bool,
    #[serde(default)]
    pub events: Vec<ShipmentEvent>,
}

/// 调度和应用检查点；与正式供应商订单使用相同 id。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct OrderTracking {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub purchase_order_id: String,
    pub source_lines: Vec<TrackingSourceLine>,
    pub state: TrackingState,
    pub poll_after: Instant,
    pub lease_id: Option<String>,
    pub lease_until: Option<Instant>,
    pub latest_observation_id: Option<String>,
    pub applied_observation_id: Option<String>,
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_verified_at: Option<Instant>,
    #[serde(default)]
    pub applied_shipments: Vec<super::AppliedShipment>,
}

impl OrderTracking {
    /// 核对当前待应用观察及各来源轨道，禁止旧观察覆盖新查询。
    /// 身份及最新观察匹配时允许后续应用；取消结果另行核对已发货和退款冲突。
    /// 身份或版本不符、独立退款缺少正式依据时拒绝。
    pub fn ensure_current_observation(&self, observation: &OrderObservation) -> Result<()> {
        if observation.order_id != self.base.id
            || observation.connection_id != self.connection_id
            || self.latest_observation_id.as_deref() != Some(observation.base.id.as_str())
        {
            return Err(Error::ConflictError("订单跟踪观察已变化，请重新查询".into()));
        }

        if observation.cancellation == Some(CancelStatus::Canceled) && !self.applied_shipments.is_empty() {
            return Err(Error::BusinessLogicError("订单已经存在真实分批发货，请沿原履约流程处理取消".into()));
        }
        if observation.cancellation != Some(CancelStatus::Canceled)
            && observation.refund.is_some_and(|status| status != RefundStatus::None)
        {
            return Err(Error::BusinessLogicError(
                "供应商退款进度需要单笔退款依据，不自动生成退款或成本冲减".into(),
            ));
        }
        Ok(())
    }

    /// 完成当前观察的业务应用，不改动供应商来源或生成交付事实。
    /// 更新 applied 身份、清理错误和查询权，保留最新来源证据。
    /// 观察已变化时拒绝完成旧检查点。
    pub fn record_applied(
        &mut self,
        observation: &OrderObservation,
        complete: bool,
        now: Instant,
    ) -> Result<()> {
        self.ensure_current_observation(observation)?;

        self.applied_observation_id = Some(observation.base.id.clone());
        self.state = if complete { TrackingState::Complete } else { TrackingState::Polling };
        self.last_error = None;
        self.lease_id = None;
        self.lease_until = None;
        self.poll_after = Instant::from_unix_secs(now.unix_secs().saturating_add(60));
        Ok(())
    }

    /// 判断回调是否可唤醒该订单，已被采购变更替代的执行永不重新调度。
    /// 未退役且没有活跃查询权时返回 true；正常完成订单仍可接收新的来源变化。
    /// `superseded` 为同一事务读取的执行退役事实；`now` 为调度时刻。
    pub fn can_refresh(&self, superseded: bool, now: Instant) -> bool {
        !superseded && self.lease_until.is_none_or(|until| until <= now)
    }

    /// 结束被采购变更替代的旧执行跟踪，不改写已有来源或履约证据。
    /// 清理查询权并标记完成，保留原始观察与应用身份。
    /// 调用方须在同一事务证明执行已经退役。
    pub fn stop_for_purchase_change(&mut self) {
        self.state = TrackingState::Complete;
        self.lease_id = None;
        self.lease_until = None;
        self.last_error = Some("采购变更已替代安全停止的原执行，已停止原单跟踪".into());
    }

    /// 取得本子单明确冻结的直发单身份集合。
    /// 去重且稳定排序的发货单身份。
    /// 任一来源行未绑定发货单时拒绝推断。
    /// 调用方须已经证明采购履约责任为供应商直发。
    pub fn direct_delivery_ids(&self) -> Result<Vec<String>> {
        self.source_lines
            .iter()
            .map(|line| {
                line.delivery_id
                    .clone()
                    .ok_or_else(|| Error::BusinessLogicError("API 直发缺少冻结发货单关联，请核对".into()))
            })
            .collect::<Result<BTreeSet<_>>>()
            .map(|ids| ids.into_iter().collect())
    }

    /// 为同一事务中已创建的正式供应商订单登记跟踪关联。
    /// 可立即调度的跟踪记录。
    /// 身份为空、行为空/超过 200、重复来源行或数量非正时拒绝。
    pub fn new(
        order_id: String,
        connection_id: String,
        purchase_order_id: String,
        source_lines: Vec<TrackingSourceLine>,
    ) -> Result<Self> {
        let mut ids = HashSet::new();
        if [&order_id, &connection_id, &purchase_order_id].iter().any(|id| id.trim().is_empty())
            || source_lines.is_empty()
            || source_lines.len() > 200
            || source_lines.iter().any(|line| {
                line.fulfillment_item_id.trim().is_empty()
                    || line.purchase_order_revision_line_id.trim().is_empty()
                    || line.quantity.to_decimal() <= Decimal::ZERO
                    || !ids.insert(line.fulfillment_item_id.clone())
                    || line.sales_order_line_id.as_ref().is_some_and(|id| id.trim().is_empty())
                    || line.delivery_id.as_ref().is_some_and(|id| id.trim().is_empty())
            })
        {
            return Err(Error::ValidationError("供应商订单跟踪来源无效".into()));
        }
        Ok(Self {
            base: BaseModel::new(order_id),
            connection_id,
            purchase_order_id,
            source_lines,
            state: TrackingState::Polling,
            poll_after: Instant::now(),
            lease_id: None,
            lease_until: None,
            latest_observation_id: None,
            applied_observation_id: None,
            last_error: None,
            last_verified_at: None,
            applied_shipments: Vec::new(),
        })
    }
}

/// 查询所得不可变来源观察；本地主键不冒充供应商事件身份。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct OrderObservation {
    #[serde(flatten)]
    pub base: BaseModel,
    pub order_id: String,
    pub connection_id: String,
    pub merchant_order_no: String,
    pub external_order_no: String,
    pub source_revision: SourceRevision,
    pub source_updated_at: Option<Instant>,
    pub observed_at: Instant,
    pub fulfillment: Option<FulfillmentStatus>,
    pub cancellation: Option<CancelStatus>,
    pub refund: Option<RefundStatus>,
    pub payment_confirmed: Option<bool>,
    pub shipments: Option<Vec<OrderShipment>>,
    #[serde(default)]
    pub topology: Option<OrderTree>,
    #[serde(default)]
    pub complete_line_coverage: bool,
    #[serde(default)]
    pub source_evidence: Option<EncryptedOrderEvidence>,
}

impl OrderObservation {
    /// 核对已正式应用的供应商取消及未发货、退款已解决条件。
    /// 明确取消且不存在已知发货或未决退款时允许仅完成跟踪检查点。
    /// 身份、取消、发货或退款证据不一致时拒绝，不生成取消或资金事实。
    /// `order` 为同一事务内读取的正式供应商订单。
    pub fn ensure_safe_cancellation(&self, order: &SupplierFulfillmentOrder) -> Result<()> {
        self.ensure_leaf_cancellations()?;
        let before_dispatch = |status| {
            matches!(
                status,
                FulfillmentStatus::Received
                    | FulfillmentStatus::Submitting
                    | FulfillmentStatus::Accepted
                    | FulfillmentStatus::Rejected
            )
        };
        let refund_resolved = matches!(
            (order.refund_status, self.refund),
            (RefundStatus::None, None | Some(RefundStatus::None))
                | (RefundStatus::Refunded, Some(RefundStatus::Refunded))
        );
        if self.order_id != order.base.id
            || self.connection_id != order.connection_id.as_ref()
            || self.merchant_order_no != order.fulfillment_order_no
            || order.external_order_no.as_deref() != Some(self.external_order_no.as_str())
            || self.cancellation != Some(CancelStatus::Canceled)
            || order.cancel_status != CancelStatus::Canceled
            || !before_dispatch(order.fulfillment_status)
            || self.fulfillment.is_some_and(|status| !before_dispatch(status))
            || self.shipments.as_ref().is_some_and(|shipments| !shipments.is_empty())
            || !refund_resolved
        {
            return Err(Error::BusinessLogicError(
                "供应商取消尚未核实或存在已发货、未决退款，请核对原单后处理".into(),
            ));
        }
        Ok(())
    }

    /// 将查询快照转换为不含个人资料的来源证据。
    /// 保留缺失业务时间、独立轨道与包裹行关联的观察。
    /// 身份、包裹数量、字段长度或已冻结来源行不一致时拒绝。
    pub fn from_snapshot(
        id: String,
        tracking: &OrderTracking,
        snapshot: Snapshot<OrderSnapshot>,
    ) -> Result<Self> {
        let value = snapshot.value;
        if value.reference.merchant_order_no.is_empty() || value.reference.external_order_no.is_empty() {
            return Err(Error::ValidationError("供应商查询缺少订单身份".into()));
        }
        if let Some(topology) = &value.topology
            && topology.root_order_no != value.reference.external_order_no
        {
            return Err(Error::ConflictError("来源订单树不属于原始根单".into()));
        }

        let shipments = value
            .shipments
            .map(|shipments| shipments.into_iter().map(OrderShipment::from_connector).collect::<Vec<_>>());
        validate_shipments(shipments.as_deref(), &tracking.source_lines)?;

        Ok(Self {
            base: BaseModel::new(id),
            order_id: tracking.base.id.clone(),
            connection_id: tracking.connection_id.clone(),
            merchant_order_no: value.reference.merchant_order_no,
            external_order_no: value.reference.external_order_no,
            source_revision: snapshot.stamp.revision,
            source_updated_at: snapshot.stamp.changed_at,
            observed_at: snapshot.stamp.observed_at,
            fulfillment: value.fulfillment,
            cancellation: value.cancellation,
            refund: value.refund,
            payment_confirmed: value.payment.map(|state| state == PaymentState::Confirmed),
            shipments,
            topology: value.topology,
            complete_line_coverage: value.complete_line_coverage,
            source_evidence: value.source_evidence,
        })
    }

    /// 判定该查询是否证明整个供应商子单已进入发货或完成阶段。
    /// 仅显式已发货或已完成返回 true，单有物流号不扩大成全单已发货。
    pub fn proves_dispatch(&self) -> bool {
        matches!(self.fulfillment, Some(FulfillmentStatus::Shipped | FulfillmentStatus::Completed))
    }

    /// 判断适配器是否明确证明整个冻结明细已覆盖。
    /// 整单证明或全部包裹的显式证明成立时返回 true；默认缺失不推断。
    pub fn proves_complete_coverage(&self) -> bool {
        self.complete_line_coverage
            || self.shipments.as_ref().is_some_and(|shipments| {
                !shipments.is_empty() && shipments.iter().all(|shipment| shipment.complete_line_coverage)
            })
    }

    /// 比较业务内容，重复查询不追加相同证据。
    /// 身份、来源版本及全部已取得字段相同返回 true。
    pub fn same_content(&self, previous: &Self) -> bool {
        self.order_id == previous.order_id
            && self.connection_id == previous.connection_id
            && self.merchant_order_no == previous.merchant_order_no
            && self.external_order_no == previous.external_order_no
            && self.source_revision == previous.source_revision
            && self.source_updated_at == previous.source_updated_at
            && self.fulfillment == previous.fulfillment
            && self.cancellation == previous.cancellation
            && self.refund == previous.refund
            && self.payment_confirmed == previous.payment_confirmed
            && same_shipments(self.shipments.as_deref(), previous.shipments.as_deref())
            && self.topology == previous.topology
            && self.complete_line_coverage == previous.complete_line_coverage
            && self.source_evidence.as_ref().map(|value| &value.content_fingerprint)
                == previous.source_evidence.as_ref().map(|value| &value.content_fingerprint)
    }
}

fn validate_shipments(shipments: Option<&[OrderShipment]>, lines: &[TrackingSourceLine]) -> Result<()> {
    let Some(shipments) = shipments else {
        return Ok(());
    };
    let ids = lines.iter().map(|line| line.fulfillment_item_id.as_str()).collect::<HashSet<_>>();
    if shipments.len() > 100
        || shipments.iter().any(|shipment| {
            shipment.line_ids.len() > 200
                || shipment.line_ids.iter().any(|id| !ids.contains(id.as_str()))
                || shipment
                    .tracking_no
                    .as_ref()
                    .is_some_and(|value| value.trim().is_empty() || value.len() > 128)
                || shipment.carrier.as_ref().is_some_and(|value| value.chars().count() > 64)
                || shipment
                    .external_shipment_id
                    .as_ref()
                    .is_some_and(|value| value.is_empty() || value.chars().count() > 128)
        })
    {
        return Err(Error::ValidationError("供应商包裹明细不属于原订单或超出限制".into()));
    }
    Ok(())
}

impl OrderShipment {
    fn from_connector(shipment: Shipment) -> Self {
        Self {
            external_shipment_id: shipment.external_shipment_id,
            line_ids: shipment.line_ids,
            source_order_no: shipment.source_order_no,
            quantity_lines: shipment.quantity_lines,
            complete_line_coverage: shipment.complete_line_coverage,
            carrier: shipment.carrier,
            tracking_no: shipment.tracking_no,
            has_courier_contact: shipment.courier_phone.is_some(),
            events: shipment.events,
        }
    }
}

fn same_shipments(current: Option<&[OrderShipment]>, previous: Option<&[OrderShipment]>) -> bool {
    match (current, previous) {
        (None, None) => true,
        (Some(current), Some(previous)) => {
            current.len() == previous.len()
                && current.iter().zip(previous).all(|(current, previous)| current.same_content(previous))
        },
        _ => false,
    }
}

impl OrderShipment {
    fn same_content(&self, previous: &Self) -> bool {
        self.external_shipment_id == previous.external_shipment_id
            && self.line_ids == previous.line_ids
            && self.source_order_no == previous.source_order_no
            && self.quantity_lines == previous.quantity_lines
            && self.complete_line_coverage == previous.complete_line_coverage
            && self.carrier == previous.carrier
            && self.tracking_no == previous.tracking_no
            && self.has_courier_contact == previous.has_courier_contact
            && self.events.len() == previous.events.len()
            && self.events.iter().zip(&previous.events).all(|(current, previous)| {
                current.source_time_text == previous.source_time_text
                    && current.description == previous.description
                    && match (&current.content_fingerprint, &previous.content_fingerprint) {
                        (Some(current), Some(previous)) => current == previous,
                        (None, None) => current.encrypted_description == previous.encrypted_description,
                        _ => false,
                    }
            })
    }
}
