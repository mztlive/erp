//! 拒单历史、原PLACE动作状态与同域事务内保存。
use erp_core::common::time::Instant;
use erp_core::ids::SupplierOrderStatusHistoryId;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::SupplierFulfillmentService;
use crate::dto::supplier_fulfillment::*;
use crate::entity::supplier_fulfillment::*;
use crate::repository::SupplierFulfillmentExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
impl SupplierFulfillmentService {
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
    /// 仓储查询失败时返回对应错误。
    pub async fn latest_place_action(&self, id: &str) -> Result<SupplierOrderAction> {
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
    /// 原订单拒单推进后构造历史，再查最近PLACE并标记结果。
    ///
    /// # 参数
    /// * `id` - 供应商子订单主键，用于查找最近的 `PLACE` 动作。
    /// * `order` - 就地推进为拒单的子订单。
    /// * `req` - 供应商拒单回调请求。
    ///
    /// # 返回
    /// 返回尚未写入的状态历史，以及已标为失败的最近下单动作。
    ///
    /// # 错误
    /// 状态迁移、历史构造或动作更新失败时返回对应错误。不存在 `PLACE` 动作时返回 `NotFound`。仓储读取失败时返回对应错误。
    pub async fn prepare_reject(
        &self,
        id: &str,
        order: &mut SupplierFulfillmentOrder,
        req: &RecordSupplierRejectRequest,
    ) -> Result<(SupplierOrderStatusHistory, SupplierOrderAction)> {
        let connection_id = order.connection_id.clone();
        let previous = order.fulfillment_status;
        order.advance_fulfillment(FulfillmentStatus::Rejected)?;
        let history = SupplierOrderStatusHistory::new(
            SupplierOrderStatusHistoryId::new(next_id()),
            SupplierOrderStatusHistoryData::supplier_callback(SupplierCallbackParams {
                order_id: SupplierFulfillmentOrderId::new(order.base.id.as_str()),
                connection_id,
                previous_status: previous,
                new_status: FulfillmentStatus::Rejected,
                supplier_status_version: req.supplier_status_version.clone(),
                occurred_at: Instant::from_unix_secs(req.occurred_at),
                received_at: Instant::now(),
                external_event_id: req.external_event_id.clone(),
            }),
        )?;
        let mut action = self.latest_place_action(id).await?;
        action.update(SupplierOrderActionUpdate {
            status: Some(SupplierOrderActionStatus::Failed),
            response_summary: Some("供应商明确拒单（回调登记）".to_string()),
            ..Default::default()
        })?;
        Ok((history, action))
    }
}
/// 保持订单CAS、状态历史、动作CAS的顺序与执行器。
///
/// # 参数
/// * `db` - 履约集合所在数据库。
/// * `order` - 待按 CAS 写回的子订单。
/// * `history` - 待创建的状态历史。
/// * `action` - 待按 CAS 写回的下单动作。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 无返回值。写入顺序为订单、状态历史、动作。
///
/// # 错误
/// 任一步仓储写入失败时返回对应错误。
pub async fn persist_reject(
    db: &Database,
    order: &mut SupplierFulfillmentOrder,
    history: &SupplierOrderStatusHistory,
    action: &mut SupplierOrderAction,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.supplier_fulfillment_orders().update(order, executor).await?;
    db.supplier_order_status_histories().create(history, executor).await?;
    db.supplier_order_actions().update(action, executor).await?;
    Ok(())
}
