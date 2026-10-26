//! 供应商订单查询、接收意图消费与真实 ERP 履约应用。

mod application;
mod callbacks;
mod delivery;
mod persistence;

use std::sync::Arc;

use erp_core::common::time::Instant;
use erp_supply::order_tracking::{OrderObservation, OrderTracking, repository};
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::order::OrderKey;
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::connectors::dangaoshushu::DangaoshushuRuntime;
use crate::{Error, Result};

/// 后台只读补查入口；供应商写操作由独立发单 Process 负责。
pub struct SupplierOrderTrackingProcess {
    db: Database,
    runtime: Arc<DangaoshushuRuntime>,
}

impl SupplierOrderTrackingProcess {
    /// 注入部署数据库及供应商实际协议运行时。
    /// # 参数
    /// `db` 为当前部署业务库；`runtime` 为已配置协议的共享运行时。
    /// # 返回
    /// 订单跟踪 Process。
    /// # 错误
    /// 无。
    pub fn new(db: Database, runtime: Arc<DangaoshushuRuntime>) -> Self {
        Self { db, runtime }
    }

    /// 调度至多二十个到期订单，并推进有界回调队列。
    /// # 参数
    /// 无；身份与来源仅从已登记订单关联取得。
    /// # 返回
    /// 本轮取得租约并完成查询处理的订单数。
    /// # 错误
    /// 队列查询失败返回错误；单单故障记录并留待下次处理，不阻断其他订单。
    pub async fn run_once(&self) -> Result<usize> {
        self.refresh_callbacks().await?;
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
        self.finish_callbacks().await?;
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
        let result = self
            .runtime
            .observe_order(&connection, &OrderKey::External(external.clone()))
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
