//! 消息只触发当前事实查询；目录复用原同步应用，订单复用原跟踪事实。

use std::future::Future;
use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_callback::SupplierRefreshHint;
use erp_integration::entity::supplier_message::SupplierPulledMessage;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderExt, for_external_order};
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::entity::supplier_fulfillment::SupplierFulfillmentOrder;
use erp_supply::order_tracking::TrackingState;
use erp_supply::order_tracking::repository::OrderTrackingExt;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{Lookup, SupplierSku};
use erp_supply::ports::connector::messages::SupplierMessageSource;
use erp_supply::ports::connector::order::OrderKey;
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::repository::supplier_catalog::{self, CatalogMessageTarget};
use erp_supply::repository::{SupplierFulfillmentExt, SupplierFulfillmentOrderRepositoryExt};
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::address::refresh_address_notice;
use super::{SupplierMessageCatalog, SupplierMessageFundsInvestigation, SupplierMessageRefreshApplier};
use crate::{Error, Result};

pub(super) struct SupplierMessageRefresh {
    db: Database,
    registry: Arc<SupplierConnectorRegistry>,
    connection: SupplierApiConnection,
    funds: Option<Arc<dyn SupplierMessageFundsInvestigation>>,
    catalog: Arc<dyn SupplierMessageCatalog>,
}

impl SupplierMessageRefresh {
    pub(super) fn new(
        db: Database,
        registry: Arc<SupplierConnectorRegistry>,
        connection: SupplierApiConnection,
        funds: Option<Arc<dyn SupplierMessageFundsInvestigation>>,
        catalog: Arc<dyn SupplierMessageCatalog>,
    ) -> Self {
        Self { db, registry, connection, funds, catalog }
    }

    async fn refresh_catalog(
        &self,
        message: &SupplierPulledMessage,
        hint: &SupplierRefreshHint,
        connector: &dyn SupplierConnector,
    ) -> Result<()> {
        if !matches!(hint.topic.as_str(), "product" | "price" | "availability") {
            return Err(Error::BusinessLogicError("消息所需来源地区或资金核验尚未完成".into()));
        }
        let offers = connector
            .offers()
            .ok_or_else(|| Error::BusinessLogicError("消息来源不支持定向商品查询".into()))?;
        let skus = self.catalog_targets(hint).await?;
        let actor = self.catalog.actor(&self.connection.base.id).await?;
        for sku in skus {
            let Lookup::Found(snapshot) =
                offers.offer(&sku).await.map_err(|error| Error::BusinessLogicError(error.summary))?
            else {
                return Err(Error::BusinessLogicError("消息商品暂不可见，保留原消息继续核验".into()));
            };
            if snapshot.value.sku.spec_id != sku.spec_id
                || sku
                    .product_id
                    .as_ref()
                    .is_some_and(|id| snapshot.value.sku.product_id.as_ref() != Some(id))
                || snapshot.stamp.observed_at < message.received_at
            {
                return Err(Error::ConflictError("消息补查的商品身份或观察时间不一致".into()));
            }
            let mut item = SupplierCatalogItem::observed(
                &self.connection.base.id,
                self.connection.supplier_id.as_ref(),
                &message.base.id,
                snapshot,
            )?;
            item.availability_valid_until = self
                .registry
                .catalog_availability_validity(&self.connection)
                .and_then(|seconds| i64::try_from(seconds).ok())
                .and_then(|seconds| item.observed_at.unix_secs().checked_add(seconds))
                .map(Instant::from_unix_secs);
            if item.images.as_ref().is_some_and(|images| !images.is_empty())
                || item.detail_images.as_ref().is_some_and(|images| !images.is_empty())
            {
                item.media_retry_at = Some(Instant::now().unix_secs());
            }
            self.catalog.save(&self.connection, item, &actor).await?;
        }
        Ok(())
    }

    async fn catalog_targets(&self, hint: &SupplierRefreshHint) -> Result<Vec<SupplierSku>> {
        let target = match hint.target_kind.as_str() {
            "sku" => {
                return Ok(vec![SupplierSku {
                    product_id: hint.product_id.clone(),
                    spec_id: hint.object_id.clone(),
                }]);
            },
            "product" => CatalogMessageTarget::Product(&hint.object_id),
            "brand" => CatalogMessageTarget::Brand(&hint.object_id),
            _ => return Err(Error::BusinessLogicError("消息目录刷新身份尚未支持".into())),
        };
        let items =
            supplier_catalog::message_targets(&self.db, &self.connection.base.id, target, &mut NoTransaction)
                .await?;
        if items.is_empty() || items.len() > 50 {
            return Err(Error::BusinessLogicError("消息涉及的目录范围需先完成全量基线核验".into()));
        }
        Ok(items
            .into_iter()
            .map(|item| SupplierSku { product_id: Some(item.product_id), spec_id: item.spec_id })
            .collect())
    }

    async fn refresh_order(
        &self,
        source: &dyn SupplierMessageSource,
        message: &SupplierPulledMessage,
        hint: &SupplierRefreshHint,
    ) -> Result<()> {
        let key = match hint.target_kind.as_str() {
            "merchant_order" => OrderKey::Merchant(hint.object_id.clone()),
            "external_order" => OrderKey::External(hint.object_id.clone()),
            _ => return Err(Error::BusinessLogicError("订单消息缺少可核验的原单身份".into())),
        };
        let (db, received) = (self.db.clone(), message.clone());
        let refresh = async {
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move { refresh_order_intent(&db, &received, &key, executor).await })
                })
                .await
        };
        let funds = async {
            self.funds
                .as_ref()
                .ok_or_else(|| Error::BusinessLogicError("资金调查尚未装配，消息继续保留待核实".into()))?
                .investigate(message, hint)
                .await
        };
        apply_order_refresh(source, message, hint, refresh, funds).await
    }
}

#[async_trait]
impl SupplierMessageRefreshApplier for SupplierMessageRefresh {
    async fn refresh(&self, message: &SupplierPulledMessage) -> Result<()> {
        if message.connection_id.as_ref() != self.connection.base.id || message.refresh_hints.is_empty() {
            return Err(Error::Forbidden("消息来源不属于当前连接或缺少刷新意图".into()));
        }
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&self.connection.base.id, &mut NoTransaction)
            .await?;
        let connector = self
            .registry
            .fulfillment_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let source = connector
            .messages()
            .filter(|source| source.account_scope() == message.account_scope)
            .ok_or_else(|| Error::Forbidden("消息来源账号与当前适配器不一致".into()))?;
        for hint in &message.refresh_hints {
            if hint.topic == "address" {
                let source = connector.address_source().ok_or_else(|| {
                    Error::BusinessLogicError("当前供应商未提供实时地址查询，请先核实连接能力".into())
                })?;
                refresh_address_notice(source, hint, message.received_at).await?;
            } else if matches!(hint.topic.as_str(), "order" | "refund") {
                self.refresh_order(source, message, hint).await?;
            } else {
                self.refresh_catalog(message, hint, connector.as_ref()).await?;
            }
        }
        Ok(())
    }
}

/// 按来源协议恢复原单提示；旧资金提示不能跳过原单应用或真实资金身份核验。
/// # 参数
/// 原账号消息能力、可靠接收证据、原提示以及原单和资金应用步骤。
/// # 返回
/// 原单已补查应用，且适用的资金步骤已经结束时返回成功。
/// # 错误
/// 原单未完成或必要资金步骤失败时停止，保留消息及原资金证据。
pub(super) async fn apply_order_refresh(
    source: &dyn SupplierMessageSource,
    message: &SupplierPulledMessage,
    hint: &SupplierRefreshHint,
    refresh: impl Future<Output = Result<bool>>,
    funds: impl Future<Output = Result<()>>,
) -> Result<()> {
    if !refresh.await? {
        return Err(Error::BusinessLogicError("原订单补查应用或资金调查尚未完成，消息继续保留".into()));
    }
    if hint.topic == "refund"
        && (hint.funds_transaction_id.is_some() || !source.order_only_notice(&message.message_type))
    {
        funds.await?;
    }
    Ok(())
}

async fn refresh_order_intent(
    db: &Database,
    message: &SupplierPulledMessage,
    key: &OrderKey,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let (order, execution) = resolve_execution(db, message, key, executor).await?;
    if execution.connection_id != message.connection_id.as_ref()
        || execution.supplier_id != order.supplier_id.as_ref()
    {
        return Err(Error::ConflictError("消息订单执行关联不一致".into()));
    }
    if execution.superseded_by_purchase_revision_id.is_some() {
        return Ok(true);
    }
    let mut tracking = db
        .supplier_order_tracking()
        .find_by_id(&order.base.id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("消息原订单尚未登记跟踪".into()))?;
    if tracking.connection_id != message.connection_id.as_ref()
        || tracking.purchase_order_id != execution.purchase_order_id
    {
        return Err(Error::Forbidden("消息跟踪归属不一致".into()));
    }
    if tracking.applied_since(message.received_at) {
        return Ok(true);
    }
    if tracking.can_refresh(false, Instant::now()) {
        tracking.poll_after = Instant::now();
        tracking.state = TrackingState::Polling;
        db.supplier_order_tracking().update(&mut tracking, executor).await?;
    }
    Ok(false)
}

async fn resolve_execution(
    db: &Database,
    message: &SupplierPulledMessage,
    key: &OrderKey,
    executor: &mut dyn Executor,
) -> Result<(SupplierFulfillmentOrder, ApiOrderExecution)> {
    if let Some(order) =
        db.supplier_fulfillment_orders().find_for_connection(&message.connection_id, key, executor).await?
    {
        let execution = db
            .supplier_api_order_executions()
            .find_by_id(&order.base.id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("消息原订单缺少接口执行关联".into()))?;
        return Ok((order, execution));
    }
    if let OrderKey::External(external) = key
        && let Some(execution) =
            for_external_order(db, message.connection_id.as_ref(), external, executor).await?
    {
        let order = db
            .supplier_fulfillment_orders()
            .find_by_id(&execution.base.id, executor)
            .await?
            .filter(|order| order.connection_id == message.connection_id)
            .ok_or_else(|| Error::NotFound("消息子单缺少原根单关联".into()))?;
        return Ok((order, execution));
    }
    Err(Error::BusinessLogicError("消息原订单尚未定位，请核对原根单及子单".into()))
}
