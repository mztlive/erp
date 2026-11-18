//! 分页读取及来源映射在事务外；来源、公司资料、检查点与任务心跳同事务。
use std::cmp::Ordering;

use application_core::AuditActor;
use erp_catalog::CatalogService;
use erp_core::common::time::Instant;
use erp_identity::AccessTarget;
use erp_permissions::supplier_api;
use erp_supply::catalog_membership::{
    CatalogMembershipPage, CatalogMembershipState, repository as memberships,
};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::{SupplierCatalogItem, SupplierCatalogRun};
use erp_supply::ports::catalog_sync::CatalogSyncPolicy;
use erp_supply::ports::connector::common::{Scan, ScanWindow, Snapshot, SourceStamp, SupplierSku};
use erp_supply::ports::connector::offer::{Offer, OfferChange};
use erp_supply::ports::supplier_api_gateway::{CatalogPage, ClassifiedError};
use erp_supply::repository::supplier_catalog::{self, SupplierCatalogExt};
use erp_supply::service::supplier_api::SupplierApiService;
use erp_support::{BackgroundJob, BulkJobExt, JobStatus};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction, Transactional};

use super::SupplierConnectionExecutionProcess;
use crate::{Error, Result};

struct Withdrawal {
    sku: SupplierSku,
    stamp: SourceStamp,
}
struct PreparedPage {
    membership: Option<CatalogMembershipPage>,
    items: Vec<SupplierCatalogItem>,
    withdrawn: Vec<Withdrawal>,
    next: Option<String>,
    count: u64,
    skipped: Vec<String>,
}

impl SupplierConnectionExecutionProcess {
    // 启动事务提交后，按任务冻结窗口顺序拉取并补齐目录资料，将页面逐批保存；当前页失败不撤销先前页面。
    //
    // 连接、已开始任务及真实任务操作人。
    //
    // 末页完成持久化时返回成功。
    //
    // 上游、映射、配置冲突或存储失败时返回脱敏分类错误。
    pub(super) async fn scan_catalog(
        &self,
        connection: &SupplierApiConnection,
        job: &BackgroundJob,
        actor: &AuditActor,
    ) -> std::result::Result<(), ClassifiedError> {
        let policy = self.catalog_policy(connection)?;
        self.prepare_scan(connection, &job.base.id, policy).await.map_err(classify)?;

        loop {
            let run = self
                .db
                .supplier_catalog_runs()
                .find_by_id(&job.base.id, &mut NoTransaction)
                .await
                .map_err(|_| storage_error())?
                .ok_or_else(storage_error)?;
            if run.finished {
                self.reconcile_catalog_memberships(&run, actor).await?;
                return Ok(());
            }

            if self.control.stopping() {
                return Err(ClassifiedError {
                    class: SupplierFailureClass::TransientFailure,
                    code: "CATALOG_SHUTDOWN".into(),
                    summary: "商品同步已暂停，系统将从已处理的位置继续，请稍后查看进度".into(),
                });
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
                limit: policy.page_size,
            };
            let page = self.gateway.catalog_page(&current, &scan).await?;
            let prepared = self.prepare_page(&current, &run.base.id, page).await.map_err(classify)?;

            self.save_catalog_page(current, run, prepared, actor).await.map_err(classify)?;
        }
    }

    async fn prepare_scan(
        &self,
        connection: &SupplierApiConnection,
        job: &str,
        policy: CatalogSyncPolicy,
    ) -> Result<()> {
        let mut run = self
            .db
            .supplier_catalog_runs()
            .find_by_id(job, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("同步记录不存在".into()))?;
        if !run.compatible(connection.technical_config_version, policy.mapping_version) {
            return Err(Error::ConflictError("同步配置、映射或检查点格式已变化，请重新同步".into()));
        }
        if run.pages != 0 || run.scan_since.is_some() {
            return Ok(());
        }

        let latest = supplier_catalog::successful_run(
            &self.db,
            &connection.base.id,
            connection.technical_config_version,
            policy.mapping_version,
            false,
            &mut NoTransaction,
        )
        .await?;
        let full = supplier_catalog::successful_run(
            &self.db,
            &connection.base.id,
            connection.technical_config_version,
            policy.mapping_version,
            true,
            &mut NoTransaction,
        )
        .await?;

        run.plan(policy, latest.as_ref(), full.as_ref());
        self.db.supplier_catalog_runs().update(&mut run, &mut NoTransaction).await?;
        Ok(())
    }

    async fn prepare_page(
        &self,
        connection: &SupplierApiConnection,
        job: &str,
        page: CatalogPage,
    ) -> Result<PreparedPage> {
        let count =
            u64::try_from(page.items.len()).map_err(|_| Error::ValidationError("来源商品数量超限".into()))?;
        let mut prepared = PreparedPage {
            membership: page.membership,
            items: vec![],
            withdrawn: vec![],
            next: page.next,
            count,
            skipped: page.skipped,
        };

        for snapshot in page.items {
            self.heartbeat_catalog_job(job).await?;
            match snapshot.value {
                OfferChange::Upsert(offer) => {
                    let item = self.prepare_catalog_observation(
                        connection,
                        job,
                        Snapshot { value: *offer, stamp: snapshot.stamp },
                    )?;
                    prepared.items.push(item);
                },
                OfferChange::Withdrawn(sku) => {
                    prepared.withdrawn.push(Withdrawal { sku, stamp: snapshot.stamp })
                },
            }
        }
        Ok(prepared)
    }

    fn prepare_catalog_observation(
        &self,
        connection: &SupplierApiConnection,
        job: &str,
        snapshot: Snapshot<Offer>,
    ) -> Result<SupplierCatalogItem> {
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

        if item.images.as_ref().is_some_and(|images| !images.is_empty())
            || item.detail_images.as_ref().is_some_and(|images| !images.is_empty())
        {
            item.media_retry_at = Some(Instant::now().unix_secs());
        }
        Ok(item)
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
        let db = self.db.clone();
        let client = db.client().clone();
        let actor = actor.clone();
        let catalog = self.catalog.clone();
        let authorization = self.authorization.clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(&actor, &supplier_api::ADOPTIONS_PREVIEW, AccessTarget::Operation, executor)
                        .await?;

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

                    save_page_items(&db, &catalog, &current, &mut checkpoint, page, &actor, executor).await?;
                    job.record_progress(0, 0, 0, Instant::now())?;
                    db.background_jobs().update(&mut job, executor).await?;
                    Ok(())
                })
            })
            .await
    }
}

// 页面资料在事务外补齐后，用本页事务写入有效来源观察、异常及检查点，避免检查点越过未保存的页面。
async fn save_page_items(
    db: &Database,
    catalog: &CatalogService,
    connection: &SupplierApiConnection,
    checkpoint: &mut SupplierCatalogRun,
    page: PreparedPage,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    save_membership_page(db, connection, checkpoint, &page, executor).await?;
    checkpoint.advance(page.next, page.count)?;
    checkpoint.record_skipped(page.skipped)?;

    for item in page.items {
        save_observation(db, catalog, item, actor, executor).await?;
    }
    for withdrawal in page.withdrawn {
        apply_withdrawal(db, &checkpoint.connection_id, &checkpoint.base.id, withdrawal, actor, executor)
            .await?;
    }

    db.supplier_catalog_runs().update(checkpoint, executor).await?;
    Ok(())
}

pub(super) fn verify_connection(connection: &SupplierApiConnection, run: &SupplierCatalogRun) -> Result<()> {
    if connection.base.id != run.connection_id
        || connection.technical_config_version != run.technical_config_version
    {
        return Err(Error::ConflictError("同步期间供应商连接设置已变化，请刷新设置后重新同步商品".into()));
    }
    Ok(())
}

async fn save_membership_page(
    db: &Database,
    connection: &SupplierApiConnection,
    checkpoint: &mut SupplierCatalogRun,
    page: &PreparedPage,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(proof) = &page.membership else { return Ok(()) };
    if proof.scan_finished != page.next.is_none() {
        return Err(Error::ConflictError("目录池完成证据与本页检查点不一致".into()));
    }

    let scan = memberships::record_page(
        db,
        &connection.base.id,
        connection.supplier_id.as_ref(),
        &checkpoint.base.id,
        proof,
        Instant::now(),
        executor,
    )
    .await?;
    if scan.pages != u64::from(checkpoint.pages) + 1 {
        return Err(Error::ConflictError("目录池成员与目录页面进度不一致".into()));
    }
    if scan.state == CatalogMembershipState::Uncertain {
        checkpoint.record_skipped(vec!["商品池授权范围需要核实；已保留本次观察，未按缺项撤回商品".into()])?;
    }
    Ok(())
}

// 目录同步或消息补查取得完整来源观察后，在调用方结果事务更新来源、已关联供给和已选品的公司资料。
//
// `db`、`item`、`actor` 为已核验来源与采集授权，`executor` 属于调用方结果事务。
//
// 来源观察、供给及已选品资料按原规则原子应用。
//
// 来源身份、版本、供给关联或仓储失败时拒绝。
pub(crate) async fn save_observation(
    db: &Database,
    catalog: &CatalogService,
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
    crate::product_source_sync::apply_source_observation(db, catalog, &item, actor, executor).await?;
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

pub(super) fn classify(error: Error) -> ClassifiedError {
    match error {
        Error::ConflictError(_) => ClassifiedError {
            class: SupplierFailureClass::BusinessRejected,
            code: "CATALOG_CONTEXT_CHANGED".into(),
            summary: "同步任务、供应商商品或连接设置已变化，请刷新后重新同步商品".into(),
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
