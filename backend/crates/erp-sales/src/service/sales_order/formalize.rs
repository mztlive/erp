//! 销售本域不可变正式版本构造，以及调用方事务内的写入。
use erp_core::common::time::Instant;
use erp_core::ids::{SalesOrderId, SalesOrderRevisionId, SalesOrderSubmissionId};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::sales_order::{
    FormalRevisionContext, FormalRevisionIdentities, FormalRevisionLineIdentity,
    FormalRevisionSubtypeIdentity, LineType, RevisionSource, SalesOrder, SalesOrderRevisionAggregate,
    SalesOrderSubmission, SalesOrderSubmissionLine,
};
use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 读取该销售单最新提交及其明细。
///
/// # 参数
/// * `db` - 销售集合所在数据库。
/// * `sales_order_id` - 销售单身份。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回最新提交及其明细。
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

/// 在构造聚合之前，按冻结提交行顺序分配正式版本头、公共行和子类型身份。
fn allocate_formal_revision_identities(lines: &[SalesOrderSubmissionLine]) -> FormalRevisionIdentities {
    FormalRevisionIdentities::new(
        SalesOrderRevisionId::new(next_id()),
        lines
            .iter()
            .map(|line| {
                let subtype = match line.line_type {
                    LineType::GoodsService => FormalRevisionSubtypeIdentity::goods(
                        erp_core::ids::SalesOrderGoodsServiceLineRevisionId::new(next_id()),
                    ),
                    LineType::Voucher => FormalRevisionSubtypeIdentity::voucher(
                        erp_core::ids::SalesOrderVoucherLineRevisionId::new(next_id()),
                    ),
                };
                FormalRevisionLineIdentity::new(
                    erp_core::ids::SalesOrderRevisionLineId::new(next_id()),
                    subtype,
                )
            })
            .collect(),
    )
}

/// 按业务性质构造正式版本。
///
/// 版本号固定为 1，来源固定为 ERP 审批。
///
/// # 参数
/// * `order` - 待形式化的销售单。
/// * `submission` - 用于构造版本的提交。
/// * `submission_lines` - 冻结提交行，身份按该顺序分配。
/// * `effective_at` - 生效时间。
///
/// # 返回
/// 返回尚未持久化的正式版本聚合。
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

/// 调用方完成外域准备后，按原顺序审批销售单、挂上修订，再审批不可变提交。
///
/// # 参数
/// * `order` - 待审批的稳定销售单。
/// * `submission` - 待审批的不可变提交。
/// * `aggregate` - 已构造的正式版本，只读取其修订身份。
/// * `now` - 审批时间。
/// * `actor_id` - 审批人。
///
/// # 返回
/// 内存中的销售单和提交都完成审批时返回 `Ok(())`，不写库。
///
/// # 错误
/// 销售单或提交的审批动作被拒绝时返回对应错误。
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

/// 在调用方同步采购任务之前，写入稳定销售单和正式版本。
///
/// # 参数
/// * `db` - 销售集合所在数据库。
/// * `order` - 已挂上修订的销售单。
/// * `aggregate` - 正式版本聚合。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 形式化写入成功时返回 `Ok(())`。
///
/// # 错误
/// 仓储或形式化写入失败时返回对应错误。
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

/// 在调用方同步采购任务之后，写回已审批的提交。
///
/// # 参数
/// * `db` - 销售集合所在数据库。
/// * `submission` - 已审批的提交。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 更新成功时返回 `Ok(())`。
///
/// # 错误
/// 仓储更新失败时返回对应错误。
pub async fn persist_submission(
    db: &Database,
    submission: &mut SalesOrderSubmission,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(db.sales_order_submissions().update(submission, executor).await?)
}
