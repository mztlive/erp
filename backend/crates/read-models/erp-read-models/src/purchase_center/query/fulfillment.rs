//! 已授权原采购的接口摘要和ERP实物履约数量，供应商状态不参与数量计算。

use std::collections::{HashMap, HashSet};

use erp_core::ids::PurchaseOrderId;
use erp_core::money::Quantity;
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::{
    FulfillmentResponsibility, PurchaseLineType, PurchaseOrder, PurchaseOrderRevisionLine, PurchaseType,
    purchase_line_stable_key, rollup_by_stable_key,
};
use erp_supply::api_order::{connections_by_ids, for_purchase, fulfillment_orders_by_execution_ids};
use mongodb::Database;
use persistence_core::NoTransaction;
use rust_decimal::Decimal;

use crate::purchase_center::change_bounds::{all_revision_lines, shipped_rows};
use crate::purchase_center::dto::{PurchaseApiExecutionView, PurchaseFulfillmentQuantityView};
use crate::{Error, Result};

type FulfillmentViews = (Vec<PurchaseApiExecutionView>, Vec<PurchaseFulfillmentQuantityView>);

pub(super) async fn fulfillment_views(db: &Database, order: &PurchaseOrder) -> Result<FulfillmentViews> {
    let Some(current_revision) = order.stable.current_revision_id.as_deref() else {
        return Ok((vec![], vec![]));
    };
    let lines = all_revision_lines(db, &PurchaseOrderId::new(&order.base.id), &mut NoTransaction).await?;
    let current = lines
        .iter()
        .filter(|line| line.purchase_order_revision_id.as_ref() == current_revision)
        .cloned()
        .collect::<Vec<_>>();
    let api = execution_views(db, order, &current).await?;
    if order.purchase_type != PurchaseType::Physical {
        return Ok((api, vec![]));
    }
    let inbound = db
        .fulfillment()
        .qualified_received_totals_by_purchase_revision_line(
            &PurchaseOrderId::new(&order.base.id),
            &mut NoTransaction,
        )
        .await?;
    let known = lines.iter().map(|line| line.base.id.as_str()).collect::<HashSet<_>>();
    if inbound.keys().any(|id| !known.contains(id.as_ref())) {
        return Err(Error::BusinessLogicError("已过账收货存在未定位的采购版本行，请核对原记录".into()));
    }
    let inbound = rollup_by_stable_key(&lines, &inbound).map_err(Error::Logic)?;
    let mut shipped = HashMap::<String, Decimal>::new();
    for (line, quantity) in shipped_rows(db, order, &mut NoTransaction).await? {
        let total = shipped.entry(line).or_default();
        *total = total
            .checked_add(quantity.to_decimal())
            .ok_or_else(|| Error::BusinessLogicError("累计发货数量超出范围".into()))?;
    }
    let shipped = shipped
        .into_iter()
        .map(|(line, value)| Quantity::try_from(value).map(|quantity| (line, quantity)).map_err(Error::Logic))
        .collect::<Result<HashMap<_, _>>>()?;
    let quantities = quantity_views(&current, order.fulfillment_responsibility, &inbound, &shipped)?;
    Ok((api, quantities))
}

async fn execution_views(
    db: &Database,
    order: &PurchaseOrder,
    current: &[PurchaseOrderRevisionLine],
) -> Result<Vec<PurchaseApiExecutionView>> {
    let executions =
        for_purchase(db, &order.base.id, order.stable.current_revision_id.as_deref(), &mut NoTransaction)
            .await?;
    if executions.is_empty() {
        return Ok(vec![]);
    }
    let root_ids = executions.iter().map(|execution| execution.base.id.clone()).collect::<Vec<_>>();
    let connections =
        executions.iter().map(|execution| execution.connection_id.clone()).collect::<HashSet<_>>();
    let roots = fulfillment_orders_by_execution_ids(db, &root_ids, &mut NoTransaction)
        .await?
        .into_iter()
        .map(|root| (root.base.id.clone(), root))
        .collect::<HashMap<_, _>>();
    let connections =
        connections_by_ids(db, &connections.into_iter().collect::<Vec<_>>(), &mut NoTransaction)
            .await?
            .into_iter()
            .map(|connection| (connection.base.id.clone(), connection))
            .collect::<HashMap<_, _>>();
    let mut views = Vec::with_capacity(executions.len());
    for execution in executions {
        let connection = connections
            .get(&execution.connection_id)
            .ok_or_else(|| Error::NotFound("采购接口执行缺少原连接".into()))?;
        views.push(PurchaseApiExecutionView::from_purchase_execution(
            &execution,
            order,
            current,
            connection,
            roots.get(&execution.base.id),
        )?);
    }
    Ok(views)
}

fn quantity_views(
    current: &[PurchaseOrderRevisionLine],
    responsibility: FulfillmentResponsibility,
    inbound: &HashMap<String, Quantity>,
    shipped: &HashMap<String, Quantity>,
) -> Result<Vec<PurchaseFulfillmentQuantityView>> {
    let mut occurrences = HashMap::<String, usize>::new();
    for line in current.iter().filter(|line| line.line_type == PurchaseLineType::ItemService) {
        *occurrences.entry(purchase_line_stable_key(line)).or_default() += 1;
    }
    let zero = Quantity::try_from(Decimal::ZERO).map_err(Error::Logic)?;
    current
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .map(|line| {
            let key = purchase_line_stable_key(line);
            let ambiguous = occurrences.get(&key).is_some_and(|count| *count > 1)
                || (responsibility == FulfillmentResponsibility::SupplierDirect
                    && line.sales_order_line_id.is_none());
            let inbound = (responsibility == FulfillmentResponsibility::Warehouse && !ambiguous)
                .then(|| inbound.get(&key).copied().unwrap_or(zero));
            let shipped = (responsibility == FulfillmentResponsibility::SupplierDirect && !ambiguous)
                .then(|| shipped.get(&key).copied().unwrap_or(zero));
            PurchaseFulfillmentQuantityView::from_applied_facts(
                line,
                responsibility,
                inbound,
                shipped,
                ambiguous,
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::{
        PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, SalesOrderId, SalesOrderLineId,
        SalesOrderRevisionId, SupplierAccountId, SupplierApiConnectionId, SupplierOfferingId,
        SupplierOfferingRevisionId,
    };
    use erp_core::money::Amount;
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{PurchaseOfferingSource, PurchaseOrderData};
    use erp_supply::api_order::{ApiOrderExecution, ApiOrderSourceLine, ApiOrderStage};
    use erp_supply::entity::supplier_api::{
        SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
    };
    use erp_supply::entity::supplier_fulfillment::{
        CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentOrder,
    };

    use super::*;

    fn qty(value: &str) -> Quantity {
        value.parse().unwrap()
    }

    fn line(id: &str, stable: &str, quantity: &str) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine {
            base: BaseModel { id: id.into(), ..BaseModel::fake() },
            purchase_order_revision_id: PurchaseOrderRevisionId::new("revision"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            supplier_offering_source: None,
            delivery_allocations: vec![],
            product_name_snapshot: Some("同一商品".into()),
            specification_snapshot: None,
            quantity: Some(qty(quantity)),
            base_unit_code: Some("件".into()),
            unit_cost_gross: None,
            gross_amount: Amount::zero(),
            net_amount: Amount::zero(),
            tax_amount: Amount::zero(),
            input_tax_rate: None,
            expected_delivery_date: None,
            sales_order_line_id: Some(SalesOrderLineId::new(stable)),
            sales_order_revision_line_id: None,
            allocated_quantity: Some(qty(quantity)),
            expense: None,
        }
    }

    fn purchase() -> PurchaseOrder {
        let mut purchase = PurchaseOrder::new(
            PurchaseOrderId::new("purchase"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: "PO-001".into(),
                sales_order_id: SalesOrderId::new("sale"),
                sales_order_revision_id: SalesOrderRevisionId::new("sale-revision"),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("supplier"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::SupplierDirect,
                owner_user_id: "buyer".into(),
                target_warehouse_id: None,
            },
            "buyer",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        purchase.stable.current_revision_id = Some("revision".into());
        purchase
    }

    fn connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier"),
                connection_code: "api".into(),
                connector_type: SupplierConnectorType::Dangaoshushu,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "buyer",
        )
        .unwrap()
    }

    fn execution() -> ApiOrderExecution {
        ApiOrderExecution::queued(
            "execution".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![ApiOrderSourceLine {
                fulfillment_item_id: "item".into(),
                purchase_order_revision_line_id: "line".into(),
                sales_order_line_id: Some("sale-line".into()),
                supplier_offering_revision_id: "offering-revision".into(),
                quantity: qty("3"),
                delivery_allocation_index: 0,
            }],
        )
    }

    fn root() -> SupplierFulfillmentOrder {
        SupplierFulfillmentOrder {
            base: BaseModel { id: "execution".into(), ..BaseModel::fake() },
            fulfillment_order_no: "履约-001".into(),
            supplier_id: SupplierAccountId::new("supplier"),
            connection_id: SupplierApiConnectionId::new("connection"),
            split_no: 1,
            fulfillment_status: FulfillmentStatus::Received,
            cancel_status: CancelStatus::None,
            refund_status: RefundStatus::None,
            external_order_no: None,
            submitted_at: None,
            accepted_at: None,
            completed_at: None,
            address_snapshot_encrypted: "encrypted".into(),
            address_snapshot_fingerprint: "fingerprint".into(),
            follow_up_user_id: "buyer".into(),
            business_org_unit_id: "org".into(),
        }
    }

    fn execution_view(
        execution: &ApiOrderExecution,
        root: Option<&SupplierFulfillmentOrder>,
    ) -> Result<PurchaseApiExecutionView> {
        PurchaseApiExecutionView::from_purchase_execution(
            execution,
            &purchase(),
            &[line("line", "sale-line", "3")],
            &connection(),
            root,
        )
    }

    #[test]
    fn queued_execution_without_initialized_order_preserves_purchase_summary() {
        let mut execution = execution();
        execution.blocker = Some("后台初始化暂时失败，请稍后重试".into());
        let view = execution_view(&execution, None).unwrap();
        assert_eq!(view.stage, ApiOrderStage::Queued);
        assert!(view.order_no.is_empty());
        assert!(!view.detail_available);
        assert_eq!(view.current_order_count, 0);
        assert_eq!(view.confirmed_order_count, 0);
        assert!(view.blocker.is_some());
    }

    #[test]
    fn initialized_order_keeps_business_number_and_detail_while_setup_is_pending() {
        let mut execution = execution();
        execution.stage = ApiOrderStage::AwaitingSetup;
        let view = execution_view(&execution, Some(&root())).unwrap();
        assert_eq!(view.order_no, "履约-001");
        assert!(view.detail_available);
        assert_eq!(view.stage, ApiOrderStage::AwaitingSetup);
        assert_eq!(view.blocker.as_deref(), Some("请补齐本采购的履约资料"));
    }

    #[test]
    fn missing_order_after_initialization_is_not_hidden_as_queued() {
        for stage in [
            ApiOrderStage::AwaitingSetup,
            ApiOrderStage::PrepareAddress,
            ApiOrderStage::AddressUnknown,
            ApiOrderStage::PlanDelivery,
            ApiOrderStage::Create,
            ApiOrderStage::CreationUnknown,
            ApiOrderStage::DiscoverOrders,
            ApiOrderStage::ConfirmOrder,
            ApiOrderStage::ConfirmationUnknown,
            ApiOrderStage::CancelOrders,
            ApiOrderStage::CancellationUnknown,
            ApiOrderStage::ConfirmPayment,
            ApiOrderStage::PaymentUnknown,
            ApiOrderStage::Observe,
            ApiOrderStage::Stopped,
        ] {
            let mut execution = execution();
            execution.stage = stage;
            assert!(matches!(execution_view(&execution, None), Err(Error::NotFound(_))));
        }
        for evidence in ["request", "address", "create", "setup", "cancel", "superseded"] {
            let mut execution = execution();
            match evidence {
                "request" => execution.request_encrypted = Some("encrypted".into()),
                "address" => {
                    execution.action_hashes.insert("address".into(), "hash".into());
                },
                "create" => {
                    execution.action_hashes.insert("create".into(), "hash".into());
                },
                "setup" => execution.setup_idempotency_key = Some("setup".into()),
                "cancel" => execution.cancellation_action_id = Some("cancel".into()),
                _ => execution.superseded_by_purchase_revision_id = Some("new-revision".into()),
            }
            assert!(matches!(execution_view(&execution, None), Err(Error::NotFound(_))));
        }
    }

    #[test]
    fn queued_summary_still_rejects_foreign_purchase_supplier_connection_and_revision() {
        for mismatch in ["purchase", "supplier", "connection", "revision", "line", "empty"] {
            let mut execution = execution();
            match mismatch {
                "purchase" => execution.purchase_order_id = "foreign".into(),
                "supplier" => execution.supplier_id = "foreign".into(),
                "connection" => execution.connection_id = "foreign".into(),
                "revision" => execution.purchase_order_revision_id = "foreign".into(),
                "line" => execution.source_lines[0].purchase_order_revision_line_id = "foreign".into(),
                _ => execution.source_lines.clear(),
            }
            assert!(matches!(execution_view(&execution, None), Err(Error::Forbidden(_))));
        }
        for mismatch in ["order", "connection", "supplier"] {
            let mut root = root();
            match mismatch {
                "order" => root.base.id = "foreign".into(),
                "connection" => root.connection_id = SupplierApiConnectionId::new("foreign"),
                _ => root.supplier_id = SupplierAccountId::new("foreign"),
            }
            assert!(matches!(execution_view(&execution(), Some(&root)), Err(Error::Forbidden(_))));
        }
    }

    #[test]
    fn partial_shipping_keeps_same_sku_sales_lines_and_manual_api_quantities_separate() {
        let manual = line("manual", "sales-a", "5");
        let mut api = line("api", "sales-b", "7");
        api.supplier_offering_source = Some(PurchaseOfferingSource {
            supplier_offering_id: SupplierOfferingId::new("api-offering"),
            offering_version: 1,
            supplier_offering_revision_id: SupplierOfferingRevisionId::new("api-revision"),
            revision_version: 1,
            availability_version: 1,
            inquiry_quote: None,
        });
        let mut expense = line("fee", "fee", "1");
        expense.line_type = PurchaseLineType::LogisticsFee;
        let views = quantity_views(
            &[manual, api, expense],
            FulfillmentResponsibility::SupplierDirect,
            &HashMap::new(),
            &HashMap::from([("sales-a".into(), qty("2")), ("sales-b".into(), qty("4"))]),
        )
        .unwrap();
        assert_eq!(views.len(), 2);
        assert_eq!(views[0].line_id, "manual");
        assert_eq!(views[0].shipped_quantity, Some(qty("2")));
        assert_eq!(views[0].remaining_quantity, Some(qty("3")));
        assert_eq!(views[1].line_id, "api");
        assert_eq!(views[1].shipped_quantity, Some(qty("4")));
        assert_eq!(views[1].remaining_quantity, Some(qty("3")));
        assert!(views.iter().all(|view| view.inbound_quantity.is_none() && view.blocker.is_none()));
    }

    #[test]
    fn receipt_across_purchase_revisions_is_not_customer_shipping() {
        let old = line("old-line", "sales-a", "5");
        let current = line("new-line", "sales-a", "5");
        let receipts = HashMap::from([(PurchaseOrderRevisionLineId::new("old-line"), qty("3"))]);
        let inbound = rollup_by_stable_key(&[old, current.clone()], &receipts).unwrap();
        let views = quantity_views(
            &[current],
            FulfillmentResponsibility::Warehouse,
            &inbound,
            &HashMap::from([("sales-a".into(), qty("9"))]),
        )
        .unwrap();
        assert_eq!(views[0].inbound_quantity, Some(qty("3")));
        assert_eq!(views[0].shipped_quantity, None);
        assert_eq!(views[0].remaining_quantity, Some(qty("2")));
    }

    #[test]
    fn ambiguous_or_missing_facts_never_become_zero_or_completed() {
        let rows = [line("one", "same-sales", "3"), line("two", "same-sales", "4")];
        let views = quantity_views(
            &rows,
            FulfillmentResponsibility::SupplierDirect,
            &HashMap::new(),
            &HashMap::from([("same-sales".into(), qty("2"))]),
        )
        .unwrap();
        assert!(views.iter().all(|view| view.shipped_quantity.is_none()
            && view.remaining_quantity.is_none()
            && view.blocker.is_some()));
        let mut missing = line("missing", "sales", "3");
        missing.quantity = None;
        let view = quantity_views(
            &[missing],
            FulfillmentResponsibility::Warehouse,
            &HashMap::new(),
            &HashMap::new(),
        )
        .unwrap();
        assert!(view[0].ordered_quantity.is_none());
        assert!(view[0].remaining_quantity.is_none());
        assert!(view[0].blocker.is_some());
    }

    #[test]
    fn overfulfillment_preserves_actual_quantity_and_requires_review() {
        let views = quantity_views(
            &[line("one", "sales", "3")],
            FulfillmentResponsibility::SupplierDirect,
            &HashMap::new(),
            &HashMap::from([("sales".into(), qty("4"))]),
        )
        .unwrap();
        assert_eq!(views[0].shipped_quantity, Some(qty("4")));
        assert!(views[0].remaining_quantity.is_none());
        assert!(views[0].blocker.is_some());
    }
}
