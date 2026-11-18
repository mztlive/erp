//! 已收到商品后的独立通知：ERP事实先提交，发送意图先保存，未知仅查询原目标。

mod direct;
mod dispatch;
mod evidence;
mod facts;
mod store;
mod warehouse;

use erp_core::common::time::Instant;
use erp_supply::api_order::{ApiOrderExecution, ReceiptActionState, receipts_due};
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::order::OrderReference;
use erp_supply::ports::connector::receipt::{OrderReceiptConfirmation, ReceiptConfirmationState};
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::NoTransaction;

use super::SupplierApiOrderProcess;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn run_receipts_once(&self) -> Result<usize> {
        let rows = receipts_due(&self.db, &mut NoTransaction).await?;
        let count = rows.len();
        for row in rows {
            let id = row.base.id.clone();
            if let Err(error) = self.receipt_confirmation(row).await {
                tracing::warn!(execution_id = %id, error = %error, "供应商收货通知需按原记录核验");
            }
        }
        Ok(count)
    }

    async fn receipt_confirmation(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        let connector = self
            .runtime
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let Some(port) = connector.receipt_confirmation() else {
            execution.receipt_next_run_at = (!execution.receipt_actions.is_empty()).then(later);
            return self.save_receipt_progress(&mut execution).await;
        };
        let source_error =
            self.receipt_source_evidence(&execution).await.err().map(|error| error.to_string());
        if let Some(action) = execution
            .receipt_actions
            .values()
            .find(|action| action.state == ReceiptActionState::SentUnknown)
            .cloned()
        {
            return dispatch::recover(
                self,
                &mut execution,
                &action.order,
                action.topology_version,
                port,
                source_error,
            )
            .await;
        }
        if let Some(error) = source_error {
            execution.receipt_blocker = Some(if execution.receipt_actions.is_empty() {
                error
            } else {
                format!("原收货通知已登记，但ERP实收依据已变更，请核对原签收或入库冲正：{error}")
            });
            execution.receipt_next_run_at = (!execution.receipt_actions.is_empty()).then(later);
            return self.save_receipt_progress(&mut execution).await;
        }
        let Some((order, version)) = execution.next_receipt_target()? else {
            // 已通知后的净实收仍低频重核，仓库历史冲正或退货不能隐藏来源冲突。
            execution.receipt_next_run_at =
                Some(Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(300)));
            execution.receipt_blocker = None;
            return self.save_receipt_progress(&mut execution).await;
        };
        self.first_receipt(execution, &order, version, port).await
    }

    async fn first_receipt(
        &self,
        mut execution: ApiOrderExecution,
        order: &OrderReference,
        version: u64,
        port: &dyn OrderReceiptConfirmation,
    ) -> Result<()> {
        match port.receipt_confirmation(order).await {
            Ok(Lookup::Found(snapshot)) if snapshot.value != ReceiptConfirmationState::Unverified => {
                execution.observe_receipt_confirmation(order, version, snapshot.value)?;
                execution.receipt_next_run_at = Some(Instant::now());
                execution.receipt_blocker = None;
                self.save_receipt_progress(&mut execution).await
            },
            Ok(Lookup::Found(_)) => dispatch::send(self, &execution, order, port).await,
            result => {
                execution.receipt_blocker = Some(match result {
                    Err(error) => error.summary,
                    _ => "原子单暂不可见，不能据此发送收货通知".into(),
                });
                execution.receipt_next_run_at = Some(later());
                self.save_receipt_progress(&mut execution).await
            },
        }
    }
}

fn later() -> Instant {
    Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(60))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use entity_core::BaseModel;
    use erp_core::ids::{
        PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseReceiptId, WarehouseId,
    };
    use erp_core::money::{Amount, Quantity};
    use erp_fulfillment::entity::fulfillment::{
        PurchaseReceipt, PurchaseReceiptLine, PurchaseReceiptState, QualityResult,
    };
    use erp_procurement::entity::purchase_order::{PurchaseLineType, PurchaseOrderRevisionLine};
    use erp_supply::api_order::{ApiOrderSourceLine, ApiOrderStage};
    use erp_supply::ports::connector::common::{
        ActionKey, ConnectorResult, Snapshot, SourceRevision, SourceStamp, SupplierSku,
    };
    use erp_supply::ports::connector::order::{ConfirmationState, OrderLine};
    use erp_supply::ports::connector::order_tree::{OrderNode, OrderNodeLine, OrderNodeRole, OrderTree};
    use erp_supply::ports::connector::receipt::{ConfirmReceipt, ReceiptLine};

    use super::dispatch::{ReceiptDispatchStore, recover, send};
    use super::evidence::{ActualReceiptFacts, DirectReceiptFacts, ReceiptEvidence, WarehouseReceiptFacts};
    use super::*;

    fn qty(value: &str) -> Quantity {
        value.parse().unwrap()
    }
    pub(super) fn fixture() -> (ApiOrderExecution, ConfirmReceipt) {
        let sku = SupplierSku { product_id: None, spec_id: "123".into() };
        let mut execution = ApiOrderExecution::queued(
            "execution".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![ApiOrderSourceLine {
                fulfillment_item_id: "item".into(),
                purchase_order_revision_line_id: "purchase-line".into(),
                sales_order_line_id: Some("sales-line".into()),
                supplier_offering_revision_id: "offering".into(),
                quantity: qty("2"),
                delivery_allocation_index: 0,
            }],
        );
        execution.stage = ApiOrderStage::Observe;
        execution.order_tree = Some(OrderTree {
            root_order_no: "leaf".into(),
            version: 1,
            nodes: vec![OrderNode {
                external_order_no: "leaf".into(),
                parent_order_no: None,
                role: OrderNodeRole::Leaf,
                lines: vec![OrderNodeLine { line_id: "item".into(), sku: sku.clone(), quantity: qty("2") }],
                confirmation: Some(ConfirmationState::Confirmed),
                amounts: None,
                fulfillment: None,
                cancellation: None,
                refund: None,
            }],
        });
        let request = ConfirmReceipt {
            action: ActionKey { id: "receipt".into(), payload_hash: "hash".into() },
            order: OrderReference { merchant_order_no: "execution".into(), external_order_no: "leaf".into() },
            topology_version: 1,
            lines: vec![ReceiptLine { line_id: "item".into(), sku, quantity: qty("2") }],
            evidence_fingerprint: "a".repeat(64),
        };
        (execution, request)
    }

    #[test]
    fn complete_source_quantities_are_required_without_guessing_partial_leaf_coverage() {
        let (execution, request) = fixture();
        execution.ensure_receipt_coverage(&request.lines).unwrap();
        for invalid in [vec![], vec![request.lines[0].clone(), request.lines[0].clone()]] {
            assert!(execution.ensure_receipt_coverage(&invalid).is_err());
        }
        for (line_id, quantity) in [("item", "1"), ("item", "3"), ("other", "2")] {
            let mut invalid = request.lines.clone();
            invalid[0].line_id = line_id.into();
            invalid[0].quantity = qty(quantity);
            assert!(execution.ensure_receipt_coverage(&invalid).is_err());
        }
    }

    #[test]
    fn original_unknown_receipt_is_never_recreated_after_restart_or_split() {
        let (mut execution, request) = fixture();
        assert!(execution.begin_receipt_confirmation(&request).unwrap());
        let mut restored: ApiOrderExecution =
            serde_json::from_str(&serde_json::to_string(&execution).unwrap()).unwrap();
        assert!(!restored.begin_receipt_confirmation(&request).unwrap());
        let mut changed = request.clone();
        changed.lines[0].quantity = qty("1");
        assert!(restored.begin_receipt_confirmation(&changed).is_err());
        restored.order_tree.as_mut().unwrap().nodes[0].role = OrderNodeRole::Parent;
        assert!(!restored.begin_receipt_confirmation(&request).unwrap());
        assert_eq!(restored.receipt_actions["leaf"].state, ReceiptActionState::SentUnknown);
        assert!(
            restored
                .observe_receipt_confirmation(&request.order, 1, ReceiptConfirmationState::NotRequired)
                .is_err()
        );
        restored
            .observe_receipt_confirmation(&request.order, 1, ReceiptConfirmationState::Confirmed)
            .unwrap();
        assert_eq!(restored.stage, ApiOrderStage::Observe);
        assert_eq!(restored.receipt_actions["leaf"].request, Some(request));
    }

    fn warehouse() -> WarehouseReceiptFacts {
        let (execution, _) = fixture();
        let purchase_line = PurchaseOrderRevisionLine {
            base: BaseModel { id: "purchase-line".into(), ..BaseModel::fake() },
            purchase_order_revision_id: PurchaseOrderRevisionId::new("revision"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            supplier_offering_source: None,
            delivery_allocations: vec![],
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: Some(qty("2")),
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: Amount::zero(),
            net_amount: Amount::zero(),
            tax_amount: Amount::zero(),
            input_tax_rate: None,
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            allocated_quantity: None,
            expense: None,
        };
        WarehouseReceiptFacts {
            receipts: vec![PurchaseReceipt {
                base: BaseModel { id: "receipt".into(), ..BaseModel::fake() },
                receipt_no: "RK-1".into(),
                purchase_order_id: PurchaseOrderId::new("purchase"),
                warehouse_id: WarehouseId::new("warehouse"),
                status: PurchaseReceiptState::Posted,
                posted_at: Some(Instant::now()),
                posted_by: Some("operator".into()),
            }],
            lines: vec![PurchaseReceiptLine {
                base: BaseModel { id: "receipt-line".into(), ..BaseModel::fake() },
                purchase_receipt_id: PurchaseReceiptId::new("receipt"),
                line_no: 1,
                purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("purchase-line"),
                received_quantity: qty("2"),
                qualified_quantity: qty("2"),
                rejected_quantity: qty("0"),
                quality_result: QualityResult::Passed,
            }],
            purchase_lines: vec![purchase_line],
            executions: vec![execution],
            warehouse_id: "warehouse".into(),
        }
    }

    #[test]
    fn warehouse_receipt_requires_exact_original_full_quantity_and_current_effective_receipt() {
        let (execution, _) = fixture();
        assert_eq!(warehouse().quantities(&execution).unwrap()["item"], qty("2"));
        for invalid in
            ["reversed", "partial", "rejected", "wrong-warehouse", "wrong-line", "ambiguous-executions"]
        {
            let mut facts = warehouse();
            match invalid {
                "reversed" => facts.receipts[0].status = PurchaseReceiptState::Reversed,
                "partial" => {
                    facts.lines[0].qualified_quantity = qty("1");
                    facts.lines[0].received_quantity = qty("1");
                },
                "rejected" => {
                    facts.lines[0].qualified_quantity = qty("1");
                    facts.lines[0].rejected_quantity = qty("1");
                },
                "wrong-warehouse" => facts.receipts[0].warehouse_id = WarehouseId::new("other"),
                "wrong-line" => {
                    facts.lines[0].purchase_order_revision_line_id = PurchaseOrderRevisionLineId::new("other")
                },
                _ => facts.executions.push(execution.clone()),
            }
            assert!(facts.quantities(&execution).is_err(), "{invalid}");
        }
    }

    #[test]
    fn missing_direct_delivery_mapping_never_becomes_receipt_from_ordered_quantity() {
        let (execution, request) = fixture();
        let tracking = erp_supply::order_tracking::OrderTracking::new(
            "execution".into(),
            "connection".into(),
            "purchase".into(),
            vec![erp_supply::order_tracking::TrackingSourceLine {
                fulfillment_item_id: "item".into(),
                purchase_order_revision_line_id: "purchase-line".into(),
                sales_order_line_id: Some("sales-line".into()),
                delivery_id: None,
                quantity: qty("2"),
            }],
        )
        .unwrap();
        let frozen = vec![OrderLine {
            line_id: "item".into(),
            sku: request.lines[0].sku.clone(),
            quantity: qty("2"),
            approved_unit_price: "1".parse().unwrap(),
            attributes: vec![],
            source_purchase_terms: None,
        }];
        assert!(
            ReceiptEvidence::from_facts(
                &execution,
                &tracking,
                &frozen,
                &ActualReceiptFacts::Direct(DirectReceiptFacts::default())
            )
            .is_err()
        );
    }

    struct Store {
        current: Mutex<ApiOrderExecution>,
        request: ConfirmReceipt,
        fail_claim: bool,
        fail_save: Mutex<bool>,
        trace: Mutex<Vec<&'static str>>,
    }
    #[async_trait]
    impl ReceiptDispatchStore for Store {
        async fn claim(
            &self,
            _: &ApiOrderExecution,
            _: &OrderReference,
        ) -> Result<(ApiOrderExecution, ConfirmReceipt)> {
            self.trace.lock().unwrap().push("claim");
            if self.fail_claim {
                return Err(Error::ConflictError("source receipt reversed or concurrent owner".into()));
            }
            let mut current = self.current.lock().unwrap();
            if !current.begin_receipt_confirmation(&self.request)? {
                return Err(Error::ConflictError("original action already persisted".into()));
            }
            Ok((current.clone(), self.request.clone()))
        }
        async fn save(&self, execution: &mut ApiOrderExecution) -> Result<()> {
            self.trace.lock().unwrap().push("save");
            if *self.fail_save.lock().unwrap() {
                return Err(Error::ConflictError("reversal wake won CAS".into()));
            }
            *self.current.lock().unwrap() = execution.clone();
            Ok(())
        }
    }
    struct Port {
        calls: Mutex<Vec<&'static str>>,
        state: ReceiptConfirmationState,
    }
    fn snapshot(value: ReceiptConfirmationState) -> Snapshot<ReceiptConfirmationState> {
        Snapshot {
            value,
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::now(),
            },
        }
    }
    #[async_trait]
    impl OrderReceiptConfirmation for Port {
        async fn confirm_receipt(
            &self,
            _: &ConfirmReceipt,
        ) -> ConnectorResult<Snapshot<ReceiptConfirmationState>> {
            self.calls.lock().unwrap().push("send");
            Ok(snapshot(self.state))
        }
        async fn receipt_confirmation(
            &self,
            _: &OrderReference,
        ) -> ConnectorResult<Lookup<Snapshot<ReceiptConfirmationState>>> {
            self.calls.lock().unwrap().push("query");
            Ok(Lookup::Found(snapshot(self.state)))
        }
    }
    fn store() -> Store {
        let (execution, request) = fixture();
        Store {
            current: Mutex::new(execution),
            request,
            fail_claim: false,
            fail_save: Mutex::new(false),
            trace: Mutex::new(vec![]),
        }
    }

    #[tokio::test]
    async fn claim_failure_or_reversal_before_claim_sends_nothing() {
        let mut store = store();
        store.fail_claim = true;
        let expected = store.current.lock().unwrap().clone();
        let order = store.request.order.clone();
        let port = Port { calls: Mutex::new(vec![]), state: ReceiptConfirmationState::Confirmed };
        assert!(send(&store, &expected, &order, &port).await.is_err());
        assert!(port.calls.lock().unwrap().is_empty());
        assert!(store.current.lock().unwrap().receipt_actions.is_empty());
    }

    #[tokio::test]
    async fn persisted_unknown_survives_crash_and_takeover_only_queries_original_target() {
        let store = store();
        let expected = store.current.lock().unwrap().clone();
        let order = store.request.order.clone();
        store.claim(&expected, &order).await.unwrap();
        let mut restored = store.current.lock().unwrap().clone();
        let port = Port { calls: Mutex::new(vec![]), state: ReceiptConfirmationState::Unverified };
        recover(&store, &mut restored, &order, 1, &port, None).await.unwrap();
        recover(&store, &mut restored, &order, 1, &port, None).await.unwrap();
        assert_eq!(*port.calls.lock().unwrap(), ["query", "query"]);
        assert_eq!(restored.receipt_actions["leaf"].state, ReceiptActionState::SentUnknown);
        assert!(send(&store, &restored, &order, &port).await.is_err());
        assert_eq!(port.calls.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn response_save_conflict_recovers_without_resend_and_keeps_reversed_source_blocker() {
        let store = store();
        *store.fail_save.lock().unwrap() = true;
        let expected = store.current.lock().unwrap().clone();
        let order = store.request.order.clone();
        let port = Port { calls: Mutex::new(vec![]), state: ReceiptConfirmationState::Confirmed };
        assert!(send(&store, &expected, &order, &port).await.is_err());
        assert_eq!(
            store.current.lock().unwrap().receipt_actions["leaf"].state,
            ReceiptActionState::SentUnknown
        );
        *store.fail_save.lock().unwrap() = false;
        let mut restored = store.current.lock().unwrap().clone();
        recover(&store, &mut restored, &order, 1, &port, Some("原客户验收已冲正".into())).await.unwrap();
        assert_eq!(*port.calls.lock().unwrap(), ["send", "query"]);
        assert_eq!(*store.trace.lock().unwrap(), ["claim", "save", "save"]);
        assert_eq!(restored.receipt_actions["leaf"].state, ReceiptActionState::Confirmed);
        assert!(restored.receipt_blocker.as_deref().unwrap().contains("原客户验收已冲正"));
        assert_eq!(restored.stage, ApiOrderStage::Observe);
    }
}
