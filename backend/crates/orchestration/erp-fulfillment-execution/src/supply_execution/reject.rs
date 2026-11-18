use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierFulfillmentOrderId;
use erp_permissions::supplier_fulfillment;
use erp_supply::dto::supplier_fulfillment::{RecordSupplierRejectRequest, SupplierOrderStatusHistoryView};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::repository::prelude::*;
use erp_supply::service::supplier_fulfillment::reject::persist_reject;
use persistence_core::{Executor, Transactional};
use validator::Validate;

use super::SupplierFulfillmentProcess;
use crate::{Error, Result};

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
        let process = self.clone();
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { process.reject(&id, &req, &actor, executor).await })
            })
            .await
    }

    async fn reject(
        &self,
        id: &str,
        req: &RecordSupplierRejectRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SupplierOrderStatusHistoryView> {
        let mut order =
            self.require_legacy_order(id, actor, &supplier_fulfillment::ORDER_REJECT, executor).await?;
        if let Some(existing) = self
            .db
            .supplier_order_status_histories()
            .find_by_connection_and_event(&order.connection_id, &req.external_event_id, executor)
            .await?
        {
            if !existing.matches_reject(
                &SupplierFulfillmentOrderId::new(id),
                &req.supplier_status_version,
                Instant::from_unix_secs(req.occurred_at),
            ) {
                return Err(Error::ConflictError(
                    "该供应商拒单通知已用于其他订单或结果，请核对供应商的原通知".into(),
                ));
            }
            return Ok(existing.into());
        }

        let (history, mut action) = self.domain().prepare_reject(id, &mut order, req, executor).await?;
        let audit = actor.clone().resource_log(
            "supplier_fulfillment.reject",
            "supplier_fulfillment_order",
            id.to_string(),
        )?;

        persist_reject(&self.db, &mut order, &history, &mut action, executor).await?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(history.into())
    }
}
