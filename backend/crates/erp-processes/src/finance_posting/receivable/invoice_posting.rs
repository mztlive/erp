//! 销项发票过账的跨域步骤，全程共用根事务执行器。

use application_core::{AuditActor, CommandReceipt};
use async_trait::async_trait;
use erp_audit::{
    AuditAction, AuditFact, AuditField, AuditFieldKind, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult,
};
use erp_core::ids::{ReceivableAccountId, SalesOrderId, WorkItemId};
use erp_finance::entity::receivable::sales_invoice_allocation_plan::SalesInvoiceAllocationLine;
use erp_finance::entity::receivable::{Invoice, ReceivableAccount};
use erp_finance::service::command_receipt::FinanceCommandReceiptService;
use erp_finance::service::receivable::invoice_posting::persist_sales_invoice_allocations;
use mongodb::Database;
use persistence_core::Executor;

use super::invoice_task::{self, SalesInvoiceTaskChange};
use crate::audit::persist_log;
use crate::{Error, Result};

/// 扇出分片大小：同事务内按片顺序推进，首错短路。
pub(crate) const FANOUT_SHARD: usize = 50;

/// 扇出分片大小的对外别名，供红冲路径复用同一分片口径。
pub(crate) const FANOUT_CHUNK: usize = FANOUT_SHARD;

/// 去重并稳定排序，保持原失败短路顺序。
///
/// # 参数
/// * `ids` - 待分组的身份集合
///
/// # 返回
/// 返回排序去重后的身份集合。
fn dedup_sorted(mut ids: Vec<String>) -> Vec<String> {
    ids.sort();
    ids.dedup();
    ids
}

/// 根发票命令已校验的不可变过账意图。
pub(super) struct InvoicePostingInput<'a> {
    pub work_item_id: &'a WorkItemId,
    pub expected_task_version: u64,
    pub plan_lines: &'a [SalesInvoiceAllocationLine],
    pub actor: &'a AuditActor,
    pub action: &'static str,
    pub command_receipt: Option<&'a CommandReceipt>,
}

/// 在调用方已有事务中，按既定顺序把已校验发票写入财务、任务、销售和审计。
///
/// 授权、重复检查和提交恢复由根命令负责。本函数保持原步骤顺序，首次失败后不再继续写入。
///
/// # 参数
/// * `db` - 数据库。
/// * `invoice` - 待过账发票；执行活动成功时写回开票申请 ID。
/// * `input` - 根命令已校验的过账意图。
/// * `executor` - 调用方事务执行器，各步骤共用。
///
/// # 返回
/// 全部步骤成功时无返回值。
///
/// # 错误
/// 执行活动、财务事实、审计、任务同步、销售进度或命令回执任一步失败时返回该错误。
pub(super) async fn post_invoice_apply(
    db: &Database,
    invoice: &mut Invoice,
    input: InvoicePostingInput<'_>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut steps = MongoInvoicePosting {
        db,
        invoice,
        input,
        accounts: Vec::new(),
        account_ids: Vec::new(),
        audit_event_id: None,
    };
    execute_posting(&mut steps, executor).await
}

/// 发票过账步骤；每步都使用调用方传入的同一执行器。
#[async_trait]
trait InvoicePostingSteps: Send {
    /// 根命令是否携带需要在最后写入的收据。
    fn has_command_receipt(&self) -> bool;
    /// 记录开票执行活动；失败则不再写财务事实。
    async fn record_execution(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 写入销项发票的财务分配事实。
    async fn persist_finance(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 在财务事实写入后记录过账审计。
    async fn write_posting_audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 按受影响应收子账同步开票任务。
    async fn synchronize_tasks(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 刷新相关销售单的资金进度。
    async fn update_sales_progress(&mut self, executor: &mut dyn Executor) -> Result<()>;
    /// 有根收据时保存命令回执；无收据时由编排跳过。
    async fn write_command_receipt(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

/// 按执行活动、财务事实、审计、任务、销售进度和根回执的顺序推进，首错即停。
async fn execute_posting(steps: &mut impl InvoicePostingSteps, executor: &mut dyn Executor) -> Result<()> {
    steps.record_execution(executor).await?;
    steps.persist_finance(executor).await?;
    steps.write_posting_audit(executor).await?;
    steps.synchronize_tasks(executor).await?;
    steps.update_sales_progress(executor).await?;
    if steps.has_command_receipt() {
        steps.write_command_receipt(executor).await?;
    }
    Ok(())
}

/// 正式过账路径使用的仓储与服务适配。
struct MongoInvoicePosting<'a> {
    db: &'a Database,
    invoice: &'a mut Invoice,
    input: InvoicePostingInput<'a>,
    accounts: Vec<ReceivableAccount>,
    account_ids: Vec<String>,
    audit_event_id: Option<String>,
}

#[async_trait]
impl InvoicePostingSteps for MongoInvoicePosting<'_> {
    fn has_command_receipt(&self) -> bool {
        self.input.command_receipt.is_some()
    }

    async fn record_execution(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let account_ids =
            self.input.plan_lines.iter().map(|line| line.receivable_account_id.clone()).collect::<Vec<_>>();
        let request_id = invoice_task::record_invoice_execution(
            self.db,
            invoice_task::InvoiceExecutionInput {
                work_item_id: self.input.work_item_id,
                expected_task_version: self.input.expected_task_version,
                party_id: &self.invoice.party_id,
                account_ids: &account_ids,
                invoice_amount: self.invoice.gross_amount,
            },
            self.input.actor,
            executor,
        )
        .await?;
        self.invoice.sales_invoice_request_id = Some(request_id);
        Ok(())
    }

    async fn persist_finance(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let (accounts, account_ids) = persist_sales_invoice_allocations(
            self.db,
            self.invoice,
            self.input.plan_lines,
            self.input.actor.id(),
            executor,
        )
        .await?;
        self.accounts = accounts;
        self.account_ids = account_ids;
        Ok(())
    }

    async fn write_posting_audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let action = AuditAction {
            code: self.input.action,
            resource_type: "invoice",
            label: "登记并过账销项发票",
            version: 1,
            allowed_fields: &[
                AuditField { code: "gross_amount", label: "含税金额", kind: AuditFieldKind::Amount },
                AuditField { code: "net_amount", label: "不含税金额", kind: AuditFieldKind::Amount },
                AuditField { code: "tax_amount", label: "税额", kind: AuditFieldKind::Amount },
            ],
        };
        let context = BusinessEventContext::new(self.input.actor.clone(), action)?
            .with_command_id(self.input.command_receipt.map(|command| command.id().to_string()))?;
        let audit = context.log(BusinessEventContent {
            target_id: self.invoice.base.id.clone(),
            target_number: Some(self.invoice.invoice_no.clone()),
            result: BusinessEventResult::Succeeded,
            field_changes: vec![],
            facts: vec![
                AuditFact {
                    field: "gross_amount".to_string(),
                    value: AuditValue::Amount { value: self.invoice.gross_amount },
                },
                AuditFact {
                    field: "net_amount".to_string(),
                    value: AuditValue::Amount { value: self.invoice.net_amount },
                },
                AuditFact {
                    field: "tax_amount".to_string(),
                    value: AuditValue::Amount { value: self.invoice.tax_amount },
                },
            ],
        })?;
        self.audit_event_id = Some(audit.base.id.clone());
        persist_log(self.db, &audit, executor).await?;
        Ok(())
    }

    async fn synchronize_tasks(&mut self, executor: &mut dyn Executor) -> Result<()> {
        // 同一事务执行器不可并发共享（`&mut Executor` 与会话排他），
        // 按片顺序短路即分组后的可靠形态，保持原失败语义。
        for chunk in dedup_sorted(self.account_ids.clone()).chunks(FANOUT_SHARD) {
            for account_id in chunk {
                invoice_task::sync_sales_invoice_task(
                    self.db,
                    &ReceivableAccountId::new(account_id.clone()),
                    SalesInvoiceTaskChange::InvoicePosted,
                    executor,
                )
                .await?;
            }
        }
        Ok(())
    }

    async fn update_sales_progress(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let ids =
            dedup_sorted(self.accounts.iter().map(|account| account.sales_order_id.to_string()).collect());
        for chunk in ids.chunks(FANOUT_SHARD) {
            for id in chunk {
                crate::order_to_cash::progress::update_sales_order_money_progress(
                    self.db,
                    executor,
                    &SalesOrderId::new(id.clone()),
                    self.input.actor.id().to_string(),
                    None,
                )
                .await?;
            }
        }
        Ok(())
    }

    /// 缺少已生成的审计事件 ID 时返回内部错误，不写回执。
    ///
    /// # 参数
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 无命令回执时不写。有回执且已有审计事件时，回执保存成功。
    ///
    /// # 错误
    /// 有回执但 `audit_event_id` 尚未生成时返回 `Internal`，且不写回执。回执保存失败时返回对应错误。
    async fn write_command_receipt(&mut self, executor: &mut dyn Executor) -> Result<()> {
        if let Some(receipt) = self.input.command_receipt {
            let event_id = self
                .audit_event_id
                .clone()
                .ok_or_else(|| Error::Internal("发票过账缺少业务事件引用".to_string()))?;
            FinanceCommandReceiptService::new(self.db.clone())
                .save_resource(receipt, self.invoice.base.id.clone(), event_id, executor)
                .await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Error;

    // 非零大小保证执行器地址能够区分不同实例。
    struct TestExecutor {
        _identity: u8,
    }

    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    struct RecordedPosting {
        calls: Vec<(&'static str, usize)>,
        fail_at: Option<&'static str>,
        command_receipt: bool,
    }

    impl RecordedPosting {
        fn record(&mut self, step: &'static str, executor: &mut dyn Executor) -> Result<()> {
            self.calls.push((step, executor as *mut dyn Executor as *mut () as usize));
            if self.fail_at == Some(step) {
                return Err(Error::ConflictError(step.to_string()));
            }
            Ok(())
        }
    }

    #[async_trait]
    impl InvoicePostingSteps for RecordedPosting {
        fn has_command_receipt(&self) -> bool {
            self.command_receipt
        }
        async fn record_execution(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("execution", e)
        }
        async fn persist_finance(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("finance", e)
        }
        async fn write_posting_audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
        async fn synchronize_tasks(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("tasks", e)
        }
        async fn update_sales_progress(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("sales", e)
        }
        async fn write_command_receipt(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("receipt", e)
        }
    }

    #[tokio::test]
    async fn invoice_post_and_commit_keep_original_order_and_the_same_executor() {
        for command_receipt in [false, true] {
            let mut executor = TestExecutor { _identity: 1 };
            let expected_executor = &mut executor as *mut TestExecutor as usize;
            let mut steps = RecordedPosting { calls: Vec::new(), fail_at: None, command_receipt };
            execute_posting(&mut steps, &mut executor).await.unwrap();
            let mut expected = vec!["execution", "finance", "audit", "tasks", "sales"];
            if command_receipt {
                expected.push("receipt");
            }
            assert_eq!(
                steps.calls,
                expected.into_iter().map(|step| (step, expected_executor)).collect::<Vec<_>>()
            );
        }
    }

    #[tokio::test]
    async fn invoice_posting_stops_at_each_failure_and_preserves_the_original_error() {
        let sequence = ["execution", "finance", "audit", "tasks", "sales", "receipt"];
        for command_receipt in [false, true] {
            let count = if command_receipt { sequence.len() } else { sequence.len() - 1 };
            for (index, fail_at) in sequence[..count].iter().enumerate() {
                let mut steps =
                    RecordedPosting { calls: Vec::new(), fail_at: Some(fail_at), command_receipt };
                let error =
                    execute_posting(&mut steps, &mut TestExecutor { _identity: 1 }).await.unwrap_err();
                assert!(matches!(error, Error::ConflictError(message) if message == *fail_at));
                assert_eq!(steps.calls.iter().map(|(step, _)| *step).collect::<Vec<_>>(), sequence[..=index]);
            }
        }
    }
}
