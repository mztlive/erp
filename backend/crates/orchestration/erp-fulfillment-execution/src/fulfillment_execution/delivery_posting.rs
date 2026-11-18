//! 发货过账根事务：逐行库存或采购门槛、发货状态、任务与审计。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_core::ids::DeliveryId;
use erp_fulfillment::dto::{DeliveryView, PostDeliveryRequest};
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryLine, DeliveryType, DeliveryUpdate};
use erp_fulfillment::service::FulfillmentService;
use erp_fulfillment::service::delivery_posting::supplier_purchase_source;
use erp_identity::AuthorizationService;
use erp_inventory::service::fulfillment::delivery::{WarehouseShipmentLine, post_warehouse_ship_line};
use erp_procurement::repository::PurchaseOrderExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::FulfillmentProcess;
use super::customer_acceptance::task::{CustomerAcceptanceTaskReason, ensure_customer_acceptance_task};
use super::delivery_packing::PackingContext;
use super::delivery_tracking::ensure_tracking_sources;
use super::purchase_context::ensure_po_fulfillable;
use super::task::{FulfillmentTaskObject, complete_fulfillment_task};
use crate::{Error, Result};

impl FulfillmentProcess {
    /// 确认实际发货时，按最终草稿和期望版本把发货单从草稿推进到已发货，并返回过账视图（§8.2 第 2 条）。
    /// 仓发在同一事务校验本销售明细的预占归属及数量，扣减预占并写预占流水，追加出库流水、更新余额、迁移状态并写审计。
    /// 供应商直发复验采购履约门槛，不写自有库存流水；未付款本身不阻止登记实际履约。
    ///
    /// 草稿状态和流水唯一索引防止重复过账。发货单、预占或余额缺失返回 `NotFound`；非法状态或重复过账返回 `ConflictError`；
    /// 预占归属、数量或业务门槛不符返回 `BusinessLogicError`；提交无法确认返回 `OutcomeUnknown`。
    #[tracing::instrument(
        name = "fulfillment.delivery_post",
        skip_all,
        fields(layer = "service", domain = "fulfillment", operation = "delivery_post")
    )]
    pub async fn post_delivery(
        &self,
        id: &str,
        req: PostDeliveryRequest,
        actor: &AuditActor,
    ) -> Result<DeliveryView> {
        req.validate()?;
        let delivery_id = DeliveryId::new(id.to_string());
        if req.packing.is_some() && req.tracking_entries.is_some() {
            return Err(Error::ValidationError(
                "请按地址和包裹填写发货信息，不要同时填写整单物流信息".into(),
            ));
        }

        let packing_context = PackingContext {
            codec: self.sensitive_data.clone(),
            key: self.fingerprint_key.clone(),
            input: req.packing.clone(),
            legacy_tracking: req.tracking_entries.is_some(),
        };
        let actor = actor.clone();
        let db = self.db.clone();
        let task_identity = self.task_identity.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    DeliveryPostingCommand { db, task_identity, delivery_id, req, packing_context, actor }
                        .execute(executor)
                        .await
                })
            })
            .await?;
        Ok(posted.into())
    }
}

/// 单次发货根命令的原始请求与认证上下文。
struct DeliveryPostingCommand {
    db: Database,
    task_identity: Arc<AuthorizationService>,
    delivery_id: DeliveryId,
    req: PostDeliveryRequest,
    packing_context: PackingContext,
    actor: AuditActor,
}

impl DeliveryPostingCommand {
    // 在同一授权事务内准备最终草稿，再执行库存、履约、任务与审计。
    async fn execute(self, executor: &mut dyn Executor) -> Result<Delivery> {
        super::task::require_fulfillment_entry(
            &self.db,
            &self.task_identity,
            "delivery",
            self.delivery_id.as_ref(),
            &self.actor,
            executor,
        )
        .await?;

        let (mut delivery, lines) = FulfillmentService::new(self.db.clone())
            .prepare_delivery_posting(
                &self.delivery_id,
                self.req.version,
                DeliveryUpdate { tracking_entries: self.req.tracking_entries },
                executor,
            )
            .await?;
        let lines = self
            .packing_context
            .prepare_posting(&self.db, &self.task_identity, &mut delivery, lines, executor)
            .await?;

        let delivery_type = delivery.delivery_type;
        let line_count = lines.len();
        let mut posting = DeliveryPosting {
            db: &self.db,
            task_identity: &self.task_identity,
            delivery_id: self.delivery_id,
            delivery,
            lines,
            occurred_at: Instant::now(),
            actor: self.actor,
            source_shipped_at: None,
        };
        execute_posting(&mut posting, delivery_type, line_count, executor).await?;
        Ok(posting.delivery)
    }
}

/// 发货写段的实际边界；仓发行保持逐行进入，不提前检查下一行。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostingStep {
    WarehouseLine(usize),
    SupplierGate,
    Delivery,
    Task,
    AcceptanceTask,
    Audit,
}

#[async_trait]
trait PostingSteps: Send {
    // 在调用方执行器中执行当前步骤，首个失败立即返回。
    async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()>;
}

// 实际生产顺序：当前行库存完成后才推进下一行，之后推进履约和任务。
async fn execute_posting(
    steps: &mut impl PostingSteps,
    delivery_type: DeliveryType,
    line_count: usize,
    executor: &mut dyn Executor,
) -> Result<()> {
    match delivery_type {
        DeliveryType::WarehouseShip => {
            for index in 0..line_count {
                steps.apply(PostingStep::WarehouseLine(index), executor).await?;
            }
        },
        DeliveryType::SupplierDirect => steps.apply(PostingStep::SupplierGate, executor).await?,
    }

    for step in [PostingStep::Delivery, PostingStep::Task, PostingStep::AcceptanceTask, PostingStep::Audit] {
        steps.apply(step, executor).await?;
    }
    Ok(())
}

struct DeliveryPosting<'a> {
    db: &'a Database,
    task_identity: &'a Arc<AuthorizationService>,
    delivery_id: DeliveryId,
    delivery: Delivery,
    lines: Vec<DeliveryLine>,
    occurred_at: Instant,
    actor: AuditActor,
    source_shipped_at: Option<Option<Instant>>,
}

#[async_trait]
impl PostingSteps for DeliveryPosting<'_> {
    async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()> {
        match step {
            PostingStep::WarehouseLine(index) => {
                self.post_warehouse_line(index, executor).await?;
            },
            PostingStep::SupplierGate => {
                self.ensure_supplier_gate(executor).await?;
            },
            PostingStep::Delivery => {
                if let Some(source_time) = self.source_shipped_at {
                    self.delivery.mark_observed_shipped(source_time)?;
                    FulfillmentService::new(self.db.clone())
                        .persist_delivery(&mut self.delivery, executor)
                        .await?;
                } else {
                    FulfillmentService::new(self.db.clone())
                        .persist_posted_delivery(&mut self.delivery, self.occurred_at, executor)
                        .await?;
                }
            },
            PostingStep::Task => {
                complete_fulfillment_task(
                    self.db,
                    self.task_identity,
                    FulfillmentTaskObject::Delivery(&self.delivery),
                    &self.actor,
                    executor,
                )
                .await?;
            },
            PostingStep::AcceptanceTask => {
                ensure_customer_acceptance_task(
                    self.db,
                    self.task_identity,
                    &self.delivery.sales_order_id,
                    CustomerAcceptanceTaskReason::DeliveryAvailable,
                    executor,
                )
                .await?;
            },
            PostingStep::Audit => self.persist_posting_audit(executor).await?,
        }
        Ok(())
    }
}

/// 外部来源已确认直发数量、真实明细和包裹归属后，用该观察的时间与责任人，在应用观察的同一事务过账、完成履约任务并安排验收待办。
/// 不生成自有库存流水或客户验收事实。非直发、采购或预付门槛不符、任务不授权或任一写入失败时拒绝。
pub async fn post_observed_supplier_delivery(
    db: &Database,
    task_identity: &Arc<AuthorizationService>,
    delivery: Delivery,
    lines: Vec<DeliveryLine>,
    source_shipped_at: Option<Instant>,
    actor: AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if delivery.delivery_type != DeliveryType::SupplierDirect {
        return Err(Error::BusinessLogicError("供应商观察不能替代仓库出库".into()));
    }
    ensure_tracking_sources(db, &delivery, &lines, executor).await?;
    let count = lines.len();
    let mut posting = DeliveryPosting {
        db,
        task_identity,
        delivery_id: DeliveryId::new(delivery.base.id.clone()),
        delivery,
        lines,
        occurred_at: Instant::now(),
        actor,
        source_shipped_at: Some(source_shipped_at),
    };
    execute_posting(&mut posting, DeliveryType::SupplierDirect, count, executor).await
}

impl DeliveryPosting<'_> {
    async fn persist_posting_audit(&self, executor: &mut dyn Executor) -> Result<()> {
        let (actor, action) = if self.source_shipped_at.is_some() {
            (
                AuditActor::new(
                    "system:supplier-order-tracking".into(),
                    "system:supplier-order-tracking".into(),
                    AccountKind::Admin,
                ),
                "supplier_fulfillment.observe_delivery",
            )
        } else {
            (self.actor.clone(), "delivery.post")
        };

        let audit = actor.resource_log(action, "delivery", self.delivery_id.to_string())?;
        persist_log(self.db, &audit, executor).await?;
        Ok(())
    }

    async fn post_warehouse_line(&self, index: usize, executor: &mut dyn Executor) -> Result<()> {
        let line = &self.lines[index];
        post_warehouse_ship_line(
            self.db,
            executor,
            &WarehouseShipmentLine {
                delivery_id: self.delivery.base.id.clone(),
                line_id: line.base.id.clone(),
                sales_order_line_id: line.sales_order_line_id.clone(),
                stock_reservation_id: line.stock_reservation_id.clone(),
                warehouse_id: self.delivery.warehouse_id.clone(),
                quantity: line.quantity,
            },
            self.occurred_at,
            self.actor.id(),
        )
        .await?;
        Ok(())
    }

    async fn ensure_supplier_gate(&self, executor: &mut dyn Executor) -> Result<()> {
        let po = supplier_purchase_source(&self.delivery)?;
        let po = self
            .db
            .purchase_orders()
            .find_by_id(po.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购单不存在".to_string()))?;
        ensure_po_fulfillable(&po)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    /// 实例具有非零大小，执行器身份断言不得依赖零大小地址。
    use persistence_core::ReadExecutor as TestExecutor;

    use super::*;
    struct RecordingSteps {
        executor: usize,
        calls: Vec<PostingStep>,
        fail_at: Option<PostingStep>,
    }
    #[async_trait]
    impl PostingSteps for RecordingSteps {
        async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(step);
            if self.fail_at == Some(step) {
                return Err(Error::BusinessLogicError("原发货步骤失败".into()));
            }
            Ok(())
        }
    }
    fn recording(executor: &mut TestExecutor) -> RecordingSteps {
        RecordingSteps { executor: executor as *mut TestExecutor as usize, calls: vec![], fail_at: None }
    }

    // 生产入口先逐行完成仓发库存，再写发货状态、执行任务、验收任务与审计。
    #[tokio::test]
    async fn warehouse_posting_preserves_each_line_and_following_task_order() {
        let mut executor = TestExecutor::default();
        let mut steps = recording(&mut executor);
        execute_posting(&mut steps, DeliveryType::WarehouseShip, 2, &mut executor).await.unwrap();
        assert_eq!(
            steps.calls,
            [
                PostingStep::WarehouseLine(0),
                PostingStep::WarehouseLine(1),
                PostingStep::Delivery,
                PostingStep::Task,
                PostingStep::AcceptanceTask,
                PostingStep::Audit,
            ]
        );
    }

    // 供应商直发只校验采购可履约状态，不调用任何自有库存行步骤。
    #[tokio::test]
    async fn supplier_direct_posting_checks_purchase_before_delivery_without_stock() {
        let mut executor = TestExecutor::default();
        let mut steps = recording(&mut executor);
        execute_posting(&mut steps, DeliveryType::SupplierDirect, 2, &mut executor).await.unwrap();
        assert_eq!(
            steps.calls,
            [
                PostingStep::SupplierGate,
                PostingStep::Delivery,
                PostingStep::Task,
                PostingStep::AcceptanceTask,
                PostingStep::Audit
            ]
        );
    }

    // 任一当前行或后续步骤失败，保留原错，不访问下一行，不推进后续任务与审计。
    #[tokio::test]
    async fn posting_failure_stops_before_next_line_and_later_domains() {
        for (delivery_type, expected) in [
            (
                DeliveryType::WarehouseShip,
                vec![
                    PostingStep::WarehouseLine(0),
                    PostingStep::WarehouseLine(1),
                    PostingStep::WarehouseLine(2),
                    PostingStep::Delivery,
                    PostingStep::Task,
                    PostingStep::AcceptanceTask,
                    PostingStep::Audit,
                ],
            ),
            (
                DeliveryType::SupplierDirect,
                vec![
                    PostingStep::SupplierGate,
                    PostingStep::Delivery,
                    PostingStep::Task,
                    PostingStep::AcceptanceTask,
                    PostingStep::Audit,
                ],
            ),
        ] {
            for (index, step) in expected.iter().copied().enumerate() {
                let mut executor = TestExecutor::default();
                let mut steps = recording(&mut executor);
                steps.fail_at = Some(step);

                let error = execute_posting(&mut steps, delivery_type, 3, &mut executor).await.unwrap_err();
                assert!(matches!(error, Error::BusinessLogicError(message) if message == "原发货步骤失败"));
                assert_eq!(steps.calls, expected[..=index]);
            }
        }
    }
}
