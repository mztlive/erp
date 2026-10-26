//! 发货过账根事务：逐行库存或采购门槛、发货状态、任务与审计。

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::AuditActorLogs;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_core::ids::DeliveryId;
use erp_fulfillment::dto::{DeliveryView, PostDeliveryRequest};
use erp_fulfillment::entity::fulfillment::{Delivery, DeliveryLine, DeliveryType, DeliveryUpdate};
use erp_fulfillment::service::FulfillmentService;
use erp_fulfillment::service::delivery_posting::supplier_purchase_source;
use erp_inventory::service::fulfillment::delivery::{WarehouseShipmentLine, post_warehouse_ship_line};
use erp_procurement::repository::PurchaseOrderExt;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::FulfillmentProcess;
use super::customer_acceptance::task::{CustomerAcceptanceTaskReason, ensure_customer_acceptance_task};
use super::delivery_tracking::ensure_tracking_sources;
use super::purchase_context::{ensure_po_fulfillable, ensure_prepay_gate};
use super::task::{FulfillmentTaskObject, complete_fulfillment_task};
use crate::audit::persist_log;
use crate::{Error, Result};

impl FulfillmentProcess {
    /// 过账发货（草稿 → 已发货；§8.2 第 2 条跨集合事务）。
    ///
    /// 仓发在同一事务内：校验预占归属（预占必须属于本销售明细且数量充足）、
    /// 消耗预占（含预占流水）、追加库存减少流水、更新库存余额、迁移发货单
    /// 状态、写审计。供应商直发不写自有库存流水，只做 `PREPAY` 门槛校验
    /// （§8.1.5）后迁移状态。重复过账由状态守卫（仅草稿）与流水唯一索引双重
    /// 防护。
    ///
    /// # 参数
    /// * `id` - 发货单主键
    /// * `req` - 最终草稿与期望版本
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后的发货单视图。
    ///
    /// # 错误
    /// * `NotFound` - 发货单/预占/余额不存在
    /// * `ConflictError` - 状态不允许过账或重复过账
    /// * `BusinessLogicError` - 预占归属不符、数量不足或门槛未满足
    /// * `OutcomeUnknown` - 提交结果无法确认
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
        let expected_version = req.version;
        let tracking_entries = req.tracking_entries;
        let actor = actor.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let posted = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (delivery, lines) = FulfillmentService::new(db.clone())
                        .prepare_delivery_posting(
                            &delivery_id,
                            expected_version,
                            DeliveryUpdate { tracking_entries },
                            executor,
                        )
                        .await?;
                    ensure_tracking_sources(&db, &delivery, &lines, executor).await?;
                    let occurred_at = Instant::now();
                    let delivery_type = delivery.delivery_type;
                    let line_count = lines.len();
                    let mut posting = DeliveryPosting {
                        db: &db,
                        delivery_id,
                        delivery,
                        lines,
                        occurred_at,
                        actor,
                        source_shipped_at: None,
                    };
                    execute_posting(&mut posting, delivery_type, line_count, executor).await?;
                    Ok::<Delivery, Error>(posting.delivery)
                })
            })
            .await?;
        Ok(posted.into())
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
    /// 在调用方执行器中执行当前步骤，首个失败立即返回。
    async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()>;
}

/// 实际生产顺序：当前行库存完成后才推进下一行，之后推进履约和任务。
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
                    FulfillmentTaskObject::Delivery(&self.delivery),
                    self.actor.id(),
                    executor,
                )
                .await?;
            },
            PostingStep::AcceptanceTask => {
                ensure_customer_acceptance_task(
                    self.db,
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

/// 在来源观察应用事务内复用直发的采购门槛、任务完成与验收待办。
/// # 参数
/// 数据库、已经校验全部数量与包裹关联的发货单、真实行、来源发货时间、责任人及同一执行器。
/// # 返回
/// 原子完成直发过账，不创建自有库存和客户验收事实。
/// # 错误
/// 非直发、采购/预付/任务权限门槛未满足或任一步写入失败时拒绝。
pub(crate) async fn post_observed_supplier_delivery(
    db: &Database,
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
        ensure_prepay_gate(self.db, executor, &po).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 实例具有非零大小，执行器身份断言不得依赖零大小地址。
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
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

    /// 生产入口先逐行完成仓发库存，再写发货状态、执行任务、验收任务与审计。
    #[tokio::test]
    async fn warehouse_posting_preserves_each_line_and_following_task_order() {
        let mut executor = TestExecutor { _identity: 1 };
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

    /// 供应商直发只执行采购门槛，不调用任何自有库存行步骤。
    #[tokio::test]
    async fn supplier_direct_posting_checks_purchase_before_delivery_without_stock() {
        let mut executor = TestExecutor { _identity: 1 };
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

    /// 任一当前行或后续步骤失败，保留原错，不访问下一行，不推进后续任务与审计。
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
                let mut executor = TestExecutor { _identity: 1 };
                let mut steps = recording(&mut executor);
                steps.fail_at = Some(step);
                let error = execute_posting(&mut steps, delivery_type, 3, &mut executor).await.unwrap_err();
                assert!(matches!(error, Error::BusinessLogicError(message) if message == "原发货步骤失败"));
                assert_eq!(steps.calls, expected[..=index]);
            }
        }
    }
}
