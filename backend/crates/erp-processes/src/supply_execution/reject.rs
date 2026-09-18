use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_supply::dto::supplier_fulfillment::{RecordSupplierRejectRequest, SupplierOrderStatusHistoryView};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::repository::prelude::*;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::SupplierFulfillmentProcess;
use crate::Result;

impl SupplierFulfillmentProcess {
    /// 登记供应商拒单结果（回调幂等键 `(connection_id, external_event_id)`，§6.19）。
    ///
    /// 同事务推进履约主线到 `REJECTED`、追加状态历史并把原 `PLACE` 动作标记为
    /// 明确失败。重复回调（同一事件 ID）返回原状态历史，不重复推进。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    /// * `req` - 拒单结果请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新增的状态历史视图。
    ///
    /// # 错误
    /// * `NotFound` - 订单不存在
    /// * `BusinessLogicError` - 当前履约状态不可拒单
    /// * `ConflictError` - 事件 ID 冲突（事务注入失败时全部不可见）
    pub async fn record_reject(
        &self,
        id: &str,
        req: RecordSupplierRejectRequest,
        actor: &AuditActor,
    ) -> Result<SupplierOrderStatusHistoryView> {
        req.validate()?;
        let mut order = self.require_scoped_order(id, actor, "reject", &mut NoTransaction).await?;
        let connection_id = order.connection_id.clone();
        if let Some(existing) = self
            .db
            .supplier_order_status_histories()
            .find_by_connection_and_event(&connection_id, &req.external_event_id, &mut NoTransaction)
            .await?
        {
            tracing::info!(account = %actor.id(), order_id = %id, "拒单回调幂等命中");
            return Ok(existing.into());
        }
        let (history, action) = self.domain().prepare_reject(id, &mut order, &req).await?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.reject",
            "supplier_fulfillment_order",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let history_for_tx = history.clone();
        let mut action_for_tx = action.clone();
        let audit_for_tx = audit.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    erp_supply::service::supplier_fulfillment::reject::persist_reject(
                        &db,
                        &mut order_for_tx,
                        &history_for_tx,
                        &mut action_for_tx,
                        executor,
                    )
                    .await?;
                    db.audit_logs().create(&audit_for_tx, executor).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await?;
        Ok(history.into())
    }
}
