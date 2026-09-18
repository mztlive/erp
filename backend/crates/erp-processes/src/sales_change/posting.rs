//! 销售变更同事务写入合同；生产与纯替身共同执行相同编排函数。

use async_trait::async_trait;
use erp_audit::AuditExt;
use erp_core::ids::ReceivableAccountId;
use erp_finance::service::receivable::sales_change::{
    SalesChangeReceivableInput, SalesChangeReceivableWrite, prepare_sales_change_receivable,
};
use erp_sales::service::sales_review::EffectiveChangeWrite;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 销售变更最终生效的最小写入能力；不得在实现内另开事务。
#[async_trait]
trait SalesChangePostingPort: Send {
    /// 写入新的销售正式版本。
    async fn revision(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 写入差额并更新卡券复核及开票任务。
    async fn receivable_and_tasks(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 在财务和任务成功之后推进变更状态。
    async fn change(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 最后写入成功审计。
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

/// 依次写入销售修订、应收差额及工作项、变更状态与审计；任何失败原样传播。
async fn post(port: &mut impl SalesChangePostingPort, executor: &mut dyn Executor) -> Result<()> {
    port.revision(executor).await?;
    port.receivable_and_tasks(executor).await?;
    port.change(executor).await?;
    port.audit(executor).await
}

/// 生产适配器持有一次性销售写入计划和组合层构造的审计。
struct DatabasePosting<'a> {
    db: &'a Database,
    write: EffectiveChangeWrite,
    delta: Option<SalesChangeReceivableWrite>,
    audit: &'a erp_audit::AuditLog,
}

#[async_trait]
impl SalesChangePostingPort for DatabasePosting<'_> {
    async fn revision(&mut self, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.write.persist_revision(self.db, executor).await?)
    }
    async fn receivable_and_tasks(&mut self, executor: &mut dyn Executor) -> Result<()> {
        if let Some(delta) = self.delta.take() {
            write_receivable_delta(self.db, delta, executor).await?;
        }
        Ok(())
    }
    async fn change(&mut self, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.write.persist_change(self.db, executor).await?)
    }
    async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.db.audit_logs().create(self.audit, executor).await?;
        Ok(())
    }
}

/// 使用传入 Executor 执行真实销售变更写入路径。
pub(super) async fn persist_effective_writes(
    db: &Database,
    write: EffectiveChangeWrite,
    delta: Option<SalesChangeReceivableWrite>,
    audit: &erp_audit::AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    post(&mut DatabasePosting { db, write, delta, audit }, executor).await
}

/// 写入应收差额分录。
///
/// # 错误
/// 仓储失败时返回错误。
async fn write_receivable_delta(
    db: &mongodb::Database,
    mut delta: SalesChangeReceivableWrite,
    executor: &mut dyn Executor,
) -> Result<()> {
    delta.persist(db, executor).await?;
    let account = delta.account();
    let account_id = ReceivableAccountId::new(account.base.id.clone());
    crate::finance_posting::receivable::invoice_task::sync_sales_invoice_task(
        db,
        &account_id,
        crate::finance_posting::receivable::invoice_task::SalesInvoiceTaskChange::ReceivableChanged,
        executor,
    )
    .await
}

/// 在原销售准备完成后读取主应收子账，保持 NoTransaction 和财务取时位置。
pub(super) async fn prepare_receivable_delta(
    db: &Database,
    write: &EffectiveChangeWrite,
    actor: &application_core::AuditActor,
) -> Result<Option<SalesChangeReceivableWrite>> {
    let fact = write.revision_fact()?;
    let business_type = match fact.business_type {
        erp_sales::entity::sales_order::BusinessType::GoodsService => {
            erp_finance::entity::receivable::SalesBusinessTypeFact::GoodsService
        },
        erp_sales::entity::sales_order::BusinessType::Voucher => {
            erp_finance::entity::receivable::SalesBusinessTypeFact::Voucher
        },
    };
    Ok(prepare_sales_change_receivable(
        db,
        SalesChangeReceivableInput {
            sales_order_id: fact.sales_order_id,
            revision_id: fact.revision_id,
            business_type,
            current_gross: fact.current_gross,
            new_gross: fact.new_gross,
            posted_at: fact.posted_at,
            updated_by: actor.id().to_string(),
        },
        &mut persistence_core::NoTransaction,
    )
    .await?)
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use persistence_core::Executor;

    use super::{SalesChangePostingPort, post};
    use crate::{Error, Result};

    // 非零大小保证执行器地址能够区分不同实例。
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
    impl SalesChangePostingPort for RecordingPosting {
        async fn revision(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("revision", executor)
        }
        async fn receivable_and_tasks(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("receivable_and_tasks", executor)
        }
        async fn change(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("change", executor)
        }
        async fn audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
            self.record("audit", executor)
        }
    }

    #[tokio::test]
    async fn formal_revision_finance_tasks_change_and_audit_share_executor_in_original_order() {
        let mut port = RecordingPosting::default();
        let mut executor = TestExecutor { _identity: 1 };
        let expected = &mut executor as *mut TestExecutor as usize;
        post(&mut port, &mut executor).await.unwrap();
        assert_eq!(port.events, ["revision", "receivable_and_tasks", "change", "audit"]);
        assert_eq!(port.executors, vec![expected; 4]);
    }

    #[tokio::test]
    async fn every_failure_keeps_error_category_and_stops_later_writes() {
        let steps = ["revision", "receivable_and_tasks", "change", "audit"];
        for (index, failed_step) in steps.iter().enumerate() {
            let mut port = RecordingPosting { fail: Some(*failed_step), ..Default::default() };
            let error = post(&mut port, &mut TestExecutor { _identity: 1 }).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(ref message) if message.as_str() == *failed_step));
            assert_eq!(port.events, steps[..=index]);
        }
    }
}
