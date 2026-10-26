//! 分页读取在事务外；每页记录、检查点、心跳与配置复验在同一事务。
use std::num::NonZeroU32;

use erp_core::common::time::Instant;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::{SupplierCatalogItem, SupplierCatalogRun};
use erp_supply::ports::connector::common::{Page, Scan, ScanWindow, Snapshot};
use erp_supply::ports::connector::offer::OfferChange;
use erp_supply::ports::supplier_api_gateway::ClassifiedError;
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::SupplierConnectionExecutionProcess;
use crate::{Error, Result};

impl SupplierConnectionExecutionProcess {
    /// 顺序读取分页并保存观察；失败不擦除已提交页面。
    /// # 参数
    /// `connection` 是任务目标，`job` 是已开始的目录任务。
    /// # 返回
    /// 扫描末页保存完成时成功。
    /// # 错误
    /// 上游、映射、配置冲突或存储失败时返回脱敏分类错误。
    pub(super) async fn scan_catalog(
        &self,
        connection: &SupplierApiConnection,
        job: &BackgroundJob,
    ) -> std::result::Result<(), ClassifiedError> {
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
            verify_connection(&current, &run).map_err(|_| changed_error())?;
            let scan = Scan {
                window: ScanWindow::Full,
                cursor: run.cursor.clone(),
                limit: NonZeroU32::new(50).expect("50 非零"),
            };
            let page = self.gateway.catalog_page(&current, &scan).await?;
            self.save_catalog_page(current, run, page).await.map_err(|error| match error {
                Error::ConflictError(_) => changed_error(),
                Error::BusinessLogicError(message) | Error::ValidationError(message) => ClassifiedError {
                    class: SupplierFailureClass::MappingError,
                    code: "CATALOG_DATA_INVALID".into(),
                    summary: message,
                },
                _ => storage_error(),
            })?;
        }
    }

    /// 提交一页之前重验配置、任务状态和检查点版本；禁止并发重放覆盖。
    async fn save_catalog_page(
        &self,
        connection: SupplierApiConnection,
        run: SupplierCatalogRun,
        page: Page<Snapshot<OfferChange>>,
    ) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let items = catalog_items(&connection, &run.base.id, page.items)?;
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
                    checkpoint.advance(
                        page.next,
                        u64::try_from(items.len())
                            .map_err(|_| Error::BusinessLogicError("商品数溢出".into()))?,
                    )?;
                    for item in items {
                        save_observation(&db, item, executor).await?;
                    }
                    db.supplier_catalog_runs().update(&mut checkpoint, executor).await?;
                    job.record_progress(0, 0, 0, Instant::now())?;
                    db.background_jobs().update(&mut job, executor).await?;
                    Ok(())
                })
            })
            .await
    }
}

/// 校验已冻结的连接配置；业务停用不妨碍只读目录采集。
fn verify_connection(connection: &SupplierApiConnection, run: &SupplierCatalogRun) -> Result<()> {
    if connection.base.id != run.connection_id
        || connection.technical_config_version != run.technical_config_version
    {
        return Err(Error::ConflictError("同步期间接口配置已变化，请重新同步".into()));
    }
    Ok(())
}

/// 整页先校验，外部撤回语义必须显式支持后才能应用。
fn catalog_items(
    connection: &SupplierApiConnection,
    job: &str,
    items: Vec<Snapshot<OfferChange>>,
) -> Result<Vec<SupplierCatalogItem>> {
    items
        .into_iter()
        .map(|snapshot| {
            let OfferChange::Upsert(offer) = snapshot.value else {
                return Err(Error::BusinessLogicError("当前目录同步不支持供应商撤回记录".into()));
            };
            Ok(SupplierCatalogItem::observed(
                &connection.base.id,
                connection.supplier_id.as_ref(),
                job,
                Snapshot { value: offer, stamp: snapshot.stamp },
            )?)
        })
        .collect()
}

/// 只替换最近观察；保留人工匹配与稳定记录身份，不修改正式供给。
async fn save_observation(
    db: &Database,
    mut item: SupplierCatalogItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    let repository = db.supplier_catalog_items();
    if let Some(existing) = repository.find_by_id(&item.base.id, executor).await? {
        item.retain_match(existing)?;
        repository.update(&mut item, executor).await?;
    } else {
        repository.create(&item, executor).await?;
    }
    Ok(())
}
fn storage_error() -> ClassifiedError {
    ClassifiedError {
        class: SupplierFailureClass::TransientFailure,
        code: "CATALOG_STORAGE_FAILED".into(),
        summary: "商品同步保存失败，已保存的商品保留；请重新同步或联系管理员".into(),
    }
}
fn changed_error() -> ClassifiedError {
    ClassifiedError {
        class: SupplierFailureClass::BusinessRejected,
        code: "CATALOG_CONTEXT_CHANGED".into(),
        summary: "同步任务或接口配置已变化，请刷新后重新同步".into(),
    }
}
