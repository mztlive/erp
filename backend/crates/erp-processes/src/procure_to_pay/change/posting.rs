//! 采购变更生效真实步骤，统一使用调用方执行器；失败不得推进后继写入。
use async_trait::async_trait;
use erp_audit::AuditLog;
use erp_finance::service::payable::purchase_change::sync_purchase_subject;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::service::purchase_order::allocation_maintenance::{
    PreparedSalesAllocations, persist_current_sales_allocations,
};
use erp_procurement::service::purchase_order::change::EffectiveChangeWrite;
use erp_sales::repository::SalesOrderExt;
use mongodb::Database;
use persistence_core::Executor;

use super::super::allocation_maintenance::prepare_current_sales_allocations;
use super::super::payment_subject::revalidate_payment_subject;
use super::super::procurement_task_sync::sync_procurement_tasks_for_sales_order;
use super::effect::EffectiveChangePosting;
use crate::audit::persist_log;
use crate::{Error, Result};

/// 原事务中的真实跨域操作；没有成本构造或成本写入步骤，因为原差额成本恒空。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum EffectStep {
    SalesGuard,
    PrepareAllocations,
    Revision,
    Allocations,
    CurrentOrder,
    ProcurementTasks,
    Payable,
    Submission,
    Change,
    Audit,
}

/// 生产适配器与失败替身共用的真实写步骤合同。
#[async_trait]
trait EffectSteps: Send {
    /// 非零差额才需要写入应付。
    fn has_payable_change(&self) -> bool;

    /// 执行单步并直接传播原错误，执行器必须保持调用方的同一借用。
    async fn apply(&mut self, step: EffectStep, executor: &mut dyn Executor) -> Result<()>;
}

/// 逐步执行原持久化次序；每一步完成后才进入下一步。
async fn execute(steps: &mut impl EffectSteps, executor: &mut dyn Executor) -> Result<()> {
    use EffectStep::*;
    for step in [
        SalesGuard,
        PrepareAllocations,
        Revision,
        Allocations,
        CurrentOrder,
        ProcurementTasks,
        Payable,
        Submission,
        Change,
        Audit,
    ] {
        if matches!(step, Payable) && !steps.has_payable_change() {
            continue;
        }
        steps.apply(step, executor).await?;
    }
    Ok(())
}

/// 持有独立采购/财务计划及本次生成的分配；不获取独立事务。
struct MongoEffect<'a> {
    db: &'a Database,
    write: EffectiveChangePosting,
    audit: AuditLog,
    actor_id: &'a str,
    allocations: Option<PreparedSalesAllocations>,
}

#[async_trait]
impl EffectSteps for MongoEffect<'_> {
    fn has_payable_change(&self) -> bool {
        self.write.payable.is_some()
    }

    async fn apply(&mut self, step: EffectStep, executor: &mut dyn Executor) -> Result<()> {
        use EffectStep::*;
        let purchase = &mut self.write.purchase;
        match step {
            SalesGuard => {
                revalidate_payment_subject(self.db, &purchase.revision.payment_subject, executor).await?;
                purchase.order.sales_order_revision_id =
                    purchase.revision.payment_subject.sales_order_revision_id.clone();
                advance_source_sales_procurement_guard(self.db, &purchase.order, self.actor_id, executor)
                    .await?
            },
            PrepareAllocations => {
                self.allocations = Some(
                    prepare_current_sales_allocations(
                        self.db,
                        &purchase.order,
                        &mut purchase.revision_lines,
                        executor,
                    )
                    .await?,
                );
            },
            Revision => purchase.persist_revision(self.db, executor).await?,
            Allocations => {
                let allocations = self
                    .allocations
                    .as_ref()
                    .ok_or_else(|| Error::Internal("采购变更生效步骤缺少已准备分配".into()))?;
                persist_current_sales_allocations(self.db, allocations, executor).await?;
            },
            CurrentOrder => {
                persist_current_purchase(self.db, purchase, self.actor_id, executor).await?;
            },
            ProcurementTasks => {
                sync_procurement_tasks_for_sales_order(self.db, &purchase.order.sales_order_id, executor)
                    .await?
            },
            Payable => {
                if let Some(payable) = self.write.payable.as_ref() {
                    payable.persist(self.db, executor).await?;
                }
            },
            Submission => purchase.persist_approved_submission(self.db, executor).await?,
            Change => purchase.persist_change(self.db, executor).await?,
            Audit => {
                persist_log(self.db, &self.audit, executor).await?;
            },
        }
        Ok(())
    }
}

/// 正式采购版本切换与应付主体同步共用调用方执行器，零金额变更也必须推进依据。
async fn persist_current_purchase(
    db: &Database,
    purchase: &mut EffectiveChangeWrite,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    purchase.persist_current_order(db, actor_id, executor).await?;
    sync_purchase_subject(
        db,
        &purchase.order.base.id.clone().into(),
        &purchase.revision.base.id.clone().into(),
        purchase.revision.payment_subject.settlement_party_id.as_ref(),
        &purchase.revision.payment_subject.settlement_party_name,
        executor,
    )
    .await?;
    Ok(())
}

/// 事务内写入生效修订、指针、差额与变更单。
///
/// 先推进来源销售 guard，再按当前销售版本重建 allocation，最后切换采购
/// 当前版本并同步任务。
///
/// # 参数
/// * `db` - 采购、销售与财务所在数据库。
/// * `write` - 已准备的生效过账计划。
/// * `audit` - 成功路径上的审计日志。
/// * `actor_id` - 推进来源销售 guard 的操作人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 返回仓储写入后采购单最新乐观锁版本。
///
/// # 错误
/// 来源销售缺失、任何 CAS 或仓储写入失败时停止后续步骤并返回原错误。
pub(super) async fn persist_effective_writes(
    db: &Database,
    write: EffectiveChangePosting,
    audit: AuditLog,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<u64> {
    let mut steps = MongoEffect { db, write, audit, actor_id, allocations: None };
    execute(&mut steps, executor).await?;
    Ok(steps.write.purchase.order.base.version)
}

/// 在采购变更生效事务内推进来源销售单的采购串行化 guard。
///
/// 必须先通过销售单 `id + version` CAS 推进 `procurement_guard_version`，
/// 后续才能重算采购覆盖，不得在事务外预读后仅写采购版本。
///
/// # 错误
/// 来源销售单不存在、guard 溢出、乐观锁或瞬态事务冲突时返回原错误。
async fn advance_source_sales_procurement_guard(
    db: &Database,
    order: &PurchaseOrder,
    actor_id: &str,
    session: &mut dyn Executor,
) -> Result<()> {
    let mut sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    if sales_order.stable.current_revision_id.as_deref() != Some(order.sales_order_revision_id.as_ref()) {
        return Err(Error::ConflictError("销售来源版本在采购变更审批期间已变化，请重新提交采购变更".into()));
    }
    sales_order.advance_procurement_guard(actor_id)?;
    db.sales_orders().update(&mut sales_order, session).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    struct TestExecutor {
        identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct RecordingSteps {
        calls: Vec<EffectStep>,
        identity: usize,
        fail_at: Option<EffectStep>,
        has_payable_change: bool,
    }
    #[async_trait]
    impl EffectSteps for RecordingSteps {
        fn has_payable_change(&self) -> bool {
            self.has_payable_change
        }

        async fn apply(&mut self, step: EffectStep, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.identity);
            self.calls.push(step);
            if Some(step) == self.fail_at {
                return Err(Error::ConflictError("original purchase change conflict".into()));
            }
            Ok(())
        }
    }
    fn expected() -> Vec<EffectStep> {
        use EffectStep::*;
        vec![
            SalesGuard,
            PrepareAllocations,
            Revision,
            Allocations,
            CurrentOrder,
            ProcurementTasks,
            Payable,
            Submission,
            Change,
            Audit,
        ]
    }
    #[tokio::test]
    async fn effective_change_uses_same_executor_and_original_write_order_without_cost_step() {
        let mut executor = TestExecutor { identity: 1 };
        assert_eq!(executor.identity, 1);
        let identity = &mut executor as *mut TestExecutor as usize;
        let mut steps =
            RecordingSteps { calls: Vec::new(), identity, fail_at: None, has_payable_change: true };
        execute(&mut steps, &mut executor).await.unwrap();
        assert_eq!(steps.calls, expected());
    }
    #[tokio::test]
    async fn every_failure_stops_later_effects_and_preserves_original_error() {
        for (index, step) in expected().into_iter().enumerate() {
            let mut executor = TestExecutor { identity: 1 };
            let identity = &mut executor as *mut TestExecutor as usize;
            let mut steps =
                RecordingSteps { calls: Vec::new(), identity, fail_at: Some(step), has_payable_change: true };
            let error = execute(&mut steps, &mut executor).await.unwrap_err();
            assert!(
                matches!(error, Error::ConflictError(message) if message == "original purchase change conflict")
            );
            assert_eq!(steps.calls, expected()[..=index]);
        }
    }

    /// 零差额生效只跳过差额分录，正式版本及主体依据继续同步。
    #[tokio::test]
    async fn zero_delta_skips_delta_without_skipping_current_subject_effects() {
        let mut executor = TestExecutor { identity: 1 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let mut steps =
            RecordingSteps { calls: Vec::new(), identity, fail_at: None, has_payable_change: false };
        execute(&mut steps, &mut executor).await.unwrap();
        let expected =
            expected().into_iter().filter(|step| !matches!(step, EffectStep::Payable)).collect::<Vec<_>>();
        assert_eq!(steps.calls, expected);
    }
}
