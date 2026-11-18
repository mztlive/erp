//! 独立处理已持久化的来源图片，目录写入不等待外部媒体。
use std::sync::Arc;
use std::time::Duration;

use application_core::AuditActor;
use background_runtime::periodic;
use erp_catalog::CatalogService;
use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::repository::supplier_catalog::{self, SupplierCatalogExt};
use erp_support::BulkJobExt;
use mongodb::Database;
use persistence_core::{NoTransaction, Transactional};
use tokio::sync::watch;

use super::SourceMediaPreparer;
use crate::{Error, Result};

/// 独立且有界的来源媒体处理循环；数据库版本保护跨进程并发。
pub struct SourceMediaWorker {
    catalog: CatalogService,
    db: Database,
    preparer: Arc<SourceMediaPreparer>,
}

impl SourceMediaWorker {
    /// 创建独立媒体处理器。
    ///
    /// 数据库、受控素材准备器及已装配的商品领域服务。
    ///
    /// 不发起请求的处理器。
    pub fn new(db: Database, preparer: Arc<SourceMediaPreparer>, catalog: CatalogService) -> Self {
        Self { db, preparer, catalog }
    }

    /// 来源媒体 worker 按条目顺序执行；收到停止通知时先完成当前条目的写入和收尾，再退出。
    ///
    /// `stop` 为进程停止通知。
    ///
    /// 当前媒体处理和收尾已结束。
    ///
    /// 处理失败记录日志并在后续轮次重试。
    pub async fn run(&self, stop: watch::Receiver<bool>) {
        periodic(stop.clone(), Duration::from_secs(5), || async {
            if let Err(error) = self.run_due(&stop).await {
                tracing::warn!(error = %error, "供应商图片后台处理失败");
            }
        })
        .await;
    }

    // 在持久化条目边界检查停机，不取消媒体写入。
    async fn run_due(&self, stop: &watch::Receiver<bool>) -> Result<()> {
        let rows =
            supplier_catalog::due_media(&self.db, Instant::now().unix_secs(), &mut NoTransaction).await?;
        for item in rows {
            if *stop.borrow() || stop.has_changed().is_err() {
                break;
            }
            if let Err(error) = self.process(&item).await {
                tracing::warn!(item_id = %item.base.id, error = %error, "供应商图片处理未完成");
                self.defer(item).await;
            }
        }
        Ok(())
    }

    async fn actor(&self, item: &SupplierCatalogItem) -> Result<AuditActor> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(&item.job_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("图片来源同步任务不存在".into()))?;
        let account = self
            .db
            .accounts()
            .find_by_id(&job.requested_by, &mut NoTransaction)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::Forbidden("图片同步授权人已失效，请重新授权同步".into()))?;
        Ok(AuditActor::new(account.base.id.clone(), account.secret.account().into(), account.kind)
            .with_actor_name_snapshot(Some(account.name))?)
    }

    async fn process(&self, item: &SupplierCatalogItem) -> Result<()> {
        let actor = self.actor(item).await?;
        let media = self.preparer.prepare_pending_media(item, &actor).await?;
        let cleanup = media.cleanup_handle();

        let db = self.db.clone();
        let expected = item.clone();
        let catalog = self.catalog.clone();
        let outcome = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut current = db
                        .supplier_catalog_items()
                        .find_by_id(&expected.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("供应商商品不存在".into()))?;
                    if current.base.version != expected.base.version {
                        return Err(Error::ConflictError("供应商商品已变化，图片将重新处理".into()));
                    }

                    media.pending_assets.persist(&db, executor).await?;
                    media.apply_to(&mut current)?;
                    current.media_errors = media.verification_errors.clone();
                    current.media_retry_at =
                        (!current.media_errors.is_empty()).then(|| Instant::now().unix_secs() + 300);
                    db.supplier_catalog_items().update(&mut current, executor).await?;

                    crate::product_source_sync::apply_source_observation(
                        &db, &catalog, &current, &actor, executor,
                    )
                    .await?;
                    Ok(())
                })
            })
            .await;

        cleanup.after_transaction(&outcome).await;
        outcome
    }

    async fn defer(&self, mut item: SupplierCatalogItem) {
        item.media_retry_at = Some(Instant::now().unix_secs() + 300);
        item.media_errors = vec!["图片处理失败，将自动重试".into()];
        if let Err(error) = self.db.supplier_catalog_items().update(&mut item, &mut NoTransaction).await {
            tracing::warn!(item_id = %item.base.id, error = %error, "图片重试状态未保存");
        }
    }
}
