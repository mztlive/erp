//! 终态驳回只撤销审批资格；草稿创建时取得的询价来源占用保留到作废。

use erp_core::ids::SalesOrderId;
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 在审批事务内驳回销售单及其精确冻结提交。
///
/// # 参数
/// * `db` - 销售集合所在数据库。
/// * `id` - 销售单身份。
/// * `subject_version` - 被驳回实例的冻结提交序号。
/// * `actor_id` - 已通过审批决定授权的操作人。
/// * `executor` - 审批决定的同一事务执行器。
/// # 返回
/// 单据退回可编辑草稿并保留已驳回审核轨，冻结提交标记已驳回。
/// # 错误
/// 冻结提交不匹配、非审批中或任一 CAS 写入失败时返回错误，调用方整体回滚。
pub async fn reject_order(
    db: &Database,
    id: &str,
    subject_version: u32,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let order_id = SalesOrderId::new(id);
    let mut order = db
        .sales_orders()
        .find_by_id(&order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".into()))?;
    let mut submission = db
        .sales_order_submissions()
        .find_latest_by_order(&order_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("销售单缺少当前审批提交".into()))?;
    if submission.submission_no != subject_version {
        return Err(Error::ConflictError("销售单审批提交版本已变化".into()));
    }
    order.reject_approval_submission(actor_id)?;
    submission.reject(actor_id)?;
    db.sales_order_submissions().update(&mut submission, executor).await?;
    db.sales_orders().update(&mut order, executor).await?;
    Ok(())
}
