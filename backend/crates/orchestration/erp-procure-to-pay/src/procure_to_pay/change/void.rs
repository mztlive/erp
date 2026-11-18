//! 作废采购改单草稿（第一期 §10.1：草稿允许编辑和作废）。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_permissions::internal;
use erp_procurement::dto::purchase_order::VoidPurchaseChangeRequest;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::service::purchase_order::change::state::void_purchase_change;
use erp_read_models::purchase_center::dto::PurchaseChangeOrderView;
use erp_workflow::service::work_item::approval_rework::complete_document_rework;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::super::PurchaseOrderProcess;
use super::super::inquiry_supply::restore_current_quotes;
use crate::{Error, Result};

impl PurchaseOrderProcess {
    /// 作废采购改单草稿；原采购单当前版本保持不变，之后可重新发起改单。
    ///
    /// `id` 为变更单主键；`req` 为作废请求（期望版本）。返回作废后的变更单视图。`NotFound`：变更单不存在或来源采购单不可写。
    ///
    /// `ConflictError`：版本已变化，或变更单不是草稿（审批中须先撤回）。
    ///
    /// 必须先证明变更作废资格及来源采购单更新范围，再校验版本与状态；全部使用原事务。
    pub async fn void_change(
        &self,
        id: &str,
        req: VoidPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeOrderView> {
        req.validate()?;
        let service = self.clone();
        let transaction_actor = actor.clone();
        let change_id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service.void_change_apply(&change_id, &req, &transaction_actor, executor).await
                })
            })
            .await?;

        self.change_view(id, actor, &internal::PURCHASE_CHANGE_VOIDED_RESULT_READ).await
    }

    // 使用原事务先检查完整作废资格，再校验版本并写入作废、报价恢复和审计。
    async fn void_change_apply(
        &self,
        id: &str,
        req: &VoidPurchaseChangeRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut change = self
            .db
            .purchase_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
        self.ports
            .access(self.db.clone())
            .require_entry(
                actor,
                &internal::PURCHASE_CHANGE_VOID_SOURCE,
                change.purchase_order_id.as_ref(),
                executor,
            )
            .await?;

        change
            .ensure_expected_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        void_purchase_change(&mut change, actor.id())?;

        let audit = actor.clone().resource_log(
            "purchase_change_order.void",
            "purchase_change_order",
            id.to_string(),
        )?;

        restore_current_quotes(&self.db, change.purchase_order_id.as_ref(), executor).await?;
        self.db.purchase_change_orders().update(&mut change, executor).await?;
        complete_document_rework(&self.db, "purchase_change_order", id, actor.id(), Instant::now(), executor)
            .await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(())
    }
}
