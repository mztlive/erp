//! 分页读取及素材准备在事务外；来源、公司资料、检查点与任务心跳同事务。
use std::cmp::Ordering;
use std::num::NonZeroU32;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_supply::dto::supplier_catalog::CatalogSyncMode;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::{SupplierCatalogItem, SupplierCatalogRun};
use erp_supply::ports::connector::common::{Scan, ScanWindow, Snapshot, SourceStamp, SupplierSku};
use erp_supply::ports::connector::offer::{IncrementalSupport, Offer, OfferChange};
use erp_supply::ports::supplier_api_gateway::{CatalogPage, ClassifiedError};
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_support::repository::prelude::*;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::SupplierConnectionExecutionProcess;
use crate::source_media::PreparedSourceMedia;
use crate::{Error, Result};

struct PreparedObservation {
    item: SupplierCatalogItem,
    media: PreparedSourceMedia,
}
struct Withdrawal {
    sku: SupplierSku,
    stamp: SourceStamp,
}
struct PreparedPage {
    items: Vec<PreparedObservation>,
    withdrawn: Vec<Withdrawal>,
    next: Option<String>,
    count: u64,
    skipped: Vec<String>,
}

impl SupplierConnectionExecutionProcess {
    /// 按冻结窗口顺序采集、补齐资料并保存；失败保留此前页面。
    /// # 参数
    /// 连接、已开始任务及真实任务操作人。
    /// # 返回
    /// 末页完成持久化时返回成功。
    /// # 错误
    /// 上游、映射、配置冲突或存储失败时返回脱敏分类错误。
    pub(super) async fn scan_catalog(
        &self,
        connection: &SupplierApiConnection,
        job: &BackgroundJob,
        actor: &AuditActor,
    ) -> std::result::Result<(), ClassifiedError> {
        self.prepare_scan(connection, &job.base.id).await.map_err(classify)?;
        loop {
            let run = self
                .db
                .supplier_catalog_runs()
                .find_by_id(&job.base.id, &mut NoTransaction)
                .await
                .map_err(|_| storage_error())?
                .ok_or_else(storage_error)?;
            if run.finished {
                return Ok(());
            }
            let current = SupplierApiService::new(self.db.clone())
                .load_connection(&connection.base.id, &mut NoTransaction)
                .await
                .map_err(|_| storage_error())?;
            verify_connection(&current, &run).map_err(classify)?;
            let scan = Scan {
                window: run.scan_since.map_or(ScanWindow::Full, |since| ScanWindow::Changes {
                    since,
                    through: run.scan_through,
                }),
                cursor: run.cursor.clone(),
                limit: NonZeroU32::new(5).expect("5 非零"),
            };
            let page = self.gateway.catalog_page(&current, &scan).await?;
            let prepared = self.prepare_page(&current, &run.base.id, page, actor).await.map_err(classify)?;
            self.save_catalog_page(current, run, prepared, actor).await.map_err(classify)?;
        }
    }

    async fn prepare_scan(&self, connection: &SupplierApiConnection, job: &str) -> Result<()> {
        let support = self
            .gateway
            .catalog_incremental_support(connection)
            .map_err(|error| Error::BusinessLogicError(error.summary))?;
        let mut run = self
            .db
            .supplier_catalog_runs()
            .find_by_id(job, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("同步记录不存在".into()))?;
        if run.pages != 0 || run.scan_since.is_some() || support == IncrementalSupport::Unsupported {
            return Ok(());
        }
        let history =
            self.db.background_jobs().recent_catalog_jobs(&connection.base.id, &mut NoTransaction).await?;
        let full_interval = i64::try_from(self.gateway.catalog_full_sync_interval(connection))
            .map_err(|_| Error::ValidationError("完整同步周期超限".into()))?;
        let full_since = run.scan_through.unix_secs().saturating_sub(full_interval);
        if !self.full_scan_is_recent(connection, &history, full_since).await? {
            return Ok(());
        }
        for previous in history
            .into_iter()
            .filter(|record| record.status == JobStatus::Succeeded && record.base.id != job)
        {
            let Some(prior) =
                self.db.supplier_catalog_runs().find_by_id(&previous.base.id, &mut NoTransaction).await?
            else {
                continue;
            };
            if prior.skipped_count > 0 {
                return Ok(());
            }
            if prior.finished && prior.technical_config_version == connection.technical_config_version {
                // 日粒度来源保留一天重叠；适配器继续向外扩展 through 当日。
                run.scan_since =
                    Some(Instant::from_unix_secs(prior.scan_through.unix_secs().saturating_sub(86_400)));
                run.sync_mode = if support == IncrementalSupport::Complete {
                    CatalogSyncMode::IncrementalComplete
                } else {
                    CatalogSyncMode::IncrementalPartial
                };
                self.db.supplier_catalog_runs().update(&mut run, &mut NoTransaction).await?;
                break;
            }
        }
        Ok(())
    }

    async fn full_scan_is_recent(
        &self,
        connection: &SupplierApiConnection,
        history: &[BackgroundJob],
        since: i64,
    ) -> Result<bool> {
        for prior_job in history.iter().filter(|job| job.status == JobStatus::Succeeded) {
            if let Some(prior) =
                self.db.supplier_catalog_runs().find_by_id(&prior_job.base.id, &mut NoTransaction).await?
                && prior.finished
                && prior.skipped_count == 0
                && prior.sync_mode == CatalogSyncMode::Full
                && prior.technical_config_version == connection.technical_config_version
                && prior.scan_through.unix_secs() >= since
            {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn prepare_page(
        &self,
        connection: &SupplierApiConnection,
        job: &str,
        page: CatalogPage,
        actor: &AuditActor,
    ) -> Result<PreparedPage> {
        let count =
            u64::try_from(page.items.len()).map_err(|_| Error::ValidationError("来源商品数量超限".into()))?;
        let mut prepared =
            PreparedPage { items: vec![], withdrawn: vec![], next: page.next, count, skipped: page.skipped };
        if let Err(error) = self.prepare_page_items(connection, job, page.items, actor, &mut prepared).await {
            for observation in &prepared.items {
                observation.media.cleanup_handle().discard_uncommitted().await;
            }
            return Err(error);
        }
        Ok(prepared)
    }

    async fn prepare_page_items(
        &self,
        connection: &SupplierApiConnection,
        job: &str,
        items: Vec<Snapshot<OfferChange>>,
        actor: &AuditActor,
        prepared: &mut PreparedPage,
    ) -> Result<()> {
        for snapshot in items {
            self.heartbeat_catalog_job(job).await?;
            match snapshot.value {
                OfferChange::Upsert(offer) => {
                    let observation = self
                        .prepare_catalog_observation(
                            connection,
                            job,
                            Snapshot { value: *offer, stamp: snapshot.stamp },
                            actor,
                        )
                        .await?;
                    prepared.items.push(observation);
                },
                OfferChange::Withdrawn(sku) => {
                    prepared.withdrawn.push(Withdrawal { sku, stamp: snapshot.stamp })
                },
            }
        }
        Ok(())
    }

    async fn prepare_catalog_observation(
        &self,
        connection: &SupplierApiConnection,
        job: &str,
        snapshot: Snapshot<Offer>,
        actor: &AuditActor,
    ) -> Result<PreparedObservation> {
        let mut item = SupplierCatalogItem::observed(
            &connection.base.id,
            connection.supplier_id.as_ref(),
            job,
            snapshot,
        )?;
        item.availability_valid_until = self
            .gateway
            .catalog_availability_validity(connection)
            .and_then(|seconds| i64::try_from(seconds).ok())
            .and_then(|seconds| item.observed_at.unix_secs().checked_add(seconds))
            .map(Instant::from_unix_secs);
        let media = if let Some(preparer) = &self.source_media {
            let previous =
                self.db.supplier_catalog_items().find_by_id(&item.base.id, &mut NoTransaction).await?;
            let media = preparer.prepare_observation_media(&item, previous.as_ref(), actor).await?;
            if let Err(error) = media.apply_to(&mut item) {
                media.cleanup_handle().discard_uncommitted().await;
                return Err(error);
            }
            media
        } else {
            PreparedSourceMedia::empty()
        };
        Ok(PreparedObservation { item, media })
    }

    async fn heartbeat_catalog_job(&self, id: &str) -> Result<()> {
        let mut job = self
            .db
            .background_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("同步任务不存在".into()))?;
        if job.status != JobStatus::Running {
            return Err(Error::ConflictError("同步任务已停止".into()));
        }
        job.record_progress(0, 0, 0, Instant::now())?;
        self.db.background_jobs().update(&mut job, &mut NoTransaction).await?;
        Ok(())
    }

    async fn save_catalog_page(
        &self,
        connection: SupplierApiConnection,
        run: SupplierCatalogRun,
        page: PreparedPage,
        actor: &AuditActor,
    ) -> Result<()> {
        let cleanups = page.items.iter().map(|item| item.media.cleanup_handle()).collect::<Vec<_>>();
        let outcome = self.commit_catalog_page(connection, run, page, actor).await;
        for cleanup in cleanups {
            cleanup.after_transaction(&outcome).await;
        }
        outcome
    }

    async fn commit_catalog_page(
        &self,
        connection: SupplierApiConnection,
        run: SupplierCatalogRun,
        page: PreparedPage,
        actor: &AuditActor,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut current = SupplierApiService::new(db.clone())
                        .load_connection(&connection.base.id, executor)
                        .await?;
                    verify_connection(&current, &run)?;
                    SupplierApiService::new(db.clone()).persist_connection(&mut current, executor).await?;
                    let mut checkpoint = db
                        .supplier_catalog_runs()
                        .find_by_id(&run.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("同步记录不存在".into()))?;
                    if checkpoint.base.version != run.base.version {
                        return Err(Error::ConflictError("同步页面已被处理".into()));
                    }
                    let mut job = db
                        .background_jobs()
                        .find_by_id(&run.base.id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("同步任务不存在".into()))?;
                    if job.status != JobStatus::Running {
                        return Err(Error::ConflictError("同步任务已停止".into()));
                    }
                    save_page_items(&db, &mut checkpoint, page, &actor, executor).await?;
                    job.record_progress(0, 0, 0, Instant::now())?;
                    db.background_jobs().update(&mut job, executor).await?;
                    Ok(())
                })
            })
            .await
    }
}

/// 在当前页面事务中保存有效来源、异常和检查点。
async fn save_page_items(
    db: &Database,
    checkpoint: &mut SupplierCatalogRun,
    page: PreparedPage,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    checkpoint.advance(page.next, page.count)?;
    checkpoint.record_skipped(page.skipped)?;
    for observation in page.items {
        observation.media.pending_assets.persist(db, executor).await?;
        save_observation(db, observation.item, actor, executor).await?;
    }
    for withdrawal in page.withdrawn {
        apply_withdrawal(db, &checkpoint.connection_id, &checkpoint.base.id, withdrawal, actor, executor)
            .await?;
    }
    db.supplier_catalog_runs().update(checkpoint, executor).await?;
    Ok(())
}

fn verify_connection(connection: &SupplierApiConnection, run: &SupplierCatalogRun) -> Result<()> {
    if connection.base.id != run.connection_id
        || connection.technical_config_version != run.technical_config_version
    {
        return Err(Error::ConflictError("同步期间接口配置已变化，请重新同步".into()));
    }
    Ok(())
}

async fn save_observation(
    db: &Database,
    mut item: SupplierCatalogItem,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let repository = db.supplier_catalog_items();
    if let Some(existing) = repository.find_by_id(&item.base.id, executor).await? {
        let old_revision = item.source_revision.compare(&existing.source_revision) == Some(Ordering::Less);
        item.merge_observation(existing)?;
        if old_revision {
            return Ok(());
        }
        repository.update(&mut item, executor).await?;
    } else {
        repository.create(&item, executor).await?;
    }
    super::catalog_supply::apply_source_supply(db, &item, actor, executor).await?;
    crate::product_source_sync::apply_source_observation(db, &item, actor, executor).await?;
    Ok(())
}

async fn apply_withdrawal(
    db: &Database,
    connection: &str,
    job: &str,
    withdrawal: Withdrawal,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let product =
        withdrawal.sku.product_id.ok_or_else(|| Error::ValidationError("撤回记录缺少商品身份".into()))?;
    let id = SupplierCatalogItem::source_id(connection, &product, &withdrawal.sku.spec_id);
    let Some(mut item) = db.supplier_catalog_items().find_by_id(&id, executor).await? else { return Ok(()) };
    if item.withdraw(withdrawal.stamp.revision, withdrawal.stamp.observed_at, job)? {
        item.source_updated_at = withdrawal.stamp.changed_at;
        db.supplier_catalog_items().update(&mut item, executor).await?;
        super::catalog_supply::apply_source_supply(db, &item, actor, executor).await?;
    }
    Ok(())
}

fn classify(error: Error) -> ClassifiedError {
    match error {
        Error::ConflictError(_) => ClassifiedError {
            class: SupplierFailureClass::BusinessRejected,
            code: "CATALOG_CONTEXT_CHANGED".into(),
            summary: "同步任务、来源版本或接口配置已变化，请刷新后重新同步".into(),
        },
        Error::BusinessLogicError(message) | Error::ValidationError(message) => ClassifiedError {
            class: SupplierFailureClass::MappingError,
            code: "CATALOG_DATA_INVALID".into(),
            summary: message,
        },
        _ => storage_error(),
    }
}
fn storage_error() -> ClassifiedError {
    ClassifiedError {
        class: SupplierFailureClass::TransientFailure,
        code: "CATALOG_STORAGE_FAILED".into(),
        summary: "商品同步保存失败，已保存的商品保留；请继续同步或联系管理员".into(),
    }
}
