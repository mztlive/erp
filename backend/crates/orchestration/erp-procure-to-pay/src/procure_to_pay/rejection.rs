//! 审批事务内的采购终态驳回；草稿创建持有的来源及费用占用继续归属原单。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::service::purchase_order::cancel_approval::persist_cancelled_order;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::repository::{ApprovalIntegrationExt, ApprovalSubjectSnapshotRepositoryExt};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 驳回采购单并派生独立可编辑提交，保存原审批快照。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `id` - 已授权审批主体。
/// * `expected_subject_version` - 当前实例冻结的审批版本。
/// * `approval_process_instance_id` - 当前决定所属的精确审批实例。
/// * `actor` - 审批决定人。
/// * `executor` - 审批运行时的同一事务执行器。
/// # 返回
/// 状态和新编辑副本原子写入；原单草稿来源占用保留。
/// # 错误
/// 主体缺失、非审批中、冻结来源错误或写入失败时返回错误。
pub async fn reject_order_approval(
    db: &Database,
    id: &str,
    expected_subject_version: u32,
    approval_process_instance_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut order = db
        .purchase_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".into()))?;
    verify_snapshot(
        db,
        DocumentType::PurchaseOrder,
        id,
        expected_subject_version,
        approval_process_instance_id,
        executor,
    )
    .await?;
    order.reject_approval(expected_subject_version, actor.id())?;
    persist_cancelled_order(db, &mut order, executor).await?;
    let audit = actor.clone().resource_log("purchase_order.reject", "purchase_order", id.to_string())?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 驳回采购变更，原采购生效版本及冻结目标提交保持不变。
///
/// # 参数
/// * `db` - 业务数据库。
/// * `id` - 已授权变更单主体。
/// * `expected_subject_version` - 当前实例冻结的审批版本。
/// * `approval_process_instance_id` - 当前决定所属的精确审批实例。
/// * `actor` - 审批决定人。
/// * `executor` - 审批运行时的同一事务执行器。
/// # 返回
/// 变更单进入可修改的已驳回状态。
/// # 错误
/// 单据缺失、状态不允许或持久化失败时返回错误。
pub async fn reject_change_approval(
    db: &Database,
    id: &str,
    expected_subject_version: u32,
    approval_process_instance_id: &str,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let mut change = db
        .purchase_change_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更单不存在".into()))?;
    verify_snapshot(
        db,
        DocumentType::PurchaseChangeOrder,
        id,
        expected_subject_version,
        approval_process_instance_id,
        executor,
    )
    .await?;
    let submission_id = change.submission_id_for_effect(None)?;
    let submission = db
        .purchase_change_submissions()
        .find_by_id(submission_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购变更审批提交不存在".into()))?;
    change.ensure_current_approval_submission(&submission)?;
    change.reject_approval(expected_subject_version, actor.id())?;
    super::inquiry_supply::restore_current_quotes(db, change.purchase_order_id.as_ref(), executor).await?;
    db.purchase_change_orders().update(&mut change, executor).await?;
    let audit = actor.clone().resource_log(
        "purchase_change_order.reject",
        "purchase_change_order",
        id.to_string(),
    )?;
    persist_log(db, &audit, executor).await?;
    Ok(())
}

/// 同事务核对精确审批实例的单据及冻结版本；不从最新快照推断身份。
async fn verify_snapshot(
    db: &Database,
    document_type: DocumentType,
    id: &str,
    expected_subject_version: u32,
    approval_process_instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(approval_process_instance_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购审批冻结快照不存在".into()))?;
    snapshot.ensure_matches_runtime_subject(document_type, id, expected_subject_version)?;
    Ok(())
}
