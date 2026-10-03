//! 从当前不可变提交的强工作副本父链证明已保存目标身份。

use erp_sales::entity::sales_review::SalesChangeOrder;
use erp_sales::repository::{SalesOrderExt, SalesReviewExt};
use persistence_core::Executor;

use crate::{Error, Result};

/// 在同一授权执行器内读取当前提交来源的内容身份。
///
/// # 参数
/// * `db` - 销售集合数据库
/// * `change` - 已沿来源销售单授权的变更单
/// * `executor` - 详情授权事务执行器
///
/// # 返回
/// 未提交返回空；已有提交时返回被冻结工作副本的内容身份。
///
/// # 错误
/// 提交、工作副本、原单或冻结版本不匹配时拒绝。
pub(super) async fn submitted_content_hash(
    db: &mongodb::Database,
    change: &SalesChangeOrder,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(submission_id) = &change.current_submission_id else { return Ok(None) };
    let submission = db
        .sales_change_submissions()
        .find_by_id(submission_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售变更当前提交不存在".into()))?;
    let copy = db
        .sales_order_working_copies()
        .find_by_id(submission.working_copy_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售变更提交来源内容不存在".into()))?;
    Ok(Some(submission.saved_target_content_hash(change, &copy)?.to_string()))
}
