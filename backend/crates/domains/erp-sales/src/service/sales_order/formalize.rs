//! 销售本域不可变正式版本构造，以及调用方事务内的写入。
use erp_core::common::time::Instant;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::inquiry::consume as consume_inquiry;
use crate::entity::sales_order::{
    FormalRevisionContext, FormalRevisionIdentities, FormalRevisionLineIdentity,
    FormalRevisionSubtypeIdentity, LineType, RevisionSource, SalesOrder, SalesOrderRevisionAggregate,
    SalesOrderSubmission, SalesOrderSubmissionLine,
};
use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 最终审批准备正式版本时，按销售单加载最新提交及对应冻结行；不从可编辑工作副本取数。
/// 返回最新提交及其明细。
/// 无提交或仓储失败时返回错误。
///
/// 输入约定：
/// - `db`：销售集合所在数据库。
/// - `sales_order_id`：销售单身份。
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

// 物化正式销售版本之前一次分配版本头和行身份，子类型及公共行身份均按原提交行顺序对齐。
pub(in crate::service) fn allocate_formal_revision_identities(
    line_types: impl Iterator<Item = LineType>,
) -> FormalRevisionIdentities {
    FormalRevisionIdentities::new(
        SalesOrderRevisionId::new(next_id()),
        line_types
            .map(|line_type| {
                let subtype = FormalRevisionSubtypeIdentity::from_line_type(line_type, next_id());
                FormalRevisionLineIdentity::new(
                    erp_core::ids::SalesOrderRevisionLineId::new(next_id()),
                    subtype,
                )
            })
            .collect(),
    )
}

/// 首次审批生效时按销售单业务性质把冻结提交构成正式版本及子行，保留原提交快照来源。
///
/// 版本号固定为 1，来源固定为 ERP 审批。
/// 返回尚未持久化的正式版本聚合。
/// 行类型与业务性质不一致或字段缺失时返回错误。
///
/// 输入约定：
/// - `order`：待形式化的销售单。
/// - `submission`：用于构造版本的提交。
/// - `submission_lines`：冻结提交行，身份按该顺序分配。
/// - `effective_at`：生效时间。
pub fn build_revision_for_order(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
    effective_at: Instant,
) -> Result<SalesOrderRevisionAggregate> {
    SalesOrderRevisionAggregate::from_sales_order_submission(
        allocate_formal_revision_identities(submission_lines.iter().map(|line| line.line_type)),
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

/// 外域准备妥当后，在内存中先批准销售单并挂接正式修订，再批准原不可变提交；此步不写库。
/// 结果：内存中的销售单和提交都完成审批时返回 `Ok(())`，不写库。
/// 失败条件：销售单或提交的审批动作被拒绝时返回对应错误。
///
/// 输入约定：
/// - `order`：待审批的稳定销售单。
/// - `submission`：待审批的不可变提交。
/// - `aggregate`：已构造的正式版本，只读取其修订身份。
/// - `now`：审批时间。
/// - `actor_id`：审批人。
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

/// 同步采购任务之前先在原事务消耗询价占用（如有），再保存首次生效的销售主记录、正式版本及其行。
/// 结果：形式化写入成功时返回 `Ok(())`。
/// 失败条件：仓储或形式化写入失败时返回对应错误。
///
/// 输入约定：
/// - `db`：销售集合所在数据库。
/// - `order`：已挂上修订的销售单。
/// - `aggregate`：正式版本聚合。
/// - `executor`：调用方事务执行器。
pub async fn persist_revision(
    db: &Database,
    order: &mut SalesOrder,
    aggregate: &SalesOrderRevisionAggregate,
    executor: &mut dyn Executor,
) -> Result<()> {
    consume_inquiry(db, &order.base.id, executor).await?;
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

/// 调用方同步采购任务成功后，沿用原事务写回本轮提交的最终审批状态，不重建冻结内容。
/// 失败条件：仓储更新失败时返回对应错误。
///
/// 输入约定：
/// - `db`：销售集合所在数据库。
/// - `submission`：已审批的提交。
/// - `executor`：调用方事务执行器。
pub async fn persist_submission(
    db: &Database,
    submission: &mut SalesOrderSubmission,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(db.sales_order_submissions().update(submission, executor).await?)
}
