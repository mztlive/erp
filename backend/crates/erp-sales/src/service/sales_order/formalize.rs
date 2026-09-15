//! Sales-owned immutable formal revision construction and transaction-local writes.
use erp_core::common::time::Instant;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_order::{
    FormalRevisionContext, FormalRevisionIdentities, FormalRevisionLineIdentity,
    FormalRevisionSubtypeIdentity, RevisionSource, SalesOrder, SalesOrderRevisionAggregate,
    SalesOrderSubmission, SalesOrderSubmissionLine,
};
use crate::repository::SalesOrderExt;
use crate::{Error, Result};
/// 读取该销售单最新提交及其明细。
///
/// # 错误
/// 无提交或仓储失败时返回错误。
pub async fn load_latest_submission(
    db: &Database,
    sales_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<(SalesOrderSubmission, Vec<SalesOrderSubmissionLine>)> {
    let order_id = SalesOrderId::new(sales_order_id);
    let submission = db
        .sales_order_submissions()
        .find_latest_by_order(&order_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售单没有可形式化的提交".to_string()))?;
    let submission_id = SalesOrderSubmissionId::new(submission.base.id.clone());
    let lines =
        db.sales_order_submission_lines().list_lines_by_submissions(&[submission_id], executor).await?;
    Ok((submission, lines))
}

/// Allocate revision and subtype identities in frozen submission-line order before building the aggregate.
fn allocate_formal_revision_identities(lines: &[SalesOrderSubmissionLine]) -> FormalRevisionIdentities {
    FormalRevisionIdentities::new(
        SalesOrderRevisionId::new(next_id()),
        lines
            .iter()
            .map(|line| {
                FormalRevisionLineIdentity::new(
                    erp_core::ids::SalesOrderRevisionLineId::new(next_id()),
                    FormalRevisionSubtypeIdentity::from_line_type(line.line_type, next_id()),
                )
            })
            .collect(),
    )
}

/// 按业务性质构造正式版本。
///
/// # 错误
/// 行类型与业务性质不一致或字段缺失时返回错误。
pub fn build_revision_for_order(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
    effective_at: Instant,
) -> Result<SalesOrderRevisionAggregate> {
    SalesOrderRevisionAggregate::from_sales_order_submission(
        allocate_formal_revision_identities(submission_lines),
        FormalRevisionContext::new(
            1,
            RevisionSource::ErpApproval,
            order.stable.current_revision_id.clone().map(Into::into),
            order.business_type,
            effective_at,
        ),
        submission,
        submission_lines,
    )
    .map_err(Error::Logic)
}

/// Apply approval to the stable order and immutable submission after the caller prepared other-domain tasks.
///
/// The original order approval, revision attachment and submission approval order is retained.
pub fn approve_submission(
    order: &mut SalesOrder,
    submission: &mut SalesOrderSubmission,
    aggregate: &SalesOrderRevisionAggregate,
    now: Instant,
    actor_id: &str,
) -> Result<()> {
    order.approve(now, actor_id)?;
    order.attach_revision(aggregate.revision.base.id.clone(), actor_id);
    submission.approve(actor_id)?;
    Ok(())
}

/// Persist the stable order and formal revision before the caller synchronizes procurement tasks.
pub async fn persist_revision(
    db: &Database,
    order: &mut SalesOrder,
    aggregate: &SalesOrderRevisionAggregate,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(db
        .sales_order()
        .formalize_submission(
            order,
            &aggregate.revision,
            &aggregate.lines,
            &aggregate.goods_lines,
            &aggregate.voucher_lines,
            executor,
        )
        .await?)
}

/// Persist the approved submission after the caller's procurement task synchronization step.
pub async fn persist_submission(
    db: &Database,
    submission: &mut SalesOrderSubmission,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(db.sales_order_submissions().update(submission, executor).await?)
}
