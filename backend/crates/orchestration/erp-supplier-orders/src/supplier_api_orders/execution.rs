use erp_core::common::time::Instant;
use erp_core::ids::{
    SupplierAccountId, SupplierApiConnectionId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId,
};
use erp_fulfillment_execution::fulfillment_execution::ensure_po_fulfillable;
use erp_procurement::entity::purchase_order::{FulfillmentResponsibility, PurchaseOrder};
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderStage, reusable_address};
use erp_supply::entity::supplier_fulfillment::{
    CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentItem, SupplierFulfillmentItemData,
    SupplierFulfillmentOrder, SupplierFulfillmentOrderData,
};
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::order_tracking::{OrderTracking, TrackingSourceLine};
use erp_supply::repository::SupplierFulfillmentExt;
use persistence_core::{NoTransaction, Transactional};

use super::SupplierApiOrderProcess;
use super::snapshot::ExecutionSnapshot;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn execute(&self, mut execution: ApiOrderExecution) -> Result<()> {
        if execution.stage == ApiOrderStage::Queued {
            self.initialize(&mut execution).await?;
        }

        // 调查可能已送达的写请求不受新业务启停、采购修订或预付款变化阻断。
        match execution.stage {
            ApiOrderStage::CancelOrders | ApiOrderStage::CancellationUnknown => {
                return self.process_cancellations(execution).await;
            },
            ApiOrderStage::AddressUnknown => return self.recover_address(execution).await,
            ApiOrderStage::CreationUnknown => return self.recover_creation(execution).await,
            ApiOrderStage::PaymentUnknown => return self.recover_payment(execution).await,
            ApiOrderStage::ConfirmationUnknown => return self.recover_confirmation(execution).await,
            ApiOrderStage::DiscoverOrders => return self.discover_orders(execution).await,
            _ => {},
        }

        if let Err(error) = self.ensure_eligible(&execution).await {
            execution.defer(error.to_string());
            return self.persist(&mut execution).await;
        }

        match execution.stage {
            ApiOrderStage::PrepareAddress => self.prepare_address(execution).await,
            ApiOrderStage::PlanDelivery => self.plan_delivery(execution).await,
            ApiOrderStage::Create => self.create(execution).await,
            ApiOrderStage::CreationUnknown => self.recover_creation(execution).await,
            ApiOrderStage::ConfirmPayment => self.confirm_payment(execution).await,
            ApiOrderStage::ConfirmOrder => self.confirm_order(execution).await,
            ApiOrderStage::PaymentUnknown => self.recover_payment(execution).await,
            _ => {
                execution.next_run_at =
                    Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(60));
                self.persist(&mut execution).await
            },
        }
    }

    pub(super) async fn ensure_eligible(&self, execution: &ApiOrderExecution) -> Result<PurchaseOrder> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&execution.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;
        ensure_po_fulfillable(&order)?;
        if order.stable.current_revision_id.as_deref() != Some(execution.purchase_order_revision_id.as_str())
        {
            return Err(Error::BusinessLogicError(
                "采购生效版本已变化，请先核对已有供应商订单；系统不会按新版本重复下单".into(),
            ));
        }

        self.ensure_preflight_funds(execution, &order, &mut NoTransaction).await?;
        Ok(order)
    }

    async fn initialize(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(&execution.purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".into()))?;

        let snapshot = self.build_snapshot(execution).await;
        match snapshot {
            Ok(mut snapshot) => {
                let fingerprint = self
                    .codec
                    .address_fingerprint(&format!(
                        "{}\0{}",
                        snapshot.recipient.region, snapshot.recipient.address
                    ))
                    .as_str()
                    .to_owned();
                self.reuse_geocode(execution, &mut snapshot, &fingerprint).await?;
                execution.address_fingerprint = Some(fingerprint);

                let ready = self.address_ready(execution, &snapshot).await;
                execution.stage =
                    if ready.is_ok() { ApiOrderStage::PrepareAddress } else { ApiOrderStage::AwaitingSetup };
                execution.blocker = ready.err().map(|error| error.to_string());
                self.encode(execution, &snapshot)?;
            },
            Err(error) => {
                execution.stage = ApiOrderStage::AwaitingSetup;
                execution.blocker = Some(error.to_string());
            },
        }

        self.persist_initial_order(execution, &order).await
    }

    async fn address_ready(&self, execution: &ApiOrderExecution, snapshot: &ExecutionSnapshot) -> Result<()> {
        use erp_supply::service::supplier_api::SupplierApiService;
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        snapshot.recipient.ensure_address_ready(
            connector.address_requirement(),
            &execution.connection_id,
            &execution.base.id,
            execution.address_fingerprint.as_deref().unwrap_or_default(),
        )?;
        Ok(())
    }

    async fn reuse_geocode(
        &self,
        execution: &ApiOrderExecution,
        snapshot: &mut ExecutionSnapshot,
        fingerprint: &str,
    ) -> Result<()> {
        for candidate in
            reusable_address(&self.db, &execution.connection_id, fingerprint, &mut NoTransaction).await?
        {
            if let Ok(old) = self.decode(&candidate)
                && old.recipient.geocoded_address.is_some()
            {
                snapshot.recipient.geocoded_address = old.recipient.geocoded_address;
                if snapshot.recipient.phone.is_empty()
                    && execution.warehouse_revision_id.is_some()
                    && candidate.warehouse_revision_id == execution.warehouse_revision_id
                    && snapshot.recipient.name == old.recipient.name
                {
                    snapshot.recipient.phone = old.recipient.phone;
                }
                break;
            }
        }
        Ok(())
    }

    async fn persist_initial_order(
        &self,
        execution: &mut ApiOrderExecution,
        purchase: &PurchaseOrder,
    ) -> Result<()> {
        let order = self.initial_order(execution, purchase)?;
        let items = self.initial_items(execution).await?;
        let tracking = tracking_for(execution, purchase)?;

        let db = self.db.clone();
        let client = db.client().clone();
        let mut saved = execution.clone();
        let result = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.supplier_fulfillment_orders().create(&order, executor).await?;
                    for item in &items {
                        db.supplier_fulfillment_items().create(item, executor).await?;
                    }
                    db.supplier_order_tracking().create(&tracking, executor).await?;
                    db.supplier_api_order_executions().update(&mut saved, executor).await?;
                    Ok::<_, Error>(saved)
                })
            })
            .await?;
        *execution = result;
        Ok(())
    }

    fn initial_order(
        &self,
        execution: &ApiOrderExecution,
        purchase: &PurchaseOrder,
    ) -> Result<SupplierFulfillmentOrder> {
        let data = SupplierFulfillmentOrderData {
            fulfillment_order_no: execution.base.id.clone(),
            supplier_id: SupplierAccountId::new(&execution.supplier_id),
            connection_id: SupplierApiConnectionId::new(&execution.connection_id),
            split_no: 1,
            fulfillment_status: FulfillmentStatus::Received,
            cancel_status: CancelStatus::None,
            refund_status: RefundStatus::None,
            external_order_no: None,
            submitted_at: None,
            accepted_at: None,
            completed_at: None,
            address_snapshot_encrypted: execution
                .request_encrypted
                .clone()
                .unwrap_or(self.codec.encrypt("null")?),
            address_snapshot_fingerprint: execution
                .address_fingerprint
                .clone()
                .unwrap_or_else(|| self.codec.address_fingerprint(&execution.base.id).as_str().to_owned()),
            follow_up_user_id: purchase
                .owner_user_id
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("来源采购缺少责任人".into()))?,
            business_org_unit_id: purchase.business_org_unit_id.clone(),
        };
        SupplierFulfillmentOrder::new(SupplierFulfillmentOrderId::new(&execution.base.id), data)
            .map_err(Into::into)
    }

    async fn initial_items(&self, execution: &ApiOrderExecution) -> Result<Vec<SupplierFulfillmentItem>> {
        let mut items = Vec::new();
        for source in &execution.source_lines {
            let line = self
                .db
                .purchase_order_revision_lines()
                .find_by_id(&source.purchase_order_revision_line_id, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("采购冻结行不存在".into()))?;
            let frozen = line
                .supplier_offering_source
                .as_ref()
                .ok_or_else(|| Error::BusinessLogicError("采购未冻结供给".into()))?;
            use erp_supply::repository::SupplierOfferingExt;
            let offering = self
                .db
                .supplier_offerings()
                .find_by_id(frozen.supplier_offering_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("冻结供给不存在".into()))?;

            let data = SupplierFulfillmentItemData::from_unit_cost(
                SupplierFulfillmentOrderId::new(&execution.base.id),
                source.supplier_offering_revision_id.clone().into(),
                offering.supplier_sku_code,
                offering.supplier_product_code,
                source.quantity,
                line.unit_cost_gross.ok_or_else(|| Error::BusinessLogicError("采购成本缺失".into()))?,
                line.input_tax_rate.ok_or_else(|| Error::BusinessLogicError("采购税率缺失".into()))?,
            )?;
            items.push(SupplierFulfillmentItem::new(
                SupplierFulfillmentItemId::new(&source.fulfillment_item_id),
                data,
            )?);
        }
        Ok(items)
    }
}

fn tracking_for(execution: &ApiOrderExecution, purchase: &PurchaseOrder) -> Result<OrderTracking> {
    let direct = purchase.fulfillment_responsibility == FulfillmentResponsibility::SupplierDirect;
    let lines = execution
        .source_lines
        .iter()
        .map(|line| TrackingSourceLine {
            fulfillment_item_id: line.fulfillment_item_id.clone(),
            purchase_order_revision_line_id: line.purchase_order_revision_line_id.clone(),
            sales_order_line_id: line.sales_order_line_id.clone(),
            quantity: line.quantity,
            delivery_id: direct.then(|| format!("{}-delivery", execution.base.id)),
        })
        .collect();
    Ok(OrderTracking::new(
        execution.base.id.clone(),
        execution.connection_id.clone(),
        execution.purchase_order_id.clone(),
        lines,
    )?)
}
