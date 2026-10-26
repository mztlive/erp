use std::collections::HashSet;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::{DeliveryId, PurchaseOrderId, SalesOrderLineId};
use erp_fulfillment::entity::fulfillment::{
    Delivery, DeliveryLine, DeliveryState, DeliveryTrackingEntry, DeliveryType,
};
use erp_fulfillment::repository::{DeliveryRepositoryExt, FulfillmentExt};
use erp_identity::AccessControlExt;
use erp_procurement::entity::purchase_order::{FulfillmentResponsibility, PurchaseOrder};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::ApiOrderExt;
use erp_supply::entity::supplier_fulfillment::{FulfillmentStatus, SupplierFulfillmentOrder};
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::order_tracking::{
    DeliveryCoverageLine, OrderObservation, OrderTracking, TrackingSourceLine, complete_delivery_coverage,
};
use erp_supply::repository::SupplierFulfillmentExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::SupplierOrderTrackingProcess;
use super::application::{ObservationApplication, apply_current_observation};
use super::persistence::load_claimed;
use crate::fulfillment_execution::post_observed_supplier_delivery;
use crate::{Error, Result};

impl SupplierOrderTrackingProcess {
    /// 在单个事务中将当前来源观察应用到真实 ERP 直发及检查点。
    /// # 参数
    /// 本轮查询权和已经持久化的来源观察身份。
    /// # 返回
    /// 保存包裹、必要发货事实及应用检查点。
    /// # 错误
    /// 来源变化、数量不符、当前采购/责任门槛不满足或写入失败时整体回滚。
    pub(super) async fn apply_delivery(&self, claimed: &OrderTracking, observation_id: &str) -> Result<()> {
        let (db, claimed, observation_id) = (self.db.clone(), claimed.clone(), observation_id.to_owned());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut tracking = load_claimed(&db, &claimed, executor).await?;
                    let observation = db
                        .supplier_order_observations()
                        .find_by_id(&observation_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商订单观察不存在".into()))?;
                    apply_current_observation(
                        &MongoObservationApplication { db: &db },
                        &mut tracking,
                        &observation,
                        Instant::now(),
                        executor,
                    )
                    .await?;
                    Ok(())
                })
            })
            .await
    }
}

struct MongoObservationApplication<'a> {
    db: &'a Database,
}

#[async_trait]
impl ObservationApplication for MongoObservationApplication<'_> {
    async fn cancelled_order(
        &self,
        order_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierFulfillmentOrder> {
        self.db
            .supplier_fulfillment_orders()
            .find_by_id(order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("供应商订单不存在".into()))
    }
    async fn deliveries(
        &self,
        tracking: &OrderTracking,
        observation: &OrderObservation,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        apply_erp_deliveries(self.db, tracking, observation, executor).await
    }
    async fn checkpoint(&self, tracking: &mut OrderTracking, executor: &mut dyn Executor) -> Result<()> {
        self.db.supplier_order_tracking().update(tracking, executor).await?;
        Ok(())
    }
}

async fn apply_erp_deliveries(
    db: &Database,
    tracking: &OrderTracking,
    observation: &OrderObservation,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let purchase = db
        .purchase_orders()
        .find_by_id(&tracking.purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;
    ensure_purchase_revision(db, tracking, &purchase, executor).await?;
    let complete = observation.fulfillment == Some(FulfillmentStatus::Completed);
    if purchase.fulfillment_responsibility != FulfillmentResponsibility::SupplierDirect {
        // 入仓只记录供应商运输结果，实际收货和自有库存继续由仓库确认。
        return Ok(complete);
    }
    if !observation.proves_dispatch() && observation.shipments.as_ref().is_none_or(Vec::is_empty) {
        return Ok(false);
    }
    let ids = tracking.direct_delivery_ids()?.into_iter().map(DeliveryId::new).collect::<Vec<_>>();
    let deliveries = db
        .deliveries()
        .find_for_purchase(&PurchaseOrderId::new(&tracking.purchase_order_id), &ids, executor)
        .await?;
    let mut covered = HashSet::new();
    for mut delivery in deliveries {
        let lines = db
            .fulfillment()
            .delivery_lines_by_delivery_ids(&[delivery.base.id.clone().into()], executor)
            .await?;
        let sources = tracking
            .source_lines
            .iter()
            .filter(|line| line.delivery_id.as_deref() == Some(delivery.base.id.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        let ids = coverage(db, &sources, &lines, executor).await?;
        if ids.is_empty() {
            continue;
        }
        if ids.iter().any(|id| !covered.insert(id.clone())) {
            return Err(Error::BusinessLogicError("同一供应商订货行关联多张完整发货单，请核对数量".into()));
        }
        apply_one_delivery(db, tracking, observation, &mut delivery, lines, executor).await?;
    }
    if observation.proves_dispatch() && covered.len() != tracking.source_lines.len() {
        return Err(Error::BusinessLogicError("来源发货结果尚未匹配全部 ERP 发货行，请核对采购分配".into()));
    }
    Ok(complete)
}

async fn ensure_purchase_revision(
    db: &Database,
    tracking: &OrderTracking,
    purchase: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let execution = db
        .supplier_api_order_executions()
        .find_by_id(&tracking.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原 API 发单执行依据不存在".into()))?;
    if purchase.stable.current_revision_id.as_ref().map(ToString::to_string)
        != Some(execution.purchase_order_revision_id)
    {
        return Err(Error::BusinessLogicError("来源采购版本已变化，请核对已发供应商订单后处理履约".into()));
    }
    Ok(())
}

async fn coverage(
    db: &Database,
    sources: &[TrackingSourceLine],
    lines: &[DeliveryLine],
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let mut facts = Vec::with_capacity(lines.len());
    for line in lines {
        let Some(id) = &line.purchase_line_sales_allocation_id else {
            return Ok(Vec::new());
        };
        let allocation = db
            .purchase_line_sales_allocations()
            .find_by_id(id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("直发采购分配不存在".into()))?;
        facts.push(DeliveryCoverageLine {
            purchase_order_revision_line_id: allocation.purchase_order_revision_line_id.to_string(),
            sales_order_line_id: line.sales_order_line_id.to_string(),
            quantity: line.quantity,
        });
    }
    complete_delivery_coverage(sources, &facts).map_err(Into::into)
}

async fn apply_one_delivery(
    db: &Database,
    tracking: &OrderTracking,
    observation: &OrderObservation,
    delivery: &mut Delivery,
    lines: Vec<DeliveryLine>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if delivery.delivery_type != DeliveryType::SupplierDirect || delivery.status == DeliveryState::Reversed {
        return Err(Error::BusinessLogicError("供应商结果不能改写仓发或已冲正发货单".into()));
    }
    let before = delivery.tracking_entries.clone();
    let entries = tracking_entries(tracking, observation)?
        .into_iter()
        .filter(|entry| lines.iter().any(|line| line.sales_order_line_id == entry.sales_order_line_id))
        .collect();
    delivery.merge_observed_tracking(entries)?;
    delivery.ensure_tracking_lines(&lines)?;
    if delivery.status == DeliveryState::Draft && observation.proves_dispatch() {
        let actor = delegated_actor(db, &tracking.purchase_order_id, executor).await?;
        let source_time = if observation.fulfillment == Some(FulfillmentStatus::Shipped) {
            observation.source_updated_at
        } else {
            None
        };
        post_observed_supplier_delivery(db, delivery.clone(), lines, source_time, actor, executor).await?;
    } else if before != delivery.tracking_entries {
        db.deliveries().update(delivery, executor).await?;
    }
    Ok(())
}

async fn delegated_actor(
    db: &Database,
    purchase_id: &str,
    executor: &mut dyn Executor,
) -> Result<AuditActor> {
    let purchase = db
        .purchase_orders()
        .find_by_id(purchase_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;
    let owner = purchase
        .owner_user_id
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("自动履约缺少采购责任人".into()))?;
    let account = db
        .accounts()
        .find_by_id(owner, executor)
        .await?
        .filter(|account| account.is_active_backoffice())
        .ok_or_else(|| Error::BusinessLogicError("自动履约责任人已失效，请重新交接".into()))?;
    Ok(AuditActor::new(account.base.id.clone(), account.secret.account().into(), account.kind)
        .with_actor_name_snapshot(Some(account.name))?)
}

fn tracking_entries(
    tracking: &OrderTracking,
    observation: &OrderObservation,
) -> Result<Vec<DeliveryTrackingEntry>> {
    let Some(shipments) = &observation.shipments else {
        return Ok(Vec::new());
    };
    if shipments.len() > 1 && shipments.iter().any(|shipment| shipment.line_ids.is_empty()) {
        return Err(Error::BusinessLogicError("多个包裹未提供明细关联，请核对后登记物流".into()));
    }
    let mut entries = Vec::new();
    for shipment in shipments {
        let Some(number) = &shipment.tracking_no else {
            continue;
        };
        for source in &tracking.source_lines {
            if !shipment.line_ids.is_empty() && !shipment.line_ids.contains(&source.fulfillment_item_id) {
                continue;
            }
            if let Some(sales) = &source.sales_order_line_id {
                entries.push(DeliveryTrackingEntry {
                    sales_order_line_id: SalesOrderLineId::new(sales),
                    tracking_no: number.clone(),
                    carrier: shipment.carrier.clone(),
                });
            }
        }
    }
    Ok(entries)
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_supply::entity::supplier_fulfillment::{CancelStatus, RefundStatus};
    use erp_supply::order_tracking::{OrderShipment, TrackingSourceLine};
    use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp};
    use erp_supply::ports::connector::order::{OrderReference, OrderSnapshot};

    use super::*;

    fn tracking() -> OrderTracking {
        OrderTracking::new(
            "order".into(),
            "connection".into(),
            "purchase".into(),
            vec![
                TrackingSourceLine {
                    fulfillment_item_id: "one".into(),
                    purchase_order_revision_line_id: "purchase-one".into(),
                    sales_order_line_id: Some("sale-one".into()),
                    delivery_id: Some("delivery-one".into()),
                    quantity: "1".parse().unwrap(),
                },
                TrackingSourceLine {
                    fulfillment_item_id: "two".into(),
                    purchase_order_revision_line_id: "purchase-two".into(),
                    sales_order_line_id: Some("sale-two".into()),
                    delivery_id: Some("delivery-two".into()),
                    quantity: "2".parse().unwrap(),
                },
            ],
        )
        .unwrap()
    }
    fn observation(tracking: &OrderTracking) -> OrderObservation {
        OrderObservation::from_snapshot(
            "observation".into(),
            tracking,
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(20),
                },
                value: OrderSnapshot {
                    reference: OrderReference {
                        merchant_order_no: "merchant".into(),
                        external_order_no: "external".into(),
                    },
                    amounts: None,
                    fulfillment: Some(FulfillmentStatus::Completed),
                    cancellation: None,
                    refund: None,
                    payment: None,
                    shipments: None,
                },
            },
        )
        .unwrap()
    }
    fn shipment(ids: &[&str], number: &str) -> OrderShipment {
        OrderShipment {
            external_shipment_id: None,
            line_ids: ids.iter().map(|id| (*id).into()).collect(),
            carrier: Some("顺丰".into()),
            tracking_no: Some(number.into()),
            has_courier_contact: false,
        }
    }
    #[test]
    fn packages_preserve_exact_source_line_association() {
        let tracking = tracking();
        let mut observation = observation(&tracking);
        observation.shipments = Some(vec![shipment(&["one"], "SF-1"), shipment(&["two"], "SF-2")]);
        let entries = tracking_entries(&tracking, &observation).unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].sales_order_line_id.as_ref(), "sale-one");
        assert_eq!(entries[1].tracking_no, "SF-2");
    }
    #[test]
    fn ambiguous_multiple_packages_require_manual_mapping() {
        let tracking = tracking();
        let mut observation = observation(&tracking);
        observation.shipments = Some(vec![shipment(&[], "SF-1"), shipment(&[], "SF-2")]);
        assert!(tracking_entries(&tracking, &observation).is_err());
        observation.shipments = Some(vec![shipment(&[], "SF-1")]);
        assert_eq!(tracking_entries(&tracking, &observation).unwrap().len(), 2);
    }
    #[test]
    fn refund_and_stale_observations_require_their_own_validation() {
        let mut tracking = tracking();
        let mut observation = observation(&tracking);
        tracking.latest_observation_id = Some(observation.base.id.clone());
        observation.cancellation = Some(CancelStatus::Canceled);
        assert!(tracking.ensure_current_observation(&observation).is_ok());
        observation.cancellation = None;
        observation.refund = Some(RefundStatus::Refunded);
        assert!(tracking.ensure_current_observation(&observation).is_err());
        observation.refund = None;
        tracking.latest_observation_id = Some("newer-observation".into());
        assert!(tracking.ensure_current_observation(&observation).is_err());
    }
}
