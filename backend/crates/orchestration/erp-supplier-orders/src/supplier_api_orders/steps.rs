use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_fulfillment_execution::fulfillment_execution::ensure_po_fulfillable;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderStage};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus};
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{ActionKey, ConnectorError, Lookup};
use erp_supply::ports::connector::funds::OrderFundingRequirement;
use erp_supply::ports::connector::offer::DeliveryQuery;
use erp_supply::ports::connector::order::{
    CreateOrder, CreationNext, DeliveryMethod, OrderKey, OrderSnapshot,
};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::{Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};

use super::snapshot::ExecutionSnapshot;
use super::{SupplierApiOrderProcess, missing_capability};
use crate::supplier_deposits::prepare_automatic_execution_funds;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn connector(
        &self,
        execution: &ApiOrderExecution,
    ) -> Result<Arc<dyn SupplierConnector>> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        let connector = if matches!(
            execution.stage,
            ApiOrderStage::ConfirmPayment
                | ApiOrderStage::PaymentUnknown
                | ApiOrderStage::Observe
                | ApiOrderStage::DiscoverOrders
                | ApiOrderStage::ConfirmationUnknown
                | ApiOrderStage::AddressUnknown
        ) {
            self.runtime.fulfillment_connector(&connection)
        } else {
            self.runtime.creation_connector(&connection)
        };
        connector.map_err(|failure| Error::BusinessLogicError(failure.summary))
    }

    pub(super) async fn claim(&self, execution: &mut ApiOrderExecution, stage: ApiOrderStage) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut claimed = execution.clone();
        claimed.stage = stage;
        claimed.next_run_at = Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(300));
        let process = self.clone();
        *execution = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.fence_purchase(&claimed, executor).await?;
                    db.supplier_api_order_executions().update(&mut claimed, executor).await?;
                    Ok::<_, Error>(claimed)
                })
            })
            .await?;
        Ok(())
    }

    async fn fence_purchase(&self, claimed: &ApiOrderExecution, executor: &mut dyn Executor) -> Result<()> {
        let mut order = self
            .db
            .purchase_orders()
            .find_by_id(&claimed.purchase_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购不存在".into()))?;
        ensure_po_fulfillable(&order)?;
        if order.stable.current_revision_id.as_deref() != Some(claimed.purchase_order_revision_id.as_str()) {
            return Err(Error::ConflictError("采购版本已变化，接口执行已阻断".into()));
        }
        self.ensure_execution_funds(claimed, &order, executor).await?;
        self.fence_connection(&claimed.connection_id, claimed.stage, executor).await?;
        if matches!(claimed.stage, ApiOrderStage::CreationUnknown | ApiOrderStage::ConfirmationUnknown) {
            self.mark_funds_sent(claimed, executor).await?;
        }
        if claimed.stage == ApiOrderStage::PaymentUnknown {
            let supplier_order = self
                .db
                .supplier_fulfillment_orders()
                .find_by_id(&claimed.base.id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("供应商订单不存在".into()))?;
            if supplier_order.cancel_status == CancelStatus::Canceled {
                return Err(Error::BusinessLogicError("供应商订单已取消，不再发送履约通知".into()));
            }
        }
        self.db.purchase_orders().update(&mut order, executor).await?;
        Ok(())
    }

    async fn fence_connection(
        &self,
        id: &str,
        stage: ApiOrderStage,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let service = SupplierApiService::new(self.db.clone());
        let mut connection = service.load_connection(id, executor).await?;
        let client = if stage == ApiOrderStage::PaymentUnknown {
            self.runtime.fulfillment_connector(&connection)
        } else {
            self.runtime.creation_connector(&connection)
        };
        client.map_err(|failure| Error::BusinessLogicError(failure.summary))?;
        service.persist_connection(&mut connection, executor).await?;
        Ok(())
    }

    pub(super) async fn prepare_address(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connector = self.connector(&execution).await?;
        let mut snapshot = self.decode(&execution)?;
        let key = freeze_action(&mut execution, "address", &snapshot.recipient)?;
        self.claim(&mut execution, ApiOrderStage::AddressUnknown).await?;
        match connector
            .delivery()
            .ok_or_else(|| missing_capability("地址准备"))?
            .prepare(&key, &snapshot.recipient)
            .await
        {
            Ok(context) => {
                snapshot.context = Some(context);
                self.encode(&mut execution, &snapshot)?;
                execution.stage = ApiOrderStage::PlanDelivery;
                execution.blocker = None;
                execution.next_run_at = Instant::now();
            },
            Err(error) => record_failure(&mut execution, error, ApiOrderStage::PrepareAddress),
        }
        self.persist(&mut execution).await
    }

    pub(super) async fn recover_address(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connector = self.connector(&execution).await?;
        let mut snapshot = self.decode(&execution)?;
        let Some(hash) = execution.action_hashes.get("address") else {
            execution.defer("原地址准备动作缺少完整记录，请联系管理员核实，系统不会重建地址");
            return self.persist(&mut execution).await;
        };
        let key = ActionKey { id: format!("{}-address", execution.base.id), payload_hash: hash.clone() };
        match connector
            .delivery()
            .ok_or_else(|| missing_capability("原地址结果查询"))?
            .context_with_recipient(&key, &snapshot.recipient)
            .await
        {
            Ok(Lookup::Found(context)) => {
                snapshot.context = Some(context);
                self.encode(&mut execution, &snapshot)?;
                execution.stage = ApiOrderStage::PlanDelivery;
                execution.blocker = None;
                execution.next_run_at = Instant::now();
            },
            Ok(Lookup::NotVisible) => execution.defer("原地址结果暂不可见，系统继续查询，不重复创建地址"),
            Err(error) => execution.defer(error.summary),
        }
        self.persist(&mut execution).await
    }

    pub(super) async fn plan_delivery(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connector = self.connector(&execution).await?;
        let mut snapshot = self.decode(&execution)?;
        let previous = snapshot.selected_choice.and_then(|index| snapshot.choices.get(index)).cloned();
        let query = DeliveryQuery {
            context: snapshot
                .context
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("地址准备尚未完成".into()))?,
            lines: snapshot.lines.clone(),
        };
        match connector.delivery().ok_or_else(|| missing_capability("配送核验"))?.options(&query).await {
            Ok(plan) => {
                if plan.value.groups.len() != 1
                    || plan.value.groups[0].line_ids
                        != snapshot.lines.iter().map(|line| line.line_id.clone()).collect::<Vec<_>>()
                {
                    execution.stage = ApiOrderStage::AwaitingSetup;
                    execution.defer("供应商配送拆分与冻结明细不一致，请核对商品配送规则");
                } else {
                    snapshot.choices = plan.value.groups[0].choices.clone();
                    snapshot.selected_choice = choose(&snapshot, previous.as_ref());
                    execution.stage = if snapshot.selected_choice.is_some() {
                        ApiOrderStage::Create
                    } else {
                        ApiOrderStage::AwaitingSetup
                    };
                    execution.blocker = if snapshot.selected_choice.is_none() {
                        Some("请按已批准交付条件选择供应商配送方案".into())
                    } else {
                        None
                    };
                    execution.next_run_at = Instant::now();
                    self.encode(&mut execution, &snapshot)?;
                }
            },
            Err(error) => record_failure(&mut execution, error, ApiOrderStage::PlanDelivery),
        }
        self.persist(&mut execution).await
    }

    pub(super) async fn create(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connector = self.connector(&execution).await?;
        let snapshot = self.decode(&execution)?;
        let mut request = match create_request(&execution, &snapshot) {
            Ok(request) => request,
            Err(error) => {
                execution.defer(error.to_string());
                return self.persist(&mut execution).await;
            },
        };
        if let Err(error) = self.preflight(&execution, connector.as_ref(), &request).await {
            execution.stage = ApiOrderStage::PlanDelivery;
            execution.defer(error.to_string());
            return self.persist(&mut execution).await;
        }
        if connector.funding_requirement() == OrderFundingRequirement::VerifiedSupplierDeposit
            && let Err(error) = prepare_automatic_execution_funds(&self.db, &execution, &self.codec).await
        {
            execution.defer(error.to_string());
            return self.persist(&mut execution).await;
        }
        request.action = freeze_create_action(&mut execution, &request)?;
        self.claim(&mut execution, ApiOrderStage::CreationUnknown).await?;
        match connector.orders().ok_or_else(|| missing_capability("下单"))?.create(&request).await {
            Ok(created) => self.save_created(&mut execution, created.order.value, created.next).await?,
            Err(error) => record_failure(&mut execution, error, ApiOrderStage::PlanDelivery),
        }
        self.persist(&mut execution).await
    }

    pub(super) async fn recover_creation(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        let frozen = self.decode(&execution)?;
        match self
            .runtime
            .observe_order_with_lines(
                &connection,
                &OrderKey::Merchant(execution.base.id.clone()),
                &frozen.lines,
            )
            .await
        {
            Ok(Lookup::Found(snapshot)) => {
                let connector = self
                    .runtime
                    .fulfillment_connector(&connection)
                    .map_err(|error| Error::BusinessLogicError(error.summary))?;
                let next = match connector
                    .orders()
                    .ok_or_else(|| missing_capability("原单恢复"))?
                    .recovery_next(&snapshot.value)
                {
                    Ok(next) => next,
                    Err(error) => {
                        execution.defer(error.summary);
                        return self.persist(&mut execution).await;
                    },
                };
                self.save_created(&mut execution, snapshot.value, next).await?;
            },
            Ok(Lookup::NotVisible) => {
                execution.defer("原下单结果仍未查明；暂未查到不代表未创建，系统不会重复发单")
            },
            Err(error) => execution.defer(error.summary),
        }
        self.persist(&mut execution).await
    }

    async fn save_created(
        &self,
        execution: &mut ApiOrderExecution,
        snapshot: OrderSnapshot,
        next: CreationNext,
    ) -> Result<()> {
        if snapshot.reference.merchant_order_no != execution.base.id {
            return Err(Error::BusinessLogicError("供应商订单身份不一致".into()));
        }
        let mut request = self.decode(execution)?;
        request.actual_payable = snapshot.amounts.as_ref().map(|amounts| amounts.payable);
        request.requires_payment = next == CreationNext::ConfirmPayment;
        let over = request.actual_payable.is_some_and(|amount| amount > request.approved_total);
        let cancelled = snapshot.cancellation == Some(CancelStatus::Canceled);
        let mut discovery_blocker = None;
        if let Some(tree) = snapshot.topology {
            execution.pending_order_tree = Some(tree.clone());
            if tree.root_order_no != snapshot.reference.external_order_no {
                discovery_blocker = Some("供应商订单关系与原根单不一致，继续调查原订单".to_owned());
            } else if let Err(error) = execution.apply_observed_tree(tree, &request.lines) {
                discovery_blocker = Some(error.to_string());
            }
        }
        execution.stage = if over || cancelled {
            ApiOrderStage::Stopped
        } else {
            match next {
                CreationNext::ConfirmPayment => ApiOrderStage::ConfirmPayment,
                CreationNext::ConfirmOrder
                    if execution.order_tree.is_none() || discovery_blocker.is_some() =>
                {
                    ApiOrderStage::DiscoverOrders
                },
                CreationNext::ConfirmOrder => ApiOrderStage::ConfirmOrder,
                CreationNext::ObserveOrder => ApiOrderStage::Observe,
            }
        };
        execution.blocker = if cancelled {
            Some("供应商已明确取消原订单，已停止履约通知".into())
        } else {
            over.then(|| "供应商已创建订单但金额超出批准范围，已阻断后续支付确认，请处理原单".into())
        };
        if discovery_blocker.is_some() && !over && !cancelled {
            execution.blocker = discovery_blocker;
        }
        execution.next_run_at = Instant::now();
        self.encode(execution, &request)?;
        self.persist_created_reference(execution, snapshot.reference.external_order_no).await
    }
    async fn persist_created_reference(
        &self,
        execution: &mut ApiOrderExecution,
        external: String,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut saved = execution.clone();
        *execution = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut order = db
                        .supplier_fulfillment_orders()
                        .find_by_id(&saved.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商订单不存在".into()))?;
                    if order.external_order_no.as_ref().is_some_and(|number| number != &external) {
                        return Err(Error::ConflictError("供应商订单号与原结果不一致".into()));
                    }
                    order.external_order_no = Some(external);
                    if order.fulfillment_status == FulfillmentStatus::Received {
                        order.fulfillment_status = FulfillmentStatus::Submitting;
                    }
                    db.supplier_fulfillment_orders().update(&mut order, executor).await?;
                    db.supplier_api_order_executions().update(&mut saved, executor).await?;
                    Ok::<_, Error>(saved)
                })
            })
            .await?;
        Ok(())
    }
}

pub(super) fn create_request(
    execution: &ApiOrderExecution,
    snapshot: &ExecutionSnapshot,
) -> Result<CreateOrder> {
    let delivery = snapshot
        .selected_choice
        .and_then(|index| snapshot.choices.get(index))
        .cloned()
        .ok_or_else(|| Error::BusinessLogicError("未选择配送方案".into()))?;
    let approved_freight =
        snapshot.approved_budget.as_ref().map(|budget| budget.approved_freight).unwrap_or_else(Amount::zero);
    if delivery.shipping_fee > approved_freight {
        return Err(Error::BusinessLogicError("配送运费超出本执行组已批准运费，请先完成采购费用审批".into()));
    }
    Ok(CreateOrder {
        action: ActionKey { id: format!("{}-create", execution.base.id), payload_hash: String::new() },
        merchant_order_no: execution.base.id.clone(),
        lines: snapshot.lines.clone(),
        recipient: snapshot.recipient.clone(),
        delivery_context: snapshot
            .context
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("配送地址上下文缺失".into()))?,
        delivery,
        approved_total: snapshot.approved_total,
    })
}

pub(super) fn freeze_action(
    execution: &mut ApiOrderExecution,
    kind: &str,
    payload: &impl serde::Serialize,
) -> Result<ActionKey> {
    let canonical =
        serde_json::to_vec(payload).map_err(|_| Error::Internal("接口独立步骤载荷无法编码".into()))?;
    let hash = hex::encode(Sha256::digest(&canonical));
    if execution.action_hashes.get(kind).is_some_and(|old| old != &hash) {
        return Err(Error::ConflictError("原接口动作载荷已变化，必须先核查原请求".into()));
    }
    execution.action_hashes.insert(kind.into(), hash.clone());
    Ok(ActionKey { id: format!("{}-{kind}", execution.base.id), payload_hash: hash })
}

fn freeze_create_action(execution: &mut ApiOrderExecution, request: &CreateOrder) -> Result<ActionKey> {
    // 地址和配送短期签名令牌不是业务载荷，刷新令牌不更换原动作业务指纹。
    freeze_action(
        execution,
        "create",
        &(
            &request.merchant_order_no,
            &request.lines,
            &request.recipient,
            &request.delivery.method,
            request.delivery.shipping_fee,
            request.approved_total,
        ),
    )
}

fn choose(
    snapshot: &ExecutionSnapshot,
    previous: Option<&erp_supply::ports::connector::order::DeliveryChoice>,
) -> Option<usize> {
    if let Some(previous) = previous {
        return snapshot.choices.iter().position(|choice| {
            choice.method == previous.method && choice.shipping_fee == previous.shipping_fee
        });
    }
    let candidates = snapshot
        .choices
        .iter()
        .enumerate()
        .filter(|(_, choice)| match &choice.method {
            DeliveryMethod::Courier => true,
            DeliveryMethod::LocalDelivery { date, .. } => snapshot.expected_delivery_date == Some(*date),
            DeliveryMethod::Pickup { .. } => false,
        })
        .map(|(index, _)| index)
        .collect::<Vec<_>>();
    (candidates.len() == 1).then(|| candidates[0])
}

pub(super) fn record_failure(
    execution: &mut ApiOrderExecution,
    error: ConnectorError,
    known_stage: ApiOrderStage,
) {
    // 所有协议都必须将写后传输或解码不确定映射为 ResultUnknown；
    // 只有写前失败或供应商明确拒绝，才可解除该步骤的发送占位。
    if error.class != SupplierFailureClass::ResultUnknown {
        let action_kind = match execution.stage {
            ApiOrderStage::AddressUnknown => Some("address"),
            ApiOrderStage::CreationUnknown => Some("create"),
            ApiOrderStage::PaymentUnknown => Some("payment"),
            _ => None,
        };
        if let Some(kind) = action_kind {
            execution.action_hashes.remove(kind);
        }
        execution.stage = if error.code == "DGSS_DELIVERY_EXPIRED"
            || matches!(
                error.class,
                SupplierFailureClass::TransientFailure | SupplierFailureClass::RateLimited
            ) {
            known_stage
        } else if execution.stage == ApiOrderStage::PaymentUnknown {
            ApiOrderStage::Stopped
        } else {
            ApiOrderStage::AwaitingSetup
        };
    }
    execution.defer(error.summary);
}

#[cfg(test)]
mod tests {
    use erp_supply::api_order::ApiOrderSourceLine;
    use erp_supply::entity::execution_budget::ApprovedExecutionBudget;
    use erp_supply::ports::connector::offer::DeliveryContext;
    use erp_supply::ports::connector::order::{DeliveryChoice, Recipient};

    use super::*;

    fn snapshot(freight: &str) -> ExecutionSnapshot {
        ExecutionSnapshot {
            recipient: Recipient {
                name: "收件人".into(),
                phone: "13800000000".into(),
                region: "110105".into(),
                address: "某路".into(),
                geocoded_address: None,
                supplier_address: None,
            },
            lines: vec![],
            approved_total: "100".parse().unwrap(),
            approved_budget: None,
            source_purchase_terms: vec![],
            expected_delivery_date: None,
            context: Some(DeliveryContext { reference: "confirmed-address".into(), supplier_address: None }),
            choices: vec![DeliveryChoice {
                method: DeliveryMethod::Courier,
                shipping_fee: freight.parse().unwrap(),
                reference: "option".into(),
            }],
            selected_choice: Some(0),
            actual_payable: None,
            requires_payment: false,
        }
    }

    #[test]
    fn old_snapshot_cannot_borrow_product_budget_for_unapproved_freight() {
        let execution = ApiOrderExecution::queued(
            "group".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![],
        );
        assert!(create_request(&execution, &snapshot("0")).is_ok());
        let error =
            create_request(&execution, &snapshot("0.01")).err().expect("没有批准运费不能消费商品节余");
        assert!(error.to_string().contains("批准运费"));
        let mut current = snapshot("2");
        current.approved_budget = Some(ApprovedExecutionBudget {
            purchase_revision_id: "revision".into(),
            execution_id: "group".into(),
            approved_goods: "98".parse().unwrap(),
            approved_freight: "2".parse().unwrap(),
            approved_other: Amount::zero(),
            approved_total: "100".parse().unwrap(),
            freight_allocations: vec![],
        });
        let request = create_request(&execution, &current).unwrap();
        assert_eq!(request.delivery.shipping_fee, "2".parse().unwrap());
        current.choices[0].shipping_fee = "2.01".parse().unwrap();
        assert!(create_request(&execution, &current).is_err());
    }

    #[test]
    fn unknown_create_never_reopens_write_step() {
        let mut order = ApiOrderExecution::queued(
            "o".into(),
            "p".into(),
            "r".into(),
            "c".into(),
            "s".into(),
            Vec::<ApiOrderSourceLine>::new(),
        );
        order.stage = ApiOrderStage::CreationUnknown;
        order.action_hashes.insert("create".into(), "original-create-hash".into());
        order.action_hashes.insert("address".into(), "original-address-hash".into());
        order.request_encrypted = Some("original-encrypted-intent".into());
        let original_order_no = order.base.id.clone();
        record_failure(
            &mut order,
            ConnectorError {
                class: SupplierFailureClass::ResultUnknown,
                code: "JD_VOP_1004".into(),
                summary: "京东原请求结果待核实".into(),
                retry_after: None,
            },
            ApiOrderStage::Create,
        );
        assert_eq!(order.stage, ApiOrderStage::CreationUnknown);
        assert!(order.blocker.is_some());
        assert_eq!(order.base.id, original_order_no);
        assert_eq!(order.action_hashes["create"], "original-create-hash");
        assert_eq!(order.action_hashes["address"], "original-address-hash");
        assert_eq!(order.request_encrypted.as_deref(), Some("original-encrypted-intent"));
        assert_eq!(create_request(&order, &snapshot("0")).unwrap().merchant_order_no, original_order_no);
    }

    #[test]
    fn independent_action_hash_ignores_ciphertext_nonce_and_rejects_changed_payload() {
        let mut execution =
            ApiOrderExecution::queued("o".into(), "p".into(), "r".into(), "c".into(), "s".into(), Vec::new());
        execution.request_encrypted = Some("nonce-one".into());
        let original = freeze_action(&mut execution, "create", &("sku-1", "2", "9.90")).unwrap();
        execution.request_encrypted = Some("nonce-two".into());
        assert_eq!(freeze_action(&mut execution, "create", &("sku-1", "2", "9.90")).unwrap(), original);
        assert!(matches!(
            freeze_action(&mut execution, "create", &("sku-1", "3", "9.90")),
            Err(Error::ConflictError(_))
        ));
        assert!(freeze_action(&mut execution, "payment", &("external-1", "19.80")).is_ok());
        assert_eq!(execution.action_hashes.len(), 2);
    }

    #[test]
    fn explicit_unsent_validation_failure_releases_only_its_action() {
        let mut execution =
            ApiOrderExecution::queued("o".into(), "p".into(), "r".into(), "c".into(), "s".into(), Vec::new());
        execution.stage = ApiOrderStage::CreationUnknown;
        execution.action_hashes.insert("create".into(), "original".into());
        execution.action_hashes.insert("address".into(), "address".into());
        record_failure(
            &mut execution,
            ConnectorError {
                class: SupplierFailureClass::MappingError,
                code: "DGSS_MAPPING".into(),
                summary: "写前参数无法核实".into(),
                retry_after: None,
            },
            ApiOrderStage::Create,
        );
        assert_eq!(execution.stage, ApiOrderStage::AwaitingSetup);
        assert!(!execution.action_hashes.contains_key("create"));
        assert!(execution.action_hashes.contains_key("address"));
    }

    #[test]
    fn expired_unsent_delivery_returns_to_read_only_planning() {
        let mut execution =
            ApiOrderExecution::queued("o".into(), "p".into(), "r".into(), "c".into(), "s".into(), Vec::new());
        execution.stage = ApiOrderStage::CreationUnknown;
        execution.action_hashes.insert("create".into(), "old".into());
        record_failure(
            &mut execution,
            ConnectorError {
                class: SupplierFailureClass::BusinessRejected,
                code: "DGSS_DELIVERY_EXPIRED".into(),
                summary: "配送令牌已过期，未发送请求".into(),
                retry_after: None,
            },
            ApiOrderStage::PlanDelivery,
        );
        assert_eq!(execution.stage, ApiOrderStage::PlanDelivery);
        assert!(!execution.action_hashes.contains_key("create"));
    }

    #[test]
    fn refreshing_short_lived_proofs_preserves_create_business_identity() {
        use erp_supply::ports::connector::offer::DeliveryContext;
        use erp_supply::ports::connector::order::{DeliveryChoice, Recipient};
        let mut execution =
            ApiOrderExecution::queued("o".into(), "p".into(), "r".into(), "c".into(), "s".into(), Vec::new());
        let mut request = CreateOrder {
            action: ActionKey { id: "o-create".into(), payload_hash: String::new() },
            merchant_order_no: "o".into(),
            lines: Vec::new(),
            recipient: Recipient {
                name: "收件人".into(),
                phone: "13800000000".into(),
                region: "110105".into(),
                address: "某路".into(),
                geocoded_address: None,
                supplier_address: None,
            },
            delivery_context: DeliveryContext { reference: "context-v1".into(), supplier_address: None },
            delivery: DeliveryChoice {
                method: DeliveryMethod::Courier,
                shipping_fee: "0".parse().unwrap(),
                reference: "token-v1".into(),
            },
            approved_total: "10".parse().unwrap(),
        };
        let original = freeze_create_action(&mut execution, &request).unwrap();
        request.delivery_context.reference = "context-v2".into();
        request.delivery.reference = "token-v2".into();
        assert_eq!(freeze_create_action(&mut execution, &request).unwrap(), original);
        request.delivery.shipping_fee = "1".parse().unwrap();
        assert!(freeze_create_action(&mut execution, &request).is_err());
    }
}
