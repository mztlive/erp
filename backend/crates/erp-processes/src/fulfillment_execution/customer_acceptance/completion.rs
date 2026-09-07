//! 验收事实写入后的销售进度、责任任务与审计顺序；三条入口共用生产编排。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use entities::fulfillment::{AcceptanceProgress, CustomerAcceptance};
use erp_audit::{AuditActorLogs, AuditExt, CommandReceiptServiceExt};
use erp_sales::entity::sales_order::FulfillmentProgress;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;
use services::fulfillment::{
    ensure_customer_acceptance_task, persist_customer_acceptance_task_after_posting,
    CustomerAcceptanceTaskReason, FulfillmentService,
};
use services::Result;

/// 用例确定任务来源和审计种类，避免给普通 post 新增幂等回放。
pub(super) enum CompletionKind {
    Commit {
        task: WorkItem,
        receipt: CommandReceipt,
    },
    Post {
        task: WorkItem,
    },
    Reverse {
        original_id: String,
        receipt: CommandReceipt,
    },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CompletionMode {
    Commit,
    Post,
    Reverse,
}
impl CompletionKind {
    fn mode(&self) -> CompletionMode {
        match self {
            Self::Commit { .. } => CompletionMode::Commit,
            Self::Post { .. } => CompletionMode::Post,
            Self::Reverse { .. } => CompletionMode::Reverse,
        }
    }
}

/// 在刚写入的履约事实可见的同一 Executor 内完成原跨域副作用。
pub(super) async fn complete_acceptance(
    db: &Database,
    acceptance: &CustomerAcceptance,
    actor: &AuditActor,
    kind: CompletionKind,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mode = kind.mode();
    let mut port = DatabaseCompletion {
        db,
        acceptance,
        actor,
        kind,
    };
    finish(&mut port, mode, executor).await
}

#[async_trait]
trait AcceptanceCompletion: Send {
    async fn refresh_sales(&mut self, executor: &mut dyn Executor) -> Result<bool>;
    async fn persist_task(&mut self, remaining: bool, executor: &mut dyn Executor) -> Result<()>;
    async fn business_audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
    async fn command_receipt(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

async fn finish(
    port: &mut impl AcceptanceCompletion,
    mode: CompletionMode,
    executor: &mut dyn Executor,
) -> Result<()> {
    let remaining = port.refresh_sales(executor).await?;
    port.persist_task(remaining, executor).await?;
    if mode != CompletionMode::Commit {
        port.business_audit(executor).await?;
    }
    if mode != CompletionMode::Post {
        port.command_receipt(executor).await?;
    }
    Ok(())
}

struct DatabaseCompletion<'a> {
    db: &'a Database,
    acceptance: &'a CustomerAcceptance,
    actor: &'a AuditActor,
    kind: CompletionKind,
}

#[async_trait]
impl AcceptanceCompletion for DatabaseCompletion<'_> {
    async fn refresh_sales(&mut self, executor: &mut dyn Executor) -> Result<bool> {
        let progress = FulfillmentService::load_customer_acceptance_progress(
            self.db,
            executor,
            &self.acceptance.sales_order_id,
        )
        .await?;
        apply_projection(
            &mut SalesProgressWriter {
                db: self.db,
                acceptance: self.acceptance,
                actor: self.actor,
            },
            progress,
            executor,
        )
        .await
    }

    async fn persist_task(&mut self, remaining: bool, executor: &mut dyn Executor) -> Result<()> {
        match &self.kind {
            CompletionKind::Commit { task, .. } | CompletionKind::Post { task } => {
                persist_customer_acceptance_task_after_posting(
                    self.db,
                    task.clone(),
                    self.actor.id(),
                    remaining,
                    executor,
                )
                .await
            }
            CompletionKind::Reverse { .. } if remaining => {
                ensure_customer_acceptance_task(
                    self.db,
                    &self.acceptance.sales_order_id,
                    CustomerAcceptanceTaskReason::ReopenedByReversal,
                    executor,
                )
                .await?;
                Ok(())
            }
            CompletionKind::Reverse { .. } => Ok(()),
        }
    }
    async fn business_audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let (action, resource_id) = match &self.kind {
            CompletionKind::Post { .. } => ("customer_acceptance.post", self.acceptance.base.id.clone()),
            CompletionKind::Reverse { original_id, .. } => {
                ("customer_acceptance.reverse", original_id.clone())
            }
            CompletionKind::Commit { .. } => return Ok(()),
        };
        let audit = self
            .actor
            .clone()
            .resource_log(action, "customer_acceptance", resource_id)?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }
    async fn command_receipt(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let receipt = match &self.kind {
            CompletionKind::Commit { receipt, .. } | CompletionKind::Reverse { receipt, .. } => receipt,
            CompletionKind::Post { .. } => return Ok(()),
        };
        let audit = receipt.audit(self.actor.clone(), self.acceptance.base.id.clone())?;
        self.db.audit_logs().create(&audit, executor).await?;
        Ok(())
    }
}

/// 可派生投影才触发销售金额刷新；None 必须连余额读取都跳过。
#[async_trait]
trait AcceptanceSalesProgress: Send {
    async fn write(&mut self, fulfillment: FulfillmentProgress, executor: &mut dyn Executor) -> Result<()>;
}
async fn apply_projection(
    writer: &mut impl AcceptanceSalesProgress,
    progress: Option<AcceptanceProgress>,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let Some(progress) = progress else {
        return Ok(false);
    };
    writer.write(progress.progress, executor).await?;
    Ok(progress.has_remaining_eligible)
}
struct SalesProgressWriter<'a> {
    db: &'a Database,
    acceptance: &'a CustomerAcceptance,
    actor: &'a AuditActor,
}
#[async_trait]
impl AcceptanceSalesProgress for SalesProgressWriter<'_> {
    async fn write(&mut self, fulfillment: FulfillmentProgress, executor: &mut dyn Executor) -> Result<()> {
        crate::order_to_cash::progress::update_sales_order_money_progress(
            self.db,
            executor,
            &self.acceptance.sales_order_id,
            self.actor.id().to_string(),
            Some(fulfillment),
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_projection, finish, AcceptanceCompletion, AcceptanceSalesProgress, CompletionMode};
    use async_trait::async_trait;
    use entities::fulfillment::AcceptanceProgress;
    use erp_sales::entity::sales_order::FulfillmentProgress;
    use persistence_core::Executor;
    use services::{Error, Result};

    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    #[derive(Default)]
    struct RecordingCompletion {
        events: Vec<&'static str>,
        executors: Vec<usize>,
        fail: Option<&'static str>,
        remaining: bool,
        task_remaining: Vec<bool>,
    }
    impl RecordingCompletion {
        fn record(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.events.push(step);
            self.executors
                .push(executor as *mut dyn Executor as *mut () as usize);
            if self.fail == Some(step) {
                return Err(Error::ConflictError(step.to_string()));
            }
            Ok(())
        }
    }
    #[async_trait]
    impl AcceptanceCompletion for RecordingCompletion {
        async fn refresh_sales(&mut self, e: &mut dyn Executor) -> Result<bool> {
            self.record("sales_progress", e)?;
            Ok(self.remaining)
        }
        async fn persist_task(&mut self, remaining: bool, e: &mut dyn Executor) -> Result<()> {
            self.task_remaining.push(remaining);
            self.record("task", e)
        }
        async fn business_audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("business_audit", e)
        }
        async fn command_receipt(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("command_receipt", e)
        }
    }
    fn cases() -> [(CompletionMode, Vec<&'static str>); 3] {
        [
            (
                CompletionMode::Commit,
                vec!["sales_progress", "task", "command_receipt"],
            ),
            (
                CompletionMode::Post,
                vec!["sales_progress", "task", "business_audit"],
            ),
            (
                CompletionMode::Reverse,
                vec!["sales_progress", "task", "business_audit", "command_receipt"],
            ),
        ]
    }
    #[tokio::test]
    async fn three_commands_keep_original_audit_order_and_one_executor() {
        for (mode, steps) in cases() {
            for remaining in [false, true] {
                let mut port = RecordingCompletion {
                    remaining,
                    ..Default::default()
                };
                let mut executor = TestExecutor { _identity: 1 };
                let expected = &mut executor as *mut TestExecutor as usize;
                finish(&mut port, mode, &mut executor).await.unwrap();
                assert_eq!(port.events, steps);
                assert_eq!(port.executors, vec![expected; steps.len()]);
                assert_eq!(port.task_remaining, vec![remaining]);
            }
        }
    }
    #[tokio::test]
    async fn each_failure_stops_remaining_task_and_audit_operations() {
        for (mode, steps) in cases() {
            for (index, step) in steps.iter().enumerate() {
                let mut port = RecordingCompletion {
                    fail: Some(step),
                    ..Default::default()
                };
                let mut executor = TestExecutor { _identity: 1 };
                let error = finish(&mut port, mode, &mut executor).await.unwrap_err();
                assert!(matches!(error, Error::ConflictError(message) if message == *step));
                assert_eq!(port.events, steps[..=index]);
            }
        }
    }
    #[derive(Default)]
    struct RecordingSales {
        progress: Vec<FulfillmentProgress>,
        executors: Vec<usize>,
    }
    #[async_trait]
    impl AcceptanceSalesProgress for RecordingSales {
        async fn write(&mut self, progress: FulfillmentProgress, executor: &mut dyn Executor) -> Result<()> {
            self.progress.push(progress);
            self.executors
                .push(executor as *mut dyn Executor as *mut () as usize);
            Ok(())
        }
    }
    #[tokio::test]
    async fn absent_projection_skips_sales_and_present_projection_keeps_remaining_fact() {
        let mut sales = RecordingSales::default();
        let mut executor = TestExecutor { _identity: 1 };
        let expected = &mut executor as *mut TestExecutor as usize;
        assert!(!apply_projection(&mut sales, None, &mut executor).await.unwrap());
        assert!(sales.progress.is_empty());
        for remaining in [false, true] {
            let progress = AcceptanceProgress {
                progress: FulfillmentProgress::PartiallyFulfilled,
                has_remaining_eligible: remaining,
            };
            assert_eq!(
                apply_projection(&mut sales, Some(progress), &mut executor)
                    .await
                    .unwrap(),
                remaining
            );
        }
        assert_eq!(sales.progress, vec![FulfillmentProgress::PartiallyFulfilled; 2]);
        assert_eq!(sales.executors, vec![expected; 2]);
    }
}
