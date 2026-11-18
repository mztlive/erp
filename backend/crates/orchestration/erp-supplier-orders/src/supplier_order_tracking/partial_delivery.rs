//! 按来源包裹数量拆分原直发草稿，并复用既有真实发货过账。
use std::collections::BTreeMap;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_core::ids::{DeliveryId, DeliveryLineId, SalesOrderLineId};
use erp_core::money::Quantity;
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryLine, DeliveryTrackingEntry};
use erp_fulfillment::repository::FulfillmentExt;
use erp_fulfillment::service::document_number::next_delivery_no;
use erp_fulfillment_execution::fulfillment_execution::post_observed_supplier_delivery;
use erp_fulfillment_execution::fulfillment_execution::task::{
    FulfillmentTaskObject, complete_fulfillment_task, ensure_fulfillment_task,
};
use erp_identity::AuthorizationService;
use erp_supply::entity::supplier_fulfillment::FulfillmentStatus;
use erp_supply::order_tracking::{OrderObservation, OrderTracking, PendingShipment, TrackingSourceLine};
use mongodb::Database;
use persistence_core::Executor;
use sha2::{Digest, Sha256};

use super::delivery::delegated_actor;
use crate::{Error, Result};

// 使用同一执行器写入全部包裹、剩余草稿和数量应用凭据。
//
// 已核验采购版本的跟踪记录、当前来源观察和根事务执行器。
//
// 来源完成且全部冻结数量已真正交付时返回 true。
//
// 任何数量、来源、草稿或正式发货门槛失败时回滚本次应用。
pub(super) async fn apply_quantity_deliveries(
    db: &Database,
    task_identity: &Arc<AuthorizationService>,
    tracking: &mut OrderTracking,
    observation: &OrderObservation,
    executor: &mut dyn Executor,
) -> Result<bool> {
    apply_quantity_observation(
        &MongoQuantityDeliveryApplication { db, task_identity, source_time: observation.source_updated_at },
        tracking,
        observation,
        executor,
    )
    .await
}

#[async_trait]
trait QuantityDeliveryApplication: Sync {
    async fn merge_tracking(
        &self,
        tracking: &OrderTracking,
        observation: &OrderObservation,
        executor: &mut dyn Executor,
    ) -> Result<()>;
    async fn dispatch(
        &self,
        tracking: &OrderTracking,
        shipment: &PendingShipment,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

struct MongoQuantityDeliveryApplication<'a> {
    db: &'a Database,
    task_identity: &'a Arc<AuthorizationService>,
    source_time: Option<Instant>,
}

#[async_trait]
impl QuantityDeliveryApplication for MongoQuantityDeliveryApplication<'_> {
    async fn merge_tracking(
        &self,
        tracking: &OrderTracking,
        observation: &OrderObservation,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        super::shipment_tracking::merge_tracking(self.db, tracking, observation, executor).await
    }
    async fn dispatch(
        &self,
        tracking: &OrderTracking,
        shipment: &PendingShipment,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let actor = delegated_actor(self.db, &tracking.purchase_order_id, executor).await?;
        let mut deliveries = Vec::new();
        for (draft, lines) in source_groups(tracking, shipment)? {
            let id = self.apply_partition(&draft, shipment, &lines, &actor, executor).await?;
            deliveries.push(id);
        }
        Ok(deliveries)
    }
}

async fn apply_quantity_observation(
    application: &impl QuantityDeliveryApplication,
    tracking: &mut OrderTracking,
    observation: &OrderObservation,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let pending = tracking.pending_shipments(observation)?;
    if pending.is_empty() {
        tracking.ensure_dispatch_quantities(observation)?;
    }

    for shipment in pending {
        let deliveries = application.dispatch(tracking, &shipment, executor).await?;
        tracking.record_shipment(&shipment, deliveries)?;
    }

    application.merge_tracking(tracking, observation, executor).await?;
    Ok(observation.fulfillment == Some(FulfillmentStatus::Completed)
        && tracking.quantities_fully_applied()?)
}

fn source_groups<'a>(
    tracking: &'a OrderTracking,
    shipment: &PendingShipment,
) -> Result<BTreeMap<String, Vec<(&'a TrackingSourceLine, Quantity)>>> {
    let mut groups = BTreeMap::new();
    for line in &shipment.lines {
        let source = tracking
            .source_lines
            .iter()
            .find(|source| source.fulfillment_item_id == line.line_id)
            .ok_or_else(|| Error::ValidationError("包裹明细不属于冻结采购".into()))?;
        let draft = source
            .delivery_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("包裹明细缺少原直发草稿，请核对采购分配".into()))?;
        groups.entry(draft.clone()).or_insert_with(Vec::new).push((source, line.quantity));
    }
    Ok(groups)
}

impl MongoQuantityDeliveryApplication<'_> {
    // 使用当前来源观察上下文分拆并正式过账一份来源包裹。
    async fn apply_partition(
        &self,
        draft_id: &str,
        shipment: &PendingShipment,
        sources: &[(&TrackingSourceLine, Quantity)],
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let (db, task_identity, source_time) = (self.db, self.task_identity, self.source_time);
        let mut draft = db
            .deliveries()
            .find_by_id(draft_id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("原直发草稿不存在或已处理，请核对分批发货".into()))?;
        let id = partition_id(draft_id, shipment)?;
        let mut delivery = draft.observed_partition(DeliveryId::new(&id), next_delivery_no(db).await?)?;
        let original =
            db.fulfillment().delivery_lines_by_delivery_ids(&[DeliveryId::new(draft_id)], executor).await?;
        let (lines, remaining) = split_lines(&original, sources, &id)?;
        delivery.merge_observed_tracking(package_tracking(shipment, &lines))?;

        db.fulfillment().create_delivery_with_lines(&delivery, &lines, executor).await?;
        ensure_fulfillment_task(db, self.task_identity, FulfillmentTaskObject::Delivery(&delivery), executor)
            .await?;
        post_observed_supplier_delivery(
            db,
            task_identity,
            delivery,
            lines,
            source_time,
            actor.clone(),
            executor,
        )
        .await?;

        persist_remainder(db, task_identity, &mut draft, &original, remaining, actor, executor).await?;
        Ok(id)
    }
}

type LineRemainder = Vec<Option<DeliveryLine>>;

fn split_lines(
    original: &[DeliveryLine],
    sources: &[(&TrackingSourceLine, Quantity)],
    id: &str,
) -> Result<(Vec<DeliveryLine>, LineRemainder)> {
    let mut remaining = original.iter().cloned().map(Some).collect::<Vec<_>>();
    let mut lines = Vec::new();
    for (source, quantity) in sources {
        let expected = format!("{}-delivery", source.fulfillment_item_id);
        let index = original
            .iter()
            .position(|line| line.base.id == expected)
            .ok_or_else(|| Error::ConflictError("来源包裹超过原草稿剩余范围，请核对原发货记录".into()))?;
        let line =
            remaining[index].take().ok_or_else(|| Error::ConflictError("包裹重复使用同一原直发行".into()))?;
        if source.sales_order_line_id.as_deref() != Some(line.sales_order_line_id.as_ref()) {
            return Err(Error::ConflictError("来源包裹与销售分配不一致".into()));
        }

        let (shipped, rest) = line.split_quantity(
            DeliveryId::new(id),
            DeliveryLineId::new(format!("{id}-{}", source.fulfillment_item_id)),
            *quantity,
        )?;
        lines.push(shipped);
        remaining[index] = rest;
    }
    Ok((lines, remaining))
}

async fn persist_remainder(
    db: &Database,
    task_identity: &Arc<AuthorizationService>,
    draft: &mut Delivery,
    original: &[DeliveryLine],
    remaining: LineRemainder,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let remaining_lines = remaining.iter().flatten().cloned().collect::<Vec<_>>();
    for (before, after) in original.iter().zip(remaining) {
        match after {
            Some(mut after) if after.quantity != before.quantity => {
                db.delivery_lines().update(&mut after, executor).await?;
            },
            None => {
                db.delivery_lines().soft_delete(&mut before.clone(), executor).await?;
            },
            _ => {},
        }
    }

    if remaining_lines.is_empty() {
        complete_fulfillment_task(db, task_identity, FulfillmentTaskObject::Delivery(draft), actor, executor)
            .await?;
        db.deliveries().soft_delete(draft, executor).await?;
    } else {
        draft.tracking_entries.retain(|entry| {
            remaining_lines.iter().any(|line| line.sales_order_line_id == entry.sales_order_line_id)
        });
        draft.ensure_tracking_lines(&remaining_lines)?;
        db.deliveries().update(draft, executor).await?;
    }
    Ok(())
}

fn package_tracking(shipment: &PendingShipment, lines: &[DeliveryLine]) -> Vec<DeliveryTrackingEntry> {
    let Some(number) = &shipment.tracking_no else {
        return Vec::new();
    };
    lines
        .iter()
        .map(|line| DeliveryTrackingEntry {
            sales_order_line_id: SalesOrderLineId::new(line.sales_order_line_id.as_ref()),
            tracking_no: number.clone(),
            carrier: shipment.carrier.clone(),
        })
        .collect()
}

fn partition_id(draft: &str, shipment: &PendingShipment) -> Result<String> {
    let encoded = serde_json::to_vec(&(draft, &shipment.source_order_no, &shipment.shipment_id))
        .map_err(|_| Error::ValidationError("包裹身份无法保存".into()))?;
    Ok(format!("{}-package-{}", draft, hex::encode(Sha256::digest(encoded))))
}

#[cfg(test)]
mod tests {
    use erp_core::ids::PurchaseLineSalesAllocationId;
    use erp_fulfillment::entity::fulfillment::{DeliveryLineData, DeliveryType};

    use super::*;

    fn line(source: &str, sale: &str, quantity: &str) -> DeliveryLine {
        DeliveryLine::new(
            DeliveryLineId::new(format!("{source}-delivery")),
            DeliveryLineData {
                delivery_id: DeliveryId::new("draft"),
                line_no: 1,
                sales_order_line_id: SalesOrderLineId::new(sale),
                quantity: quantity.parse().unwrap(),
                stock_reservation_id: None,
                purchase_line_sales_allocation_id: Some(PurchaseLineSalesAllocationId::new(format!(
                    "{source}-allocation"
                ))),
            },
            DeliveryType::SupplierDirect,
        )
        .unwrap()
    }
    fn source(id: &str, sale: &str) -> TrackingSourceLine {
        TrackingSourceLine {
            fulfillment_item_id: id.into(),
            purchase_order_revision_line_id: format!("{id}-revision"),
            sales_order_line_id: Some(sale.into()),
            delivery_id: Some("draft".into()),
            quantity: "3".parse().unwrap(),
        }
    }
    #[test]
    fn partial_same_sku_allocations_keep_their_own_sales_lines_and_remainder() {
        let original = vec![line("first", "sale-one", "3"), line("second", "sale-two", "2")];
        let source = source("second", "sale-two");
        let (shipped, remaining) =
            split_lines(&original, &[(&source, "1".parse().unwrap())], "part").unwrap();
        assert_eq!(shipped.len(), 1);
        assert_eq!(shipped[0].sales_order_line_id.as_ref(), "sale-two");
        assert_eq!(remaining[0], Some(original[0].clone()));
        assert_eq!(remaining[1].as_ref().unwrap().quantity, "1".parse().unwrap());
        assert_eq!(
            shipped[0].purchase_line_sales_allocation_id,
            original[1].purchase_line_sales_allocation_id
        );
    }
    #[test]
    fn missing_source_over_dispatch_and_cross_sales_mapping_stop_before_writes() {
        let original = vec![line("first", "sale-one", "3")];
        assert!(
            split_lines(&original, &[(&source("other", "sale-one"), "1".parse().unwrap())], "part").is_err()
        );
        assert!(
            split_lines(&original, &[(&source("first", "sale-one"), "4".parse().unwrap())], "part").is_err()
        );
        assert!(
            split_lines(&original, &[(&source("first", "sale-two"), "1".parse().unwrap())], "part").is_err()
        );
    }

    struct RecordingApplication {
        calls: std::sync::Mutex<Vec<String>>,
        tracking_calls: std::sync::atomic::AtomicUsize,
        fail: Option<String>,
    }
    #[async_trait]
    impl QuantityDeliveryApplication for RecordingApplication {
        async fn merge_tracking(
            &self,
            _: &OrderTracking,
            _: &OrderObservation,
            _: &mut dyn Executor,
        ) -> Result<()> {
            self.tracking_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(())
        }
        async fn dispatch(
            &self,
            _: &OrderTracking,
            shipment: &PendingShipment,
            _: &mut dyn Executor,
        ) -> Result<Vec<String>> {
            self.calls.lock().unwrap().push(shipment.shipment_id.clone());
            if self.fail.as_ref() == Some(&shipment.shipment_id) {
                return Err(Error::BusinessLogicError("test posting failed".into()));
            }
            Ok(vec![format!("delivery-{}", shipment.shipment_id)])
        }
    }
    fn quantity_observation(tracking: &OrderTracking, first: &str, second: &str) -> OrderObservation {
        use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp};
        use erp_supply::ports::connector::order::{OrderReference, OrderSnapshot, Shipment, ShipmentLine};
        OrderObservation::from_snapshot(
            "observation".into(),
            tracking,
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::now(),
                },
                value: OrderSnapshot {
                    source_evidence: None,
                    reference: OrderReference {
                        merchant_order_no: "merchant".into(),
                        external_order_no: "root".into(),
                    },
                    amounts: None,
                    fulfillment: Some(FulfillmentStatus::Completed),
                    cancellation: None,
                    refund: None,
                    payment: None,
                    topology: None,
                    confirmation: None,
                    complete_line_coverage: false,
                    shipments: Some(
                        [("first", first), ("second", second)]
                            .into_iter()
                            .map(|(id, quantity)| Shipment {
                                external_shipment_id: Some(id.into()),
                                source_order_no: Some("root".into()),
                                line_ids: vec!["first".into()],
                                quantity_lines: Some(vec![ShipmentLine {
                                    line_id: "first".into(),
                                    quantity: quantity.parse().unwrap(),
                                }]),
                                complete_line_coverage: false,
                                carrier: None,
                                tracking_no: Some(id.into()),
                                courier_phone: None,
                                events: vec![],
                            })
                            .collect(),
                    ),
                },
            },
        )
        .unwrap()
    }
    fn tracked() -> OrderTracking {
        OrderTracking::new(
            "order".into(),
            "connection".into(),
            "purchase".into(),
            vec![source("first", "sale-one")],
        )
        .unwrap()
    }
    #[tokio::test]
    async fn real_quantity_application_posts_only_new_packages_and_records_after_success() {
        let mut tracking = tracked();
        let observation = quantity_observation(&tracking, "1", "2");
        let application = RecordingApplication {
            calls: std::sync::Mutex::new(vec![]),
            tracking_calls: Default::default(),
            fail: None,
        };

        assert!(
            apply_quantity_observation(
                &application,
                &mut tracking,
                &observation,
                &mut persistence_core::NoTransaction
            )
            .await
            .unwrap()
        );
        assert_eq!(*application.calls.lock().unwrap(), ["first", "second"]);
        assert_eq!(tracking.applied_shipments.len(), 2);

        apply_quantity_observation(
            &application,
            &mut tracking,
            &observation,
            &mut persistence_core::NoTransaction,
        )
        .await
        .unwrap();
        assert_eq!(application.calls.lock().unwrap().len(), 2);
        assert_eq!(application.tracking_calls.load(std::sync::atomic::Ordering::SeqCst), 2);
    }
    #[tokio::test]
    async fn invalid_total_prevents_all_writes_and_posting_failure_stops_without_receipt() {
        let mut tracking = tracked();
        let application = RecordingApplication {
            calls: std::sync::Mutex::new(vec![]),
            tracking_calls: Default::default(),
            fail: Some("first".into()),
        };

        let over = quantity_observation(&tracking, "2", "2");
        assert!(
            apply_quantity_observation(
                &application,
                &mut tracking,
                &over,
                &mut persistence_core::NoTransaction
            )
            .await
            .is_err()
        );
        assert!(application.calls.lock().unwrap().is_empty());

        let valid = quantity_observation(&tracking, "1", "2");
        assert!(
            apply_quantity_observation(
                &application,
                &mut tracking,
                &valid,
                &mut persistence_core::NoTransaction
            )
            .await
            .is_err()
        );
        assert_eq!(*application.calls.lock().unwrap(), ["first"]);
        assert!(tracking.applied_shipments.is_empty());
    }

    #[tokio::test]
    async fn source_completion_without_quantities_reports_review_and_never_dispatches() {
        let mut tracking = tracked();
        let mut observation = quantity_observation(&tracking, "1", "2");
        for shipment in observation.shipments.as_mut().unwrap() {
            shipment.quantity_lines = None;
            shipment.line_ids.clear();
        }
        let application = RecordingApplication {
            calls: std::sync::Mutex::new(vec![]),
            tracking_calls: Default::default(),
            fail: None,
        };

        for _ in 0..2 {
            let error = apply_quantity_observation(
                &application,
                &mut tracking,
                &observation,
                &mut persistence_core::NoTransaction,
            )
            .await
            .unwrap_err();
            assert!(error.to_string().contains("发货数量待核实"));
        }
        assert!(application.calls.lock().unwrap().is_empty());
        assert!(tracking.applied_shipments.is_empty());
    }

    #[tokio::test]
    async fn proven_partial_package_is_applied_once_while_unknown_package_stays_unapplied() {
        let mut tracking = tracked();
        let mut observation = quantity_observation(&tracking, "1", "2");
        observation.shipments.as_mut().unwrap()[1].quantity_lines = None;
        let application = RecordingApplication {
            calls: std::sync::Mutex::new(vec![]),
            tracking_calls: Default::default(),
            fail: None,
        };

        assert!(
            !apply_quantity_observation(
                &application,
                &mut tracking,
                &observation,
                &mut persistence_core::NoTransaction,
            )
            .await
            .unwrap()
        );
        assert_eq!(*application.calls.lock().unwrap(), ["first"]);

        assert!(
            apply_quantity_observation(
                &application,
                &mut tracking,
                &observation,
                &mut persistence_core::NoTransaction,
            )
            .await
            .is_err()
        );

        let complete = quantity_observation(&tracking, "1", "2");
        assert!(
            apply_quantity_observation(
                &application,
                &mut tracking,
                &complete,
                &mut persistence_core::NoTransaction,
            )
            .await
            .unwrap()
        );
        assert_eq!(*application.calls.lock().unwrap(), ["first", "second"]);
    }
}
