//! 作废采购改单草稿（第一期 §10.1：草稿允许编辑和作废）。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_procurement::dto::purchase_order::VoidPurchaseChangeRequest;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::service::purchase_order::change::state::void_purchase_change;
use erp_read_models::purchase_center::dto::PurchaseChangeOrderView;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::super::PurchaseOrderProcess;
use crate::audit::persist_log;
use crate::{Error, Result};

impl PurchaseOrderProcess {
    /// 作废采购改单草稿；原采购单当前版本保持不变，之后可重新发起改单。
    ///
    /// # 参数
    /// * `id` - 变更单主键
    /// * `req` - 作废请求（期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回作废后的变更单视图。
    ///
    /// # 错误
    /// * `NotFound` - 变更单不存在或来源采购单不可写
    /// * `ConflictError` - 版本已变化，或变更单不是草稿（审批中须先撤回）
    ///
    /// # 关键业务约束
    /// 必须先按来源采购单 `update` 动作证明可见性，再校验版本与状态；事务内重验。
    pub async fn void_change(
        &self,
        id: &str,
        req: VoidPurchaseChangeRequest,
        actor: &AuditActor,
    ) -> Result<PurchaseChangeOrderView> {
        req.validate()?;
        let change = self.domain().load_change(id, &mut NoTransaction).await?;
        let access = self.command_access(actor, "update")?;
        access.current(change.purchase_order_id.as_ref(), &mut NoTransaction).await?;
        let audit = actor.clone().resource_log(
            "purchase_change_order.void",
            "purchase_change_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let (change_id, actor_id) = (id.to_string(), actor.id().to_string());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut change = db
                        .purchase_change_orders()
                        .find_by_id(&change_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购变更单不存在".to_string()))?;
                    access.current(change.purchase_order_id.as_ref(), executor).await?;
                    change.ensure_expected_version(req.expected_lock_version).map_err(|_| {
                        Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
                    })?;
                    void_purchase_change(&mut change, &actor_id)?;
                    db.purchase_change_orders().update(&mut change, executor).await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<(), Error>(())
                })
            })
            .await?;
        self.change_view(id, actor).await
    }
}
