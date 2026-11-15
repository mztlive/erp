//! 采购变更目标独立保存：保持草稿/已驳回状态，不启动审批或调整资源占用。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_permissions::internal;
use erp_procurement::dto::purchase_order::{
    PurchaseChangeDraftView, SavePurchaseChangeDraftRequest, SavePurchaseOrderLine,
};
use erp_procurement::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseCommandReceipt, PurchaseCommandReceiptIdentity, PurchaseOrder,
};
use erp_procurement::repository::{PurchaseCommandExt, PurchaseOrderExt};
use erp_procurement::service::purchase_order::PurchaseOrderService;
use erp_workflow::service::approval::execution::map_receipt_first_write_error;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::{PurchaseOrderPorts, PurchaseOrderProcess};
use super::guard::check_change_inputs;
use super::submit::FrozenChangeSubmission;
use crate::{Error, Result};

struct SaveTarget {
    change: PurchaseChangeOrder,
    target: FrozenChangeSubmission,
    request: SavePurchaseChangeDraftRequest,
    identity: PurchaseCommandReceiptIdentity,
    fingerprint: String,
    actor: AuditActor,
}

impl PurchaseOrderProcess {
    /// 保存原采购变更的完整编辑目标，不创建审批版本或占用报价额度。
    /// # 参数
    /// * `id` - 原变更单身份。
    /// * `request` - 完整目标、原锁版本和幂等键。
    /// * `actor` - 当前已认证操作人。
    /// # 返回
    /// 返回首次保存后的完整编辑快照；同键同请求重放该快照。
    /// # 错误
    /// 权限、来源、版本、内容不合规，或同键不同内容时拒绝。
    pub async fn save_change_draft(
        &self,
        id: &str,
        request: SavePurchaseChangeDraftRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeDraftView> {
        request.validate()?;
        let (identity, fingerprint) = request.command(id, actor.id())?;
        let service = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let change = service.domain().load_change(&id, executor).await?;
                    let order =
                        authorized_order(&service.db, &service.ports, &change, &actor, executor).await?;
                    if let Some(result) = replay_save(&service.db, &identity, &fingerprint, executor).await? {
                        return Ok(result);
                    }
                    change.ensure_expected_version(request.expected_lock_version)?;
                    change.ensure_draft_origin(&order)?;
                    let mut target = service
                        .prepare_change_target(&change, &order, &request.target_request(), &actor, executor)
                        .await?;
                    target.submission.mark_editing_copy()?;
                    let input = SaveTarget { change, target, request, identity, fingerprint, actor };
                    persist_save(&service.db, &service.ports, input, executor).await
                })
            })
            .await
    }
}

/// 读取目标前同时证明当前更新和提交资格，避免以历史审批参与身份读取成本。
async fn authorized_order(
    db: &Database,
    ports: &PurchaseOrderPorts,
    change: &PurchaseChangeOrder,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    let access = ports.access(db.clone());
    Ok(access
        .require_entry(actor, &internal::PURCHASE_CHANGE_SOURCE, change.purchase_order_id.as_ref(), executor)
        .await?)
}

/// 同事务重验权限、原版本和来源后写入独立头行及精确保存回执。
async fn persist_save(
    db: &Database,
    ports: &PurchaseOrderPorts,
    input: SaveTarget,
    executor: &mut dyn Executor,
) -> Result<PurchaseChangeDraftView> {
    let domain = PurchaseOrderService::new(db.clone());
    let mut current = domain.load_change(&input.change.base.id, executor).await?;
    let order = authorized_order(db, ports, &current, &input.actor, executor).await?;
    if let Some(result) = replay_save(db, &input.identity, &input.fingerprint, executor).await? {
        return Ok(result);
    }
    current.ensure_expected_version(input.request.expected_lock_version)?;
    current.ensure_draft_origin(&order)?;
    let lines = input.target.lines.iter().map(SavePurchaseOrderLine::from).collect::<Vec<_>>();
    let inputs = SavePurchaseOrderLine::to_line_inputs(&lines)?;
    check_change_inputs(db, &order, &inputs, input.target.submission.gross_amount, executor).await?;
    current.save_draft_target(
        &input.target.submission,
        &input.target.lines,
        input.request.reason,
        input.target.content_hash,
        input.actor.id(),
    )?;
    db.purchase_order()
        .create_change_submission(&mut current, &input.target.submission, &input.target.lines, executor)
        .await?;
    let result = domain.change_draft(&current, &order, executor).await?;
    let audit = input.actor.resource_log(
        "purchase_change_order.save_draft",
        "purchase_change_order",
        current.base.id.clone(),
    )?;
    let receipt = PurchaseCommandReceipt::new(
        &input.identity,
        &input.fingerprint,
        result.clone(),
        audit.base.id.clone(),
    )?;
    db.purchase_command_receipts::<PurchaseChangeDraftView>()
        .create(&receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    persist_log(db, &audit, executor).await?;
    Ok(result)
}

/// 按原身份和完整请求恢复保存结果，不读取当前可变目标拼接回执。
async fn replay_save(
    db: &Database,
    identity: &PurchaseCommandReceiptIdentity,
    fingerprint: &str,
    executor: &mut dyn Executor,
) -> Result<Option<PurchaseChangeDraftView>> {
    let Some(receipt) = db
        .purchase_command_receipts::<PurchaseChangeDraftView>()
        .find_by_id_including_deleted(identity.receipt_id(), executor)
        .await?
    else {
        return Ok(None);
    };
    let result = PurchaseCommandReceipt::decode(receipt, identity, fingerprint)
        .map_err(|_| Error::ConflictError("幂等键已用于不同的采购变更保存请求或回执无效".into()))?;
    Ok(Some(result.into_payload()))
}
