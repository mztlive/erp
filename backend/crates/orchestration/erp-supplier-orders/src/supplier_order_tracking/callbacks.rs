use erp_core::common::time::Instant;
use erp_integration::entity::supplier_callback::{SupplierCallbackReceipt, SupplierRefreshHint};
use erp_integration::repository::{SupplierCallbackExt, pending_order_callbacks};
use erp_supply::order_tracking::TrackingState;
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::ports::connector::order::OrderKey;
use erp_supply::repository::{SupplierFulfillmentExt, SupplierFulfillmentOrderRepositoryExt};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::persistence::is_superseded;
use super::{SupplierOrderTrackingProcess, later};
use crate::{Error, Result};

impl SupplierOrderTrackingProcess {
    /// 将已认证订单回调转换为已登记订单的立即补查意图。
    /// # 参数
    /// 无；身份限定在回调绑定连接内。
    /// # 返回
    /// 仅提前安排查询，不把接收报文直接用于业务写入。
    /// # 错误
    /// 接收或调度数据无法可靠读取、保存时返回错误。
    pub(super) async fn refresh_callbacks(&self) -> Result<()> {
        let receipts = pending_order_callbacks(&self.db, &mut NoTransaction).await?;
        for receipt in receipts {
            let (db, receipt) = (self.db.clone(), receipt.clone());
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        for hint in &receipt.refresh_hints {
                            let Some(order_id) = resolve_order(&db, &receipt, hint, executor).await? else {
                                continue;
                            };
                            let Some(mut tracking) =
                                db.supplier_order_tracking().find_by_id(&order_id, executor).await?
                            else {
                                continue;
                            };
                            let retired = is_superseded(&db, &order_id, executor).await?;
                            if !tracking.can_refresh(retired, Instant::now()) {
                                continue;
                            }
                            tracking.poll_after = Instant::now();
                            tracking.state = TrackingState::Polling;
                            db.supplier_order_tracking().update(&mut tracking, executor).await?;
                        }
                        Ok::<(), Error>(())
                    })
                })
                .await?;
        }
        Ok::<(), Error>(())
    }

    /// 仅对已取得新查询且完成领域应用的回调确认处理完成。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完成记录标记 applied，未完成记录保留 received 并退避。
    /// # 错误
    /// 接收记录冲突或持久化失败时拒绝确认。
    pub(super) async fn finish_callbacks(&self) -> Result<()> {
        let receipts = pending_order_callbacks(&self.db, &mut NoTransaction).await?;
        for receipt in receipts {
            let (db, id) = (self.db.clone(), receipt.base.id.clone());
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        let mut receipt = db
                            .supplier_callback_receipts()
                            .find_by_id(&id, executor)
                            .await?
                            .ok_or_else(|| Error::NotFound("供应商回调接收记录不存在".into()))?;
                        if receipt.status != "received" {
                            return Ok::<(), Error>(());
                        }
                        if let Some(retired) = resolved_retired_count(&db, &receipt, executor).await? {
                            receipt.mark_orders_resolved(retired, Instant::now())?;
                        } else {
                            receipt.processing_error =
                                Some("原订单尚未完成补查与履约应用；请核对供应商订单异常".into());
                            receipt.next_attempt_at = Some(later(60));
                        }
                        db.supplier_callback_receipts().update(&mut receipt, executor).await?;
                        Ok::<(), Error>(())
                    })
                })
                .await?;
        }
        Ok::<(), Error>(())
    }
}

async fn resolved_retired_count(
    db: &Database,
    receipt: &SupplierCallbackReceipt,
    executor: &mut dyn Executor,
) -> Result<Option<usize>> {
    let mut retired = 0;
    for hint in &receipt.refresh_hints {
        let Some(id) = resolve_order(db, receipt, hint, executor).await? else {
            return Ok(None);
        };
        if is_superseded(db, &id, executor).await? {
            retired += 1;
            continue;
        }
        let Some(tracking) = db.supplier_order_tracking().find_by_id(&id, executor).await? else {
            return Ok(None);
        };
        if tracking.latest_observation_id.is_none()
            || tracking.latest_observation_id != tracking.applied_observation_id
            || tracking.last_verified_at.is_none_or(|at| at < receipt.received_at)
            || tracking.last_error.is_some()
        {
            return Ok(None);
        }
    }
    Ok(Some(retired))
}

async fn resolve_order(
    db: &Database,
    receipt: &SupplierCallbackReceipt,
    hint: &SupplierRefreshHint,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    if !matches!(hint.topic.as_str(), "order" | "refund") {
        return Ok(None);
    }
    let key = match hint.target_kind.as_str() {
        "merchant_order" => OrderKey::Merchant(hint.object_id.clone()),
        "external_order" => OrderKey::External(hint.object_id.clone()),
        _ => return Ok(None),
    };
    Ok(db
        .supplier_fulfillment_orders()
        .find_for_connection(&receipt.connection_id, &key, executor)
        .await?
        .map(|order| order.base.id))
}
