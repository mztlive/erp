//! 销售变更提交、撤回和生效的状态规则，不接收审批运行时类型。

use crate::entity::sales_review::{SalesChangeOrder, SalesChangeOrderStatus};
use crate::{Error, Result};

/// 提交并启动：进入 `IN_APPROVAL`。
///
/// 版本权威来源是提交记录 `submission_no`，本方法不改写该编号。
///
/// # 参数
/// * `order` - 待提交变更单
/// * `submission_id` - 本次冻结提交
/// * `target_content_hash` - 目标内容指纹
/// * `updated_by` - 提交人
///
/// # 错误
/// 状态不允许或指纹非法时返回冲突。
pub fn start_sales_change_approval(
    order: &mut SalesChangeOrder,
    submission_id: erp_core::ids::SalesChangeSubmissionId,
    target_content_hash: impl Into<String>,
    updated_by: &str,
) -> Result<()> {
    Ok(order.start_approval(submission_id, target_content_hash, updated_by)?)
}

/// 撤回审批：回到 `DRAFT`，且提交号不回退。
///
/// # 参数
/// * `order` - 审批中的变更单
/// * `updated_by` - 操作人
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_sales_change_to_draft(order: &mut SalesChangeOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_effective(order: &SalesChangeOrder) -> Result<()> {
    if order.stable.status != SalesChangeOrderStatus::InApproval {
        return Err(Error::ConflictError("只有审批中的销售变更单可以由最终通过动作生效".to_string()));
    }
    Ok(())
}

/// 将已按审批动作迁移的销售变更状态写入原 CAS；本接口不启动事务。
///
/// # 错误
/// 销售集合更新失败原样返回，跨域工作流与审计由调用方原子回滚。
pub async fn persist_cancelled_change(
    db: &mongodb::Database,
    change: &mut SalesChangeOrder,
    executor: &mut dyn persistence_core::Executor,
) -> Result<()> {
    use crate::repository::SalesReviewExt;
    db.sales_change_orders().update(change, executor).await?;
    Ok(())
}
