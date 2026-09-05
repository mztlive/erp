use database::{AccessControlExt, NoTransaction, SupplierFulfillmentExt, Transactional};
use entities::common::time::Instant;
use entities::ids::SupplierOrderStatusHistoryId;
use entities::supplier_fulfillment::{
    FulfillmentStatus, SupplierFulfillmentOrderId, SupplierOrderAction, SupplierOrderActionStatus,
    SupplierOrderActionType, SupplierOrderActionUpdate, SupplierOrderStatusHistory,
    SupplierOrderStatusHistoryData,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{RecordSupplierRejectRequest, SupplierOrderStatusHistoryView};
use super::SupplierFulfillmentService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SupplierFulfillmentService {
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
        let mut order = self.load_order(id).await?;
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
        let previous = order.fulfillment_status;
        order.advance_fulfillment(FulfillmentStatus::Rejected)?;
        let history = SupplierOrderStatusHistory::new(
            SupplierOrderStatusHistoryId::new(next_id()),
            SupplierOrderStatusHistoryData::supplier_callback(
                SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                connection_id,
                previous,
                FulfillmentStatus::Rejected,
                req.supplier_status_version.clone(),
                Instant::from_unix_secs(req.occurred_at),
                Instant::now(),
                req.external_event_id.clone(),
            ),
        )?;
        let mut action = self.latest_place_action(id).await?;
        action.update(SupplierOrderActionUpdate {
            status: Some(SupplierOrderActionStatus::Failed),
            response_summary: Some("供应商明确拒单（回调登记）".to_string()),
            ..Default::default()
        })?;
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
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_fulfillment_orders()
                        .update(&mut order_for_tx, session)
                        .await?;
                    db.supplier_order_status_histories()
                        .create(&history_for_tx, session)
                        .await?;
                    db.supplier_order_actions()
                        .update(&mut action_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit_for_tx, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(history.into())
    }

    /// 加载该订单最近一次 `PLACE` 动作。
    ///
    /// # 参数
    /// * `id` - 供应商子订单 ID
    ///
    /// # 返回
    /// 返回 `PLACE` 动作实体。
    ///
    /// # 错误
    /// * `NotFound` - 不存在 `PLACE` 动作
    async fn latest_place_action(&self, id: &str) -> Result<SupplierOrderAction> {
        self.db
            .supplier_order_actions()
            .latest_by_order_and_type(
                &SupplierFulfillmentOrderId::new(id),
                SupplierOrderActionType::Place,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::NotFound("该订单不存在下单动作".to_string()))
    }
}
