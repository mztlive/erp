//! 供应商订单查询、接收意图消费与真实 ERP 履约应用。

mod application;
mod callbacks;
mod delivery;
mod partial_delivery;
mod persistence;
mod shipment_tracking;

use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_supply::order_tracking::{OrderObservation, OrderTracking, repository};
use erp_supply::ports::connector::common::{Lookup, SupplierSku};
use erp_supply::ports::connector::order::{OrderKey, OrderLine};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::connectors::registry::SupplierConnectorRegistry;
use crate::{Error, Result};

/// 后台只读补查入口；供应商写操作由独立发单 Process 负责。
pub struct SupplierOrderTrackingProcess {
    db: Database,
    runtime: Arc<SupplierConnectorRegistry>,
}

impl SupplierOrderTrackingProcess {
    /// 注入部署数据库及供应商实际协议运行时。
    /// # 参数
    /// `db` 为当前部署业务库；`runtime` 为已配置协议的共享运行时。
    /// # 返回
    /// 订单跟踪 Process。
    /// # 错误
    /// 无。
    pub fn new(db: Database, runtime: Arc<SupplierConnectorRegistry>) -> Self {
        Self { db, runtime }
    }

    /// 处理回调意图及完成确认，不执行周期订单查询。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前有界回调批次已处理。
    /// # 错误
    /// 回调读取或保存失败时返回错误。
    pub async fn run_callbacks(&self) -> Result<()> {
        self.refresh_callbacks().await?;
        self.finish_callbacks().await
    }

    /// 查询并应用至多二十个到期订单，保留原认领和逐单失败处理。
    /// # 参数
    /// 无。
    /// # 返回
    /// 本轮完成处理的订单数；逐单业务结果以原记录为准。
    /// # 错误
    /// 扫描、认领或结果保存失败时返回错误。
    pub async fn poll_due(&self) -> Result<usize> {
        let orders = repository::due(&self.db, Instant::now(), &mut NoTransaction).await?;
        let mut processed = 0;
        for candidate in orders {
            let Some(tracking) = self.claim(&candidate.base.id).await? else {
                continue;
            };
            let result = self.observe(&tracking).await;
            if let Err(error) = result {
                self.finish_failure(&tracking, &safe_error(&error)).await?;
            }
            processed += 1;
        }
        Ok(processed)
    }

    async fn observe(&self, tracking: &OrderTracking) -> Result<()> {
        let order = self
            .db
            .supplier_fulfillment_orders()
            .find_by_id(&tracking.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("跟踪对应的供应商订单不存在".into()))?;
        if order.connection_id.as_ref() != tracking.connection_id {
            return Err(Error::ConflictError("供应商订单跟踪连接不一致".into()));
        }
        let Some(external) = &order.external_order_no else {
            return self.release_uncreated(tracking).await;
        };
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&tracking.connection_id, &mut NoTransaction)
            .await?;
        let frozen = self.frozen_order_lines(tracking).await?;
        let result = self
            .runtime
            .observe_order_with_lines(&connection, &OrderKey::External(external.clone()), &frozen)
            .await
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let Lookup::Found(snapshot) = result else {
            return Err(Error::BusinessLogicError("供应商暂未返回原订单，请继续查询，禁止重新下单".into()));
        };
        if snapshot.value.reference.merchant_order_no != order.fulfillment_order_no
            || &snapshot.value.reference.external_order_no != external
        {
            return Err(Error::ConflictError("供应商查询结果与原订单不一致".into()));
        }
        let observation = OrderObservation::from_snapshot(next_id(), tracking, snapshot)?;
        let observation_id = self.persist_observation(tracking, observation).await?;
        self.apply_delivery(tracking, &observation_id).await
    }
    async fn frozen_order_lines(&self, tracking: &OrderTracking) -> Result<Vec<OrderLine>> {
        let mut lines = Vec::with_capacity(tracking.source_lines.len());
        for source in &tracking.source_lines {
            let item = self
                .db
                .supplier_fulfillment_items()
                .find_by_id(&source.fulfillment_item_id, &mut NoTransaction)
                .await?
                .filter(|item| {
                    item.supplier_fulfillment_order_id.as_ref() == tracking.base.id
                        && item.quantity == source.quantity
                })
                .ok_or_else(|| Error::ConflictError("原供应商订货明细缺失或与冻结数量不一致".into()))?;
            lines.push(OrderLine {
                source_purchase_terms: None,
                line_id: item.base.id,
                sku: SupplierSku {
                    product_id: item.supplier_product_code_snapshot,
                    spec_id: item.supplier_sku_code_snapshot,
                },
                quantity: item.quantity,
                approved_unit_price: item.unit_cost_snapshot_gross,
                attributes: Vec::new(),
            });
        }
        Ok(lines)
    }
}

fn later(seconds: i64) -> Instant {
    Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(seconds))
}

fn safe_error(error: &Error) -> String {
    match error {
        Error::BusinessLogicError(message)
        | Error::ConflictError(message)
        | Error::ValidationError(message)
        | Error::NotFound(message) => message.clone(),
        Error::Logic(_) => "供应商结果与当前履约规则不一致，请核对采购及发货资料".into(),
        _ => "供应商结果尚未可靠保存或应用，请联系管理员核对后继续查询".into(),
    }
}
