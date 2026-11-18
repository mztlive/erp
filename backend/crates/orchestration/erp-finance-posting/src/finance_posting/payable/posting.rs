//! 付款过账跨域步骤；统一传递调用方 Executor 并在首个失败处停止。
use application_core::AuditActor;
use async_trait::async_trait;
use erp_audit::execution::persist_log;
use erp_audit::{AuditFact, AuditValue, BusinessEventContent, BusinessEventContext, BusinessEventResult};
use erp_finance::entity::payable::{PendingPaymentAllocation, SupplierPayment};
use erp_finance::service::payable::{
    PaymentSettlement, PaymentSettlementFacts, finish_supplier_payment, settle_supplier_payment_with_facts,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};
// 在调用方事务内写入付款核销、应付余额与审计。
//
// 数据面职责归位（FIN-E02/FIN-R05）：分录/子账事实一次批量装载并去重，核销净额、逐分录开放余额、连续序号与分配实体构造由
// [`erp_finance::entity::payable::PaymentAllocationLedger`] 完成；子账进度按账户聚合后批量条件更新，分配行批量插入。
//
// `payment` 为待过账的供应商付款；`pending` 为本次付款的待核销分配；`facts` 为已装载的付款结算事实；`audit` 为过账业务事件上下文；`session` 为调用方事务执行器，
// 各步骤共用。余额、付款与审计全部写入成功时无返回值。付款状态、供应商、应付开放余额、分配金额或仓储写入不合法时返回错误。
pub(super) async fn post_supplier_payment(
    db: &Database,
    payment: &mut SupplierPayment,
    pending: &[PendingPaymentAllocation],
    facts: PaymentSettlementFacts<'_>,
    audit: &BusinessEventContext,
    actor: &AuditActor,
    session: &mut dyn Executor,
) -> Result<()> {
    let mut steps = MongoPaymentPosting { db, payment, pending, facts, audit, actor, settlement: None };
    execute_posting(&mut steps, session).await
}

/// 根事务的付款步骤合同；实现只执行本步骤，不创建新事务。
#[async_trait]
trait PaymentPostingSteps: Send {
    // 付款核销的首个写入步骤，沿根事务执行器更新应付余额。
    async fn settle_accounts(&mut self, executor: &mut dyn Executor) -> Result<()>;
    // 余额更新后，同事务写入付款状态与本次核销分配。
    async fn persist_payment(&mut self, executor: &mut dyn Executor) -> Result<()>;
    // 付款与分配写入成功后，才在原事务保存本次过账审计。
    async fn write_audit(&mut self, executor: &mut dyn Executor) -> Result<()>;
}

// 保持余额更新、付款及分配写入、审计顺序；所有步骤使用同一 Executor。
async fn execute_posting(steps: &mut impl PaymentPostingSteps, executor: &mut dyn Executor) -> Result<()> {
    steps.settle_accounts(executor).await?;
    steps.persist_payment(executor).await?;
    steps.write_audit(executor).await
}

struct MongoPaymentPosting<'a> {
    db: &'a Database,
    payment: &'a mut SupplierPayment,
    pending: &'a [PendingPaymentAllocation],
    facts: PaymentSettlementFacts<'a>,
    audit: &'a BusinessEventContext,
    actor: &'a AuditActor,
    settlement: Option<PaymentSettlement>,
}

#[async_trait]
impl PaymentPostingSteps for MongoPaymentPosting<'_> {
    // 复用付款任务事实执行财务领域原核销规则及条件写入。
    //
    // 沿用调用方的执行器。核销结果留在 `settlement`，供后续步骤读取。核销规则或条件写入失败时返回对应错误，且不留下核销结果。
    async fn settle_accounts(&mut self, executor: &mut dyn Executor) -> Result<()> {
        self.settlement = Some(
            settle_supplier_payment_with_facts(
                self.db,
                self.payment,
                self.pending,
                self.facts,
                self.actor.id(),
                executor,
            )
            .await?,
        );
        Ok(())
    }

    // 在余额更新成功后登记付款状态与核销行。
    //
    // 沿用调用方的执行器。付款状态与核销分配已写入。尚无核销结果时返回 `Internal`。付款状态或核销行写入失败时返回对应错误。
    async fn persist_payment(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let settlement = self
            .settlement
            .as_ref()
            .ok_or_else(|| Error::Internal("付款过账缺少应付核销结果".to_string()))?;
        finish_supplier_payment(self.db, self.payment, self.pending, settlement, executor).await?;
        Ok(())
    }

    // 为成功过账记录最后一条业务审计。
    //
    // 沿用调用方的执行器。成功过账的审计已写入。审计内容构造或日志保存失败时返回对应错误。
    async fn write_audit(&mut self, executor: &mut dyn Executor) -> Result<()> {
        let audit = self.audit.log(BusinessEventContent {
            target_id: self.payment.base.id.clone(),
            target_number: Some(self.payment.payment_no.clone()),
            result: BusinessEventResult::Succeeded,
            field_changes: vec![],
            facts: vec![AuditFact {
                field: "amount".to_string(),
                value: AuditValue::Amount { value: self.payment.amount },
            }],
        })?;

        persist_log(self.db, &audit, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    // 非零大小保证执行器地址能够区分不同实例。
    use persistence_core::ReadExecutor;

    use super::*;

    struct RecordedPosting {
        calls: Vec<(&'static str, usize)>,
        fail_at: Option<&'static str>,
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
    impl PaymentPostingSteps for RecordedPosting {
        async fn settle_accounts(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("settle", e)
        }
        async fn persist_payment(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("payment", e)
        }
        async fn write_audit(&mut self, e: &mut dyn Executor) -> Result<()> {
            self.record("audit", e)
        }
    }

    #[tokio::test]
    async fn posting_keeps_write_order_and_the_same_executor() {
        let mut executor = ReadExecutor::default();
        let expected_executor = &mut executor as *mut ReadExecutor as usize;
        let mut steps = RecordedPosting { calls: Vec::new(), fail_at: None };
        execute_posting(&mut steps, &mut executor).await.unwrap();

        assert_eq!(
            steps.calls,
            vec![("settle", expected_executor), ("payment", expected_executor), ("audit", expected_executor)]
        );
    }

    #[tokio::test]
    async fn posting_stops_at_each_failure_and_preserves_the_original_error() {
        let sequence = ["settle", "payment", "audit"];
        for (index, fail_at) in sequence.iter().enumerate() {
            let mut steps = RecordedPosting { calls: Vec::new(), fail_at: Some(fail_at) };
            let error = execute_posting(&mut steps, &mut ReadExecutor::default()).await.unwrap_err();
            assert!(matches!(error, Error::ConflictError(message) if message == *fail_at));
            assert_eq!(steps.calls.iter().map(|(step, _)| *step).collect::<Vec<_>>(), sequence[..=index]);
        }
    }
}
