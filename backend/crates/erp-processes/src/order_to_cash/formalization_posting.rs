//! 在调用方事务内固定首次形式化的过账顺序。

use async_trait::async_trait;
use erp_audit::AuditLog;
use erp_core::common::time::Instant;
use erp_core::ids::{PartyId, SalesOrderId, SalesOrderRevisionId};
use erp_core::money::Amount;
use erp_customer::CustomerExt;
use erp_finance::entity::receivable::SalesBusinessTypeFact;
use erp_finance::service::receivable::initial_account::{InitialReceivableInput, create_initial_receivable};
use erp_sales::entity::sales_order::{BusinessType, SalesOrder};
use erp_workflow::DocumentRegistryExt;
use persistence_core::Executor;

use super::formalize::{FormalizedSubmissionWrite, persist_procurement_work_items};
use crate::audit::persist_log;
use crate::{Error, Result};

/// 每个变体是既有副作用边界之一，顺序与下方生产顺序一致。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PostingStep {
    RevalidateProcurement,
    ProcurementTasks,
    RegisterDocument,
    SalesRevision,
    SynchronizeProcurement,
    SalesSubmission,
    Receivable,
    Audit,
}

#[async_trait]
trait PostingSteps: Send {
    /// 只用调用方执行器执行既有步骤之一，并在原错误处停止。
    async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()>;
}

/// 按生产顺序执行各步骤；失败注入的纯测试替身也走这条顺序。
async fn execute(steps: &mut impl PostingSteps, executor: &mut dyn Executor) -> Result<()> {
    use PostingStep::*;
    for step in [
        RevalidateProcurement,
        ProcurementTasks,
        RegisterDocument,
        SalesRevision,
        SynchronizeProcurement,
        SalesSubmission,
        Receivable,
        Audit,
    ] {
        steps.apply(step, executor).await?;
    }
    Ok(())
}

struct MongoPosting<'a> {
    write: FormalizedSubmissionWrite,
    audit: &'a AuditLog,
}

#[async_trait]
impl PostingSteps for MongoPosting<'_> {
    /// 按步骤在同一执行器写入采购、销售、应收与审计，任一步失败即停止。
    ///
    /// # 参数
    /// * `step` - 当前形式化过账步骤。
    /// * `executor` - 调用方执行器；本实现不另开事务。
    ///
    /// # 返回
    /// 该步写入完成。无采购计划时跳过重验、采购任务和采购同步；业务单据不存在时跳过登记。
    ///
    /// # 错误
    /// 当前步骤失败时返回原错误，后续步骤不再执行。应收步骤在客户不存在时返回 `NotFound`。
    async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()> {
        use PostingStep::*;
        let write = &mut self.write;
        match step {
            RevalidateProcurement => {
                if let Some(plan) = write.procurement.as_ref() {
                    crate::procure_to_pay::responsibility::ProcurementResponsibilityProcess::new(
                        write.db.clone(),
                        write.rbac.clone(),
                    )
                    .revalidate_plan(&plan.inputs, &plan.resolution, executor)
                    .await?;
                }
            },
            ProcurementTasks => {
                if write.procurement.is_some() {
                    persist_procurement_work_items(&write.db, &write.procurement_items, executor).await?;
                }
            },
            RegisterDocument => {
                if let Some(mut document) =
                    write.db.business_documents().find_by_id(&write.order_id, executor).await?
                {
                    document.formalize(write.now);
                    write.db.business_documents().update(&mut document, executor).await?;
                }
            },
            SalesRevision => {
                crate::business_ownership::ensure_attribution(&write.db, &write.order, executor).await?;
                erp_sales::service::sales_order::formalize::persist_revision(
                    &write.db,
                    &mut write.order,
                    &write.aggregate,
                    executor,
                )
                .await?;
            },
            SynchronizeProcurement => {
                if write.procurement.is_some() {
                    crate::procure_to_pay::sync_procurement_tasks_for_sales_order(
                        &write.db,
                        &SalesOrderId::new(write.order_id.clone()),
                        executor,
                    )
                    .await?;
                }
            },
            SalesSubmission => {
                erp_sales::service::sales_order::formalize::persist_submission(
                    &write.db,
                    &mut write.submission,
                    executor,
                )
                .await?;
            },
            Receivable => {
                post_receivable(write, executor).await?;
            },
            Audit => {
                persist_log(&write.db, self.audit, executor).await?;
            },
        }
        Ok(())
    }
}

/// 在销售生效的原事务中读取客户企业身份，形成对客户的应收。
async fn post_receivable(write: &FormalizedSubmissionWrite, executor: &mut dyn Executor) -> Result<()> {
    let order = &write.order;
    let customer = write
        .db
        .customer_accounts()
        .find_by_id(order.customer_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单客户不存在，无法形成应收".into()))?;
    create_initial_receivable(
        &write.db,
        receivable_input(
            order,
            customer.party_id,
            write.aggregate.revision.base.id.clone().into(),
            write.aggregate.revision.gross_amount,
            write.now,
        ),
        executor,
    )
    .await?;
    Ok(())
}

/// 将销售事实与独立读取的客户主体映射为财务输入。
fn receivable_input(
    order: &SalesOrder,
    customer_party: PartyId,
    revision_id: SalesOrderRevisionId,
    gross_total: Amount,
    posted_at: Instant,
) -> InitialReceivableInput {
    InitialReceivableInput {
        business_type: match order.business_type {
            BusinessType::GoodsService => SalesBusinessTypeFact::GoodsService,
            BusinessType::Voucher => SalesBusinessTypeFact::Voucher,
        },
        sales_order_id: order.base.id.clone().into(),
        customer_id: order.customer_id.clone(),
        counterparty_party_id: customer_party,
        source_sales_order_revision_id: revision_id,
        gross_total,
        posted_at,
    }
}

/// 在调用方选定的事务边界内写入销售形式化及既有供给事实，并继续写入财务、任务和审计。
///
/// # 参数
/// * `write` - 已完成领域计算的销售形式化写入上下文
/// * `audit` - 本次形式化审计记录
/// * `executor` - 调用方根事务执行器；全部步骤复用，不开启其他事务
///
/// # 返回
/// 全部写入完成后返回；销售单、正式版本与冻结时间在同一写入上下文中传递。
///
/// # 错误
/// 采购责任重验、正式版本、提交或后续财务、任务、审计任一步骤失败时，保留原错误并停止执行。
pub(super) async fn post(
    write: FormalizedSubmissionWrite,
    audit: &AuditLog,
    executor: &mut dyn Executor,
) -> Result<()> {
    execute(&mut MongoPosting { write, audit }, executor).await
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::CustomerAccountId;
    use erp_sales::entity::sales_order::{OriginSystem, SalesOrderData};

    use super::*;
    struct TestExecutor {
        _identity: u8,
    }
    impl Executor for TestExecutor {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }

    struct RecordingSteps {
        calls: Vec<PostingStep>,
        executor: usize,
        fail_at: Option<PostingStep>,
    }
    #[async_trait]
    impl PostingSteps for RecordingSteps {
        async fn apply(&mut self, step: PostingStep, executor: &mut dyn Executor) -> Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.executor);
            self.calls.push(step);
            if Some(step) == self.fail_at {
                return Err(Error::ConflictError("original posting conflict".into()));
            }
            Ok(())
        }
    }
    fn expected() -> Vec<PostingStep> {
        use PostingStep::*;
        vec![
            RevalidateProcurement,
            ProcurementTasks,
            RegisterDocument,
            SalesRevision,
            SynchronizeProcurement,
            SalesSubmission,
            Receivable,
            Audit,
        ]
    }

    #[test]
    fn receivable_debtor_uses_customer_company_independently_of_our_settlement_company() {
        for business_type in [BusinessType::GoodsService, BusinessType::Voucher] {
            let order = SalesOrder::new(
                SalesOrderId::new("sale-1"),
                SalesOrderData {
                    business_org_unit_id: "org-1".into(),
                    sales_owner_user_id: "sales-1".into(),
                    order_no: "SO-1".into(),
                    business_type,
                    origin_system: OriginSystem::Erp,
                    source_identity_id: None,
                    customer_id: CustomerAccountId::new("customer-1"),
                    contract_id: None,
                    settlement_party_id: PartyId::new("our-other-company"),
                    source_status_code: None,
                },
                "sales-1",
            )
            .unwrap();
            let input = receivable_input(
                &order,
                PartyId::new("customer-company"),
                SalesOrderRevisionId::new("revision-1"),
                Amount::from_str("123.45").unwrap(),
                Instant::from_unix_secs(1_800_000_000),
            );
            assert_eq!(input.customer_id, order.customer_id);
            assert_eq!(input.counterparty_party_id.as_ref(), "customer-company");
            assert_ne!(input.counterparty_party_id, order.settlement_party_id);
            assert_eq!(input.source_sales_order_revision_id.as_ref(), "revision-1");
            assert_eq!(input.gross_total, Amount::from_str("123.45").unwrap());
        }
    }

    #[tokio::test]
    async fn formalization_preserves_all_domain_steps_and_executor() {
        let mut executor = TestExecutor { _identity: 1 };
        let identity = &mut executor as *mut TestExecutor as usize;
        let mut steps = RecordingSteps { calls: vec![], executor: identity, fail_at: None };
        execute(&mut steps, &mut executor).await.unwrap();
        assert_eq!(steps.calls, expected());
    }

    #[tokio::test]
    async fn every_failure_stops_later_domains_and_preserves_original_error() {
        for (index, step) in expected().into_iter().enumerate() {
            let mut executor = TestExecutor { _identity: 1 };
            let identity = &mut executor as *mut TestExecutor as usize;
            let mut steps = RecordingSteps { calls: vec![], executor: identity, fail_at: Some(step) };
            let error = execute(&mut steps, &mut executor).await.unwrap_err();
            assert!(matches!(error,Error::ConflictError(ref text) if text=="original posting conflict"));
            assert_eq!(steps.calls, expected()[..=index]);
        }
    }
}
