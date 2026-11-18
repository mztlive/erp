//! 销售变更驳回保留基准生效版本及不可变提交内容。

use mongodb::Database;
use persistence_core::Executor;

use crate::repository::SalesReviewExt;
use crate::{Error, Result};

/// 在审批事务内驳回销售变更及其精确冻结提交。
///
/// # 参数
/// * `db` - 销售集合所在数据库。
/// * `id` - 变更单身份。
/// * `subject_version` - 被驳回实例的提交序号。
/// * `actor_id` - 已通过审批决定授权的操作人。
/// * `executor` - 审批决定的同一事务执行器。
/// # 返回
/// 变更与提交标记已驳回，原销售生效事实保持不变。
/// # 错误
/// 提交身份不匹配、非审批中或仓储写入失败时由审批事务回滚。
pub async fn reject_change(
    db: &Database,
    id: &str,
    subject_version: u32,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut change = db
        .sales_change_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售变更单不存在".into()))?;
    let submission_id = change.required_current_submission_id()?.clone();
    let mut submission = db
        .sales_change_submissions()
        .find_by_id(&submission_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售变更单缺少当前审批提交".into()))?;
    if submission.submission_no != subject_version || submission.sales_change_order_id.as_ref() != id {
        return Err(Error::ConflictError("销售变更审批提交版本已变化".into()));
    }

    change.reject_approval(actor_id)?;
    submission.reject(actor_id)?;
    db.sales_change_submissions().update(&mut submission, executor).await?;
    db.sales_change_orders().update(&mut change, executor).await?;
    Ok(())
}
