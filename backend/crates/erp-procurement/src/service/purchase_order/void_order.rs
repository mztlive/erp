//! 采购草稿作废的目标约束、当前提交校验和状态持久化。

use persistence_core::Executor;

use crate::entity::purchase_order::{PurchaseOrder, PurchaseOrderStatus, SubmissionStatus};
use crate::repository::PurchaseOrderExt;
use crate::{Error, Result};

/// 按 ID 加载待作废采购单。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `purchase_order_id` - 采购单 ID
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回存在的采购单。
///
/// # 错误
/// 采购单不存在或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 本函数不把采购单状态转换为幂等回放语义。
pub async fn load_purchase_order(
    db: &mongodb::Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    super::shared::load_order_by_id(db, purchase_order_id, executor).await
}

/// 校验采购草稿作废目标的创建人、已作废状态、版本和草稿状态。
///
/// # 参数
/// * `created_by` - 采购单创建人 ID
/// * `current_version` - 采购单当前乐观锁版本
/// * `status` - 采购单当前状态
/// * `expected_lock_version` - 客户端期望版本
/// * `actor_id` - 当前操作人 ID
///
/// # 返回
/// 当前账号可作废且版本、状态匹配时返回 `Ok(())`。
///
/// # 错误
/// 非创建人返回不存在；已作废但无收据返回 409；其余版本或状态不匹配返回对应错误。
///
/// # 关键业务约束
/// 只有命中稳定收据的请求可以把已作废结果标记为回放。
pub fn ensure_void_target(
    created_by: &str,
    current_version: u64,
    status: PurchaseOrderStatus,
    expected_lock_version: u64,
    actor_id: &str,
) -> Result<()> {
    if created_by != actor_id {
        return Err(Error::NotFound("采购单不存在或不可作废".to_string()));
    }
    if status == PurchaseOrderStatus::Voided {
        return Err(Error::ConflictError("采购单已作废，当前请求没有匹配的作废收据".to_string()));
    }
    if current_version != expected_lock_version {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
    }
    if status != PurchaseOrderStatus::Draft {
        return Err(Error::BusinessLogicError("只有草稿状态且没有下游事实的采购单可以作废".to_string()));
    }
    Ok(())
}

/// 校验采购单当前提交仍为可作废草稿。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已通过目标校验的采购单
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 当前提交存在且仍为草稿时返回 `Ok(())`。
///
/// # 错误
/// 当前提交引用缺失、提交不存在、提交已冻结或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 已形成不可变提交的采购单禁止直接作废。
pub async fn ensure_current_submission_is_draft(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let submission_id = order
        .current_submission_id
        .as_deref()
        .ok_or_else(|| Error::BusinessLogicError("采购单缺少当前草稿提交".to_string()))?;
    let submission = db
        .purchase_order_submissions()
        .find_by_id(submission_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购草稿提交不存在".to_string()))?;
    if submission.status != SubmissionStatus::Draft {
        return Err(Error::BusinessLogicError("采购提交已冻结，不能直接作废采购单".to_string()));
    }
    Ok(())
}

/// 在调用方执行器中推进已校验采购草稿为作废状态并执行版本 CAS。
///
/// # 错误
/// 原状态不允许作废、版本冲突或仓储写入失败时保留原错误。
///
/// # 关键业务约束
/// 调用方必须先完成来源销售 guard 写入；本方法不得另开事务。
pub async fn persist_voided_order(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    order.transition(PurchaseOrderStatus::Voided, actor_id).map_err(Error::Logic)?;
    db.purchase_orders().update(order, executor).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{PurchaseOrderStatus, ensure_void_target};
    use crate::Error;

    /// 验证没有匹配收据的已作废采购单不会被标记为任意请求回放。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 已作废状态未返回 409 时测试失败。
    #[test]
    fn voided_order_without_receipt_is_conflict() {
        let result = ensure_void_target("actor-1", 5, PurchaseOrderStatus::Voided, 4, "actor-1");

        assert!(matches!(result, Err(Error::ConflictError(_))));
    }
}
