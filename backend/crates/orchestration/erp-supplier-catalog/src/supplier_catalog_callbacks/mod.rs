//! 供应商商品回调：有界领取、当前事实补查及逐规格原子检查点。
use erp_catalog::CatalogService;
use erp_permissions::supplier_api;
mod query;
mod store;
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_identity::AuthorizationService;
use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_integration::repository::pending_catalog_callbacks;
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::ReadExecutor;
pub use query::{CatalogUpdate, query_current};

use crate::supplier_connection_execution::catalog_refresh_actor;
use crate::{Error, Result};

#[derive(Clone)]
pub struct SupplierCatalogCallbackProcess {
    catalog: CatalogService,
    db: Database,
    registry: Arc<SupplierConnectorRegistry>,
    authorization: Arc<AuthorizationService>,
}

struct Context {
    connection: SupplierApiConnection,
    connector: Arc<dyn SupplierConnector>,
    actor: AuditActor,
}

impl SupplierCatalogCallbackProcess {
    /// 绑定应用已有数据库、协议注册表及权限服务。
    /// # 参数
    /// 组合根提供数据库、协议注册表、权限来源及商品服务，不读取额外凭据。
    /// # 返回
    /// 尚未领取任务的消费者。
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: Database,
        registry: Arc<SupplierConnectorRegistry>,
        authorization: Arc<AuthorizationService>,
        catalog: CatalogService,
    ) -> Self {
        Self { db, registry, authorization, catalog }
    }

    /// 每轮至多五条通知，每条至多十个步骤；失败保留检查点并延期。
    /// # 参数
    /// 无，所有目标读取自已持久化且验签通过的回调。
    /// # 返回
    /// 成功领取的通知数，不表示已全部应用。
    /// # 错误
    /// 队列读取失败返回错误；单项失败隔离并继续下一项。
    pub async fn run_once(&self) -> Result<usize> {
        let candidates = pending_catalog_callbacks(&self.db, &mut ReadExecutor::default()).await?;
        let mut count = 0;
        for candidate in candidates {
            let mut receipt = match self.claim(&candidate).await {
                Ok(Some(receipt)) => receipt,
                Ok(None) => continue,
                Err(error) => {
                    let _ = self.defer(&candidate, &safe_error(&error)).await;
                    continue;
                },
            };
            count += 1;

            if let Err(error) = self.process(&mut receipt).await {
                let summary = safe_error(&error);
                if self.defer(&receipt, &summary).await.is_err() {
                    tracing::warn!(connection_id = %receipt.connection_id, receipt_id = %receipt.base.id,
                        "商品回调延期未保存，保留租约到期后恢复");
                }
            }
        }
        Ok(count)
    }

    async fn context(&self, receipt: &SupplierCallbackReceipt) -> Result<Context> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(receipt.connection_id.as_ref(), &mut ReadExecutor::default())
            .await?;
        let connector = self
            .registry
            .notification_connector(&connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        if connector.callbacks().is_none() || connector.connection_id().as_ref() != connection.base.id {
            return Err(Error::Forbidden("商品回调协议绑定不属于当前连接".into()));
        }

        let actor = catalog_refresh_actor(&self.db, &connection.base.id).await?;
        self.authorize(&actor).await?;
        Ok(Context { connection, connector, actor })
    }

    async fn authorize(&self, actor: &AuditActor) -> Result<()> {
        let permission = supplier_api::ADOPTIONS_PREVIEW;
        if !self.authorization.has_permission(actor, &permission, &mut ReadExecutor::default()).await? {
            return Err(Error::Forbidden("商品采集授权人已无同步权限，请重新授权同步".into()));
        }
        Ok(())
    }

    async fn process(&self, receipt: &mut SupplierCallbackReceipt) -> Result<()> {
        let context = self.context(receipt).await?;
        for _ in 0..10 {
            if receipt.catalog_hint()?.is_none() {
                break;
            }
            let target = self.next_target(receipt).await?;
            let update = match target {
                Some(sku) => Some(query::query(&context, receipt, sku, &self.registry).await?),
                None => None,
            };

            self.authorize(&context.actor).await?;
            *receipt = self.save_step(receipt, &context, update).await?;
        }

        *receipt = self.settle(receipt).await?;
        Ok(())
    }
}

fn safe_error(error: &Error) -> String {
    match error {
        Error::BusinessLogicError(message)
        | Error::ValidationError(message)
        | Error::ConflictError(message)
        | Error::NotFound(message)
        | Error::Forbidden(message) => message.clone(),
        _ => "商品回调补查或保存失败，保留原检查点后重试".into(),
    }
}

fn after(seconds: i64) -> Instant {
    Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(seconds))
}
