//! 采购变更启动的单次只读恢复。

use erp_procurement::dto::purchase_order::PurchaseChangeSubmitResult;
use erp_procurement::entity::purchase_order::PurchaseChangeOrder;
use erp_procurement::repository::PurchaseOrderExt;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::service::document_registry::find_approval_binding;
use mongodb::Database;
use persistence_core::Executor;

use super::{
    purchase_change_order_object_readable, purchase_change_order_subject_ref,
    purchase_change_responsible_org_id, replay_purchase_change_start_with_executor, require_frozen_binding,
};
use crate::{Error, Result};

/// 每次恢复沿用同一执行器依序读取变更、采购、销售和冻结启动身份。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `change_order_id` - 变更单主键。
/// * `subject_version` - 原提交冻结的主题版本。
/// * `idempotency_key` - 原启动幂等键。
/// * `actor_id` - 原提交人。
/// * `executor` - 本次恢复事务执行器。
///
/// # 返回
/// 查证启动后返回精确冻结提交结果；未查到启动回执时返回空。
///
/// # 错误
/// 关联事实缺失、授权或主题绑定不符、仓储失败时返回原错误。
pub(super) async fn replay_change_start(
    db: &Database,
    change_order_id: &str,
    subject_version: u32,
    idempotency_key: &str,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseChangeSubmitResult>> {
    let change = db
        .purchase_change_orders()
        .find_by_id(change_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
    let order = db
        .purchase_orders()
        .find_by_id(&change.purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("原采购单不存在".to_string()))?;
    let sales_order = db
        .sales_orders()
        .find_by_id(&order.sales_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
    let organization_id = purchase_change_responsible_org_id(&sales_order)?;
    let _ = purchase_change_order_object_readable(&organization_id, actor_id)?;
    let binding = find_approval_binding(db, change_order_id, executor).await.map_err(crate::Error::from)?;
    let binding = require_frozen_binding(binding.as_ref())?;
    let subject = purchase_change_order_subject_ref(change_order_id)?;
    let Some(_) = replay_purchase_change_start_with_executor(
        db,
        &subject,
        subject_version,
        idempotency_key,
        binding,
        actor_id,
        executor,
    )
    .await?
    else {
        return Ok(None);
    };
    recovered_change_result(db, &change, change_order_id, subject_version, executor).await
}

/// 只有查证到启动收据后才读取并验证精确冻结提交。
async fn recovered_change_result(
    db: &Database,
    change: &PurchaseChangeOrder,
    change_order_id: &str,
    subject_version: u32,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseChangeSubmitResult>> {
    if change.approval_subject_version != subject_version {
        return Err(Error::ConflictError("采购变更启动结果与业务主题版本不一致".to_string()));
    }
    let submission_id = change
        .current_submission_id
        .as_ref()
        .ok_or_else(|| Error::ConflictError("采购变更启动结果缺少冻结提交".to_string()))?;
    let submission = db
        .purchase_change_submissions()
        .find_by_id(submission_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购变更冻结提交不存在".to_string()))?;
    if submission.purchase_change_order_id.as_ref() != change_order_id {
        return Err(Error::ConflictError("采购变更冻结提交与业务对象不一致".to_string()));
    }
    Ok(Some(PurchaseChangeSubmitResult {
        change_id: change.base.id.clone(),
        submission_id: submission.base.id.clone(),
        submission_no: submission.submission_no.clone(),
        status: change.stable.status.as_str().to_string(),
        lock_version: change.base.version,
        reference: format!("CS-{}", submission.submission_no),
    }))
}
