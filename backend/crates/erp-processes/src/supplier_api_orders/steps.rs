use erp_core::common::time::Instant;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, ApiOrderStage};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_fulfillment::{CancelStatus, FulfillmentStatus};
use erp_supply::ports::connector::common::{ActionKey, ConnectorError, Lookup};
use erp_supply::ports::connector::offer::{DeliveryQuery, DeliverySource};
use erp_supply::ports::connector::order::{
    CreateOrder, CreationNext, DeliveryMethod, OrderKey, OrderSnapshot, Orders, PaymentState,
};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::{Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};

use super::SupplierApiOrderProcess;
use super::snapshot::ExecutionSnapshot;
use crate::connectors::dangaoshushu::DangaoshushuConnector;
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn connector(&self, execution: &ApiOrderExecution) -> Result<DangaoshushuConnector> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        self.runtime.connector(&connection).map_err(|failure| Error::BusinessLogicError(failure.summary))
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
                    use erp_procurement::repository::PurchaseOrderExt;
                    let mut order = db
                        .purchase_orders()
                        .find_by_id(&claimed.purchase_order_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源采购不存在".into()))?;
                    crate::fulfillment_execution::ensure_po_fulfillable(&order)?;
                    if order.stable.current_revision_id.as_deref()
                        != Some(claimed.purchase_order_revision_id.as_str())
                    {
                        return Err(Error::ConflictError("采购版本已变化，接口执行已阻断".into()));
                    }
                    crate::fulfillment_execution::ensure_prepay_gate(&process.db, executor, &order).await?;
                    process.fence_connection(&claimed.connection_id, executor).await?;
                    if claimed.stage == ApiOrderStage::PaymentUnknown {
                        let supplier_order = db
                            .supplier_fulfillment_orders()
                            .find_by_id(&claimed.base.id, executor)
                            .await?
                            .ok_or_else(|| Error::NotFound("供应商订单不存在".into()))?;
                        if supplier_order.cancel_status == CancelStatus::Canceled {
                            return Err(Error::BusinessLogicError(
                                "供应商订单已取消，不再发送履约通知".into(),
                            ));
                        }
                    }
                    db.purchase_orders().update(&mut order, executor).await?;
                    db.supplier_api_order_executions().update(&mut claimed, executor).await?;
                    Ok::<_, Error>(claimed)
                })
            })
            .await?;
        Ok(())
    }

    async fn fence_connection(&self, id: &str, executor: &mut dyn Executor) -> Result<()> {
        let service = SupplierApiService::new(self.db.clone());
        let mut connection = service.load_connection(id, executor).await?;
        self.runtime.connector(&connection).map_err(|failure| Error::BusinessLogicError(failure.summary))?;
        service.persist_connection(&mut connection, executor).await?;
        Ok(())
    }

    pub(super) async fn prepare_address(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connector = self.connector(&execution).await?;
        let mut snapshot = self.decode(&execution)?;
        let key = freeze_action(&mut execution, "address", &snapshot.recipient)?;
        self.claim(&mut execution, ApiOrderStage::AddressUnknown).await?;
        match connector.prepare(&key, &snapshot.recipient).await {
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
        match connector.options(&query).await {
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
        let mut request = create_request(&execution, &snapshot)?;
        if let Err(error) = self.preflight(&execution, &connector, &request).await {
            execution.stage = ApiOrderStage::PlanDelivery;
            execution.defer(error.to_string());
            return self.persist(&mut execution).await;
        }
        request.action = freeze_create_action(&mut execution, &request)?;
        self.claim(&mut execution, ApiOrderStage::CreationUnknown).await?;
        match connector.create(&request).await {
            Ok(created) => self.save_created(&mut execution, created.order.value, created.next).await?,
            Err(error) => record_failure(&mut execution, error, ApiOrderStage::PlanDelivery),
        }
        self.persist(&mut execution).await
    }

    pub(super) async fn recover_creation(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        match self.runtime.observe_order(&connection, &OrderKey::Merchant(execution.base.id.clone())).await {
            Ok(Lookup::Found(snapshot)) => {
                let next = if snapshot.value.payment == Some(PaymentState::Confirmed) {
                    CreationNext::ObserveOrder
                } else {
                    CreationNext::ConfirmPayment
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
        execution.stage = if over || cancelled {
            ApiOrderStage::Stopped
        } else if request.requires_payment {
            ApiOrderStage::ConfirmPayment
        } else {
            ApiOrderStage::Observe
        };
        execution.blocker = if cancelled {
            Some("供应商已明确取消原订单，已停止履约通知".into())
        } else {
            over.then(|| "供应商已创建订单但金额超出批准范围，已阻断后续支付确认，请处理原单".into())
        };
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

fn create_request(execution: &ApiOrderExecution, snapshot: &ExecutionSnapshot) -> Result<CreateOrder> {
    Ok(CreateOrder {
        action: ActionKey { id: format!("{}-create", execution.base.id), payload_hash: String::new() },
        merchant_order_no: execution.base.id.clone(),
        lines: snapshot.lines.clone(),
        recipient: snapshot.recipient.clone(),
        delivery_context: snapshot
            .context
            .clone()
            .ok_or_else(|| Error::BusinessLogicError("配送地址上下文缺失".into()))?,
        delivery: snapshot
            .selected_choice
            .and_then(|index| snapshot.choices.get(index))
            .cloned()
            .ok_or_else(|| Error::BusinessLogicError("未选择配送方案".into()))?,
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
    // 本运行路径限定蛋糕叔叔：写后格式/传输错误均由适配器映射为 ResultUnknown；
    // 其它类只表示写前失败或供应商明确拒绝，允许解除本步骤的发送占位。
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

    use super::*;

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
        record_failure(
            &mut order,
            ConnectorError {
                class: SupplierFailureClass::ResultUnknown,
                code: "TIMEOUT".into(),
                summary: "原请求结果未知".into(),
                retry_after: None,
            },
            ApiOrderStage::Create,
        );
        assert_eq!(order.stage, ApiOrderStage::CreationUnknown);
        assert!(order.blocker.is_some());
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
            },
            delivery_context: DeliveryContext { reference: "context-v1".into() },
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
