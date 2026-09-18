//! 回款冲正最终过账：退货状态、财务逆向核销、成功审计及销售进度共享事务。

mod approval;
mod commit;
mod context;
mod create;
mod sales_refresh;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::ids::SalesOrderId;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::service::receivable::receipt_reversal::{
    load_posted_receipt, receipt_allocation_sales_order_ids, reverse_receipt_allocations,
};
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::ReceiptReversalView;
use erp_returns::entity::returns::ReceiptReversal;
use erp_returns::service::ReturnsService;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use sales_refresh::{AffectedSales, refresh_affected_sales};

use crate::Result;

/// 回款冲正最终动作的跨域流程；客户端直接过账入口仍由原 Handler 拒绝。
pub struct ReceiptReversalProcess {
    db: Database,
}

impl ReceiptReversalProcess {
    /// 绑定组合根数据库；审批运行时负责最终动作授权。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 最终通过过账（§8.3-3 事务不变量）。
    ///
    /// 作为合同 `on_final_approve`，仅 `IN_APPROVAL` 可进入过账。同一事务内：
    /// 按原回款核销分配反向写入 `REVERSE` 分配并原子冲减子账已核销进度；
    /// 原回款迁移为已冲正；冲正单迁移为已过账。任一校验失败整体回滚。
    ///
    /// # 参数
    /// * `id` - 冲正单 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回过账后冲正单视图。
    ///
    /// # 错误
    /// * `NotFound` - 冲正单或原回款不存在
    /// * `ConflictError` - 非审批中
    /// * `BusinessLogicError` - 累计冲正超原回款、重复过账或超额冲减
    pub async fn post_receipt_reversal(&self, id: &str, actor: &AuditActor) -> Result<ReceiptReversalView> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor_owned = actor.clone();
        let reversal_id = id.to_string();
        let detail_id = reversal_id.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    apply_receipt_reversal_final_post(&db, &reversal_id, &actor_owned, executor).await
                })
            })
            .await?;
        ReturnsReadService::new(self.db.clone())
            .receipt_reversal_detail(&detail_id)
            .await
            .map_err(crate::Error::from)
    }

    /// 复用审批最终通过的会话，禁止创建内层事务或延后销售刷新。
    pub async fn post_receipt_reversal_apply(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        apply_receipt_reversal_final_post(&self.db, id, actor, executor).await
    }
}

/// 在最终通过事务内执行过账副作用并写回冲正单。
///
/// # 错误
/// 非审批中、原回款不存在或仓储失败时返回错误。
/// 顺序保持原 final_post：冲正状态闸门、原回款闸门、累计额度，然后开始写入。
async fn apply_receipt_reversal_final_post(
    db: &Database,
    id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let reversal = ReturnsService::prepare_receipt_reversal_post(db, id, executor).await?;
    let receipt = load_posted_receipt(db, &reversal.original_customer_receipt_id, executor).await?;
    ReturnsService::validate_receipt_reversal_amount(db, &reversal, receipt.amount, executor).await?;
    let mut posting = DatabasePosting { db, reversal, receipt, actor };
    post(&mut posting, executor).await
}

/// 最终写入的最小能力；生产适配器和纯替身执行相同函数。
#[async_trait]
trait ReceiptReversalPosting: Send {
    async fn reverse_finance(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn post_reversal(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn refresh_sales(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

/// 审计之后才重新读取受影响分配，任何错误停止后继写入并向审批事务传播。
async fn post(port: &mut impl ReceiptReversalPosting, executor: &mut dyn Executor) -> Result<()> {
    port.reverse_finance(executor).await?;
    port.post_reversal(executor).await?;
    port.audit(executor).await?;
    port.refresh_sales(executor).await
}

struct DatabasePosting<'a> {
    db: &'a Database,
    reversal: ReceiptReversal,
    receipt: CustomerReceipt,
    actor: &'a AuditActor,
}

#[async_trait]
impl ReceiptReversalPosting for DatabasePosting<'_> {
    async fn reverse_finance(&mut self, executor: &mut dyn Executor) -> Result<()> {
        reverse_receipt_allocations(
            self.db,
            self.receipt.clone(),
            self.reversal.amount,
            self.reversal.occurred_at,
            self.actor.id(),
            executor,
        )
        .await?;
        Ok(())
    }
    async fn post_reversal(&mut self, executor: &mut dyn Executor) -> Result<()> {
        Ok(ReturnsService::persist_posted_receipt_reversal(self.db, &mut self.reversal, executor).await?)
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let audit = self.actor.clone().resource_log(
            "receipt_reversal.post",
            "receipt_reversal",
            self.reversal.base.id.clone(),
        )?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }
    async fn refresh_sales(&mut self, executor: &mut dyn Executor) -> Result<()> {
        refresh_affected_sales(self, executor).await
    }
}

#[async_trait]
impl AffectedSales for DatabasePosting<'_> {
    async fn load_sales(&mut self, executor: &mut dyn Executor) -> Result<Vec<SalesOrderId>> {
        Ok(receipt_allocation_sales_order_ids(self.db, &self.reversal.original_customer_receipt_id, executor)
            .await?)
    }

    async fn refresh_sale(
        &mut self,
        sales_order_id: &SalesOrderId,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        crate::order_to_cash::progress::update_sales_order_money_progress(
            self.db,
            executor,
            sales_order_id,
            self.actor.id().to_string(),
            None,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use persistence_core::Executor;

    use super::{ReceiptReversalPosting, post};
    use crate::{Error, Result};

    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    #[derive(Default)]
    struct RecordingPosting {
        events: Vec<&'static str>,
        executors: Vec<usize>,
        fail: Option<&'static str>,
    }
    impl RecordingPosting {
        fn record(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.events.push(step);
            self.executors.push(executor as *mut dyn Executor as *mut () as usize);
            if self.fail == Some(step) {
                return Err(Error::ConflictError(step.to_string()));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl ReceiptReversalPosting for RecordingPosting {
        async fn reverse_finance(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("reverse_finance", e)
        }
        async fn post_reversal(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("post_reversal", e)
        }
        async fn audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
        async fn refresh_sales(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("query_allocations_and_refresh_sales", e)
        }
    }
    #[tokio::test]
    async fn reversal_finance_status_audit_and_sales_keep_one_executor_and_original_order() {
        let mut port = RecordingPosting::default();
        let mut executor = TestExecutor { _identity: 1 };
        let expected = &mut executor as *mut TestExecutor as usize;
        post(&mut port, &mut executor).await.unwrap();
        assert_eq!(
            port.events,
            ["reverse_finance", "post_reversal", "audit", "query_allocations_and_refresh_sales"]
        );
        assert_eq!(port.executors, vec![expected; 4]);
    }
    #[tokio::test]
    async fn each_failed_step_preserves_error_and_prevents_later_writes() {
        let steps = ["reverse_finance", "post_reversal", "audit", "query_allocations_and_refresh_sales"];
        for (index, step) in steps.iter().enumerate() {
            let mut port = RecordingPosting { fail: Some(step), ..Default::default() };
            let mut executor = TestExecutor { _identity: 1 };
            let error = post(&mut port, &mut executor).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == *step));
            assert_eq!(port.events, steps[..=index]);
        }
    }
}
