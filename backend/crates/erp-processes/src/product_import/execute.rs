//! 领取并执行商品导入后台任务。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::AccountKind;
use erp_core::common::time::Instant;
use erp_core::ids::BackgroundJobId;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobItem, BulkJobExt, FileAssetExt, ItemStatus, JobStatus,
    PRODUCT_IMPORT_DOMAIN_JOB_TYPE,
};
use persistence_core::{NoTransaction, Transactional};

use super::ProductImportProcess;
use super::images::RowMediaSource;
use super::parse::{ParsedProductSheet, parse_product_quote_xlsx};
use super::resolve::ImportDictionaryCache;
use super::row_manifest::{
    ROW_MANIFEST_INPUT_FORMAT, RowManifest, RowManifestRow, decode_row_manifest, read_row_manifest,
    row_entries_by_number, row_media_from_entry,
};
use crate::{Error, Result};

mod rows;
use rows::{ImportRowExecution, execute_rows};

impl ProductImportProcess {
    /// 领取待处理的商品导入任务并逐行执行。
    ///
    /// 仅由统一后台执行器调用；前台提交只投递任务，不直接执行。
    /// 并发认领冲突视为已被其他执行器接管，记 info 后跳过，不记为任务失败。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本轮认领的任务数。
    ///
    /// # 错误
    /// 查询失败时返回错误；单个任务失败记日志后继续。
    pub async fn run_due_jobs(&self) -> Result<usize> {
        let jobs = self
            .db
            .background_jobs()
            .list_open_by_domain_job_type(PRODUCT_IMPORT_DOMAIN_JOB_TYPE, 4, &mut NoTransaction)
            .await?;
        let count = jobs.len();
        for job in jobs {
            if let Err(error) = self.execute_job(&job.base.id).await {
                if is_concurrent_claim(&error) {
                    tracing::info!(job_id = %job.base.id, "商品导入任务已被其他执行器认领，跳过");
                    continue;
                }
                tracing::error!(job_id = %job.base.id, error = %error, "商品导入任务执行失败");
            }
        }
        Ok(count)
    }

    /// 执行单个导入任务中尚未完成的行。
    ///
    /// 认领阶段通过乐观锁保证单执行器推进；`Pending` 转 `Running` 冲突时返回 `Ok` 表示已被接管。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    ///
    /// # 返回
    /// 无；被其他执行器认领时直接返回，不视为错误。
    ///
    /// # 错误
    /// 源文件不存在、解析失败或状态迁移失败时返回错误。
    pub async fn execute_job(&self, job_id: &str) -> Result<()> {
        let mut job = self
            .db
            .background_jobs()
            .find_by_id(job_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入任务不存在".into()))?;
        if job.domain_job_type.as_deref() != Some(PRODUCT_IMPORT_DOMAIN_JOB_TYPE) {
            return Ok(());
        }
        if job.status == JobStatus::Pending {
            job.start(Instant::now())?;
            if let Err(error) = self.db.background_jobs().update(&mut job, &mut NoTransaction).await {
                if matches!(error, persistence_core::Error::OptimisticLockingError) {
                    return Ok(());
                }
                return Err(error.into());
            }
        }
        if !matches!(job.status, JobStatus::Running | JobStatus::PartiallySucceeded) {
            return Ok(());
        }
        let items = self
            .db
            .background_job_items()
            .list_entities_by_job(&BackgroundJobId::new(job.base.id.clone()), &mut NoTransaction)
            .await?;
        let actor = AuditActor::new(job.requested_by.clone(), job.requested_by.clone(), AccountKind::Admin);
        let mut cache = ImportDictionaryCache::default();
        let remaining = items.iter().filter(|item| item.status.is_none()).count();
        if remaining == 0 {
            if job.processed_count == job.total_count
                && matches!(job.status, JobStatus::Running | JobStatus::PartiallySucceeded)
            {
                job.record_import_result_batch(0, 0, 0, true, Instant::now())?;
                self.db.background_jobs().update(&mut job, &mut NoTransaction).await?;
            }
            return Ok(());
        }
        let row_source = self.load_row_source(&job).await?;
        self.execute_pending_items(&mut job, items, &actor, &mut cache, &row_source).await
    }

    /// 按行执行并立即保存结果；上一行保存完成后才允许执行下一行。
    ///
    /// # 参数
    /// * `job` - 本轮持有并持续推进的任务实体
    /// * `items` - 任务全部明细
    /// * `actor` - 审计操作人
    /// * `cache` - 字典缓存
    /// * `row_source` - 行来源
    ///
    /// # 返回
    /// 无；被其他执行器认领时直接返回。
    ///
    /// # 错误
    /// 行状态迁移或结果保存失败时返回错误。
    async fn execute_pending_items(
        &self,
        job: &mut BackgroundJob,
        items: Vec<BackgroundJobItem>,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
        row_source: &JobRowSource,
    ) -> Result<()> {
        let manifest_entries;
        let legacy_workbook;
        let (entries, workbook) = match row_source {
            JobRowSource::Manifest(m) => {
                manifest_entries = row_entries_by_number(m);
                (Some(&manifest_entries), None)
            },
            JobRowSource::Legacy { bytes, parsed } => {
                legacy_workbook = (bytes, parsed);
                (None, Some(&legacy_workbook))
            },
        };
        let legacy_cells = workbook.map(|(_, p)| {
            p.rows.iter().map(|r| (r.row_number, r.cells.as_slice())).collect::<HashMap<u32, &[String]>>()
        });
        let mut execution =
            ProductImportRows { process: self, actor, cache, entries, workbook, legacy_cells };
        execute_rows(&mut execution, job, items).await
    }

    /// 执行单行的业务写入并转为可持久化结果。
    ///
    /// # 参数
    /// * `row_number` - Excel 行号
    /// * `entries` - 清单行映射
    /// * `workbook` - 源文件字节与解析结果
    /// * `legacy_cells` - 源文件行号到单元格的预建映射
    /// * `actor` - 审计操作人
    /// * `cache` - 字典缓存
    ///
    /// # 返回
    /// 返回明细状态、原因码与结果对象。
    async fn import_single_row(
        &self,
        row_number: u32,
        entries: Option<&HashMap<u32, &RowManifestRow>>,
        workbook: Option<&(&Arc<Vec<u8>>, &ParsedProductSheet)>,
        legacy_cells: &Option<HashMap<u32, &[String]>>,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
    ) -> RecordedOutcome {
        match (entries, workbook) {
            (Some(map), _) => self.import_manifest_row(row_number, map, actor, cache).await,
            (_, Some((bytes, parsed))) => {
                let cells = legacy_cells.as_ref().and_then(|m| m.get(&row_number).copied()).unwrap_or(&[]);
                let media = RowMediaSource::Workbook { xlsx: bytes, sheet: parsed };
                record_row_outcome(self.import_row(row_number, cells, &media, actor, cache).await)
            },
            (None, None) => unreachable!("行来源必为清单或源文件之一"),
        }
    }

    /// 行来源：清单优先，老任务回退到源文件。
    ///
    /// 清单由提交时写入，执行阶段只读清单；清单缺失或损坏
    /// （部署前创建的老任务）时回退到源文件下载解析。
    ///
    /// # 参数
    /// * `job` - 后台任务实体
    ///
    /// # 返回
    /// 返回清单或源文件行来源。
    ///
    /// # 错误
    /// 私有输入读取失败时保留持久化错误；其余清单与源文件均不可用时返回错误。
    async fn load_row_source(&self, job: &BackgroundJob) -> Result<JobRowSource> {
        let job_id = BackgroundJobId::new(job.base.id.clone());
        let input =
            self.db.bulk_job().job_input(&job_id, ROW_MANIFEST_INPUT_FORMAT, &mut NoTransaction).await?;
        if let Some(bytes) = input {
            match decode_row_manifest(&bytes, &job.request_id) {
                Ok(manifest) => return Ok(JobRowSource::Manifest(manifest)),
                Err(error) => tracing::info!(job_id = %job.base.id, error = %error,
                    "私有行清单不可用，回退到对象清单链路"),
            }
        }
        match read_row_manifest(&self.storage, &job.request_id).await {
            Ok(manifest) => Ok(JobRowSource::Manifest(manifest)),
            Err(error) => {
                tracing::info!(
                    job_id = %job.base.id,
                    error = %error,
                    "行清单不可用，回退到源文件链路",
                );
                let (bytes, parsed) = self.load_source(job).await?;
                Ok(JobRowSource::Legacy { bytes, parsed })
            },
        }
    }

    /// 执行清单中的一行（单元格与媒体均来自清单，不接触源文件）。
    ///
    /// # 参数
    /// * `row_number` - Excel 行号
    /// * `entries` - 行号到清单行的映射
    /// * `actor` - 审计操作人
    /// * `cache` - 字典缓存
    ///
    /// # 返回
    /// 返回可持久化的明细结果；清单缺行或图片预提失败记该行失败。
    async fn import_manifest_row(
        &self,
        row_number: u32,
        entries: &HashMap<u32, &RowManifestRow>,
        actor: &AuditActor,
        cache: &mut ImportDictionaryCache,
    ) -> RecordedOutcome {
        let Some(entry) = entries.get(&row_number) else {
            return RecordedOutcome {
                status: ItemStatus::Failed,
                code: Some("manifest_missing".to_string()),
                summary: Some("行数据缺失，请重新提交导入".to_string()),
                object_type: None,
                object_id: None,
            };
        };
        if let Some(media_error) = &entry.media_error {
            return RecordedOutcome {
                status: ItemStatus::Failed,
                code: Some("media_unavailable".to_string()),
                summary: Some(media_error.clone()),
                object_type: None,
                object_id: None,
            };
        }
        let media = row_media_from_entry(entry);
        let media_source = RowMediaSource::Manifest(&media);
        record_row_outcome(self.import_row(row_number, &entry.cells, &media_source, actor, cache).await)
    }

    /// 加载任务源文件并解析报价表（老任务回退链路）。
    ///
    /// # 参数
    /// * `job` - 后台任务实体
    ///
    /// # 返回
    /// 返回源文件字节与解析后的工作表。
    ///
    /// # 错误
    /// 源文件缺失、读取失败或解析失败时返回错误。
    async fn load_source(&self, job: &BackgroundJob) -> Result<(Arc<Vec<u8>>, ParsedProductSheet)> {
        let file_id = job
            .input_file_asset_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("导入任务缺少源文件".into()))?;
        let asset = self
            .db
            .file_assets()
            .find_by_id(file_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("导入源文件不存在".into()))?;
        let bytes = self
            .storage
            .read(&asset.storage_object_key)
            .await
            .map_err(|error| Error::Internal(format!("读取导入文件失败: {error}")))?;
        let bytes = Arc::new(bytes);
        let parse_bytes = bytes.clone();
        let parsed = tokio::task::spawn_blocking(move || parse_product_quote_xlsx(&parse_bytes))
            .await
            .map_err(|_| Error::Internal("解析导入文件失败".into()))??;
        Ok((bytes, parsed))
    }

    /// 在同一事务内保存单行结果和任务进度，返回已推进版本的任务。
    ///
    /// # 参数
    /// * `job` - 本轮持有的任务版本及累计结果
    /// * `item` - 刚完成的单行结果
    ///
    /// # 返回
    /// 成功后沿用仓储已更新的任务版本，无需额外回读。
    ///
    /// # 错误
    /// 取消或并发执行造成的版本冲突、结果写入或提交失败时返回错误。
    async fn persist_progress(&self, job: &mut BackgroundJob, mut item: BackgroundJobItem) -> Result<()> {
        let db = self.db.clone();
        let client = db.client().clone();
        let mut updated = job.clone();
        *job = client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.background_job_items().update(&mut item, executor).await?;
                    db.background_jobs().update(&mut updated, executor).await?;
                    Ok::<_, Error>(updated)
                })
            })
            .await?;
        Ok(())
    }
}

struct RecordedOutcome {
    status: ItemStatus,
    code: Option<String>,
    summary: Option<String>,
    object_type: Option<String>,
    object_id: Option<String>,
}

/// 任务行来源：提交时写入的清单，或老任务的源文件。
enum JobRowSource {
    /// 行级清单（含单元格与已上传图片引用）。
    Manifest(RowManifest),
    /// 源文件字节与解析结果（老任务回退）。
    Legacy {
        /// 源文件字节。
        bytes: Arc<Vec<u8>>,
        /// 解析后的工作表。
        parsed: ParsedProductSheet,
    },
}

/// 并发认领冲突的稳定文案，与持久层乐观锁映射保持一致。
const CONCURRENT_CLAIM_MESSAGE: &str = "数据已被其他请求修改，请刷新后重试";

/// 判断是否为并发认领冲突。
///
/// # 参数
/// * `error` - 待判断的流程错误
///
/// # 返回
/// 乐观锁冲突时返回 `true`，其余错误返回 `false`。
fn is_concurrent_claim(error: &Error) -> bool {
    matches!(error, Error::ConflictError(message) if message == CONCURRENT_CLAIM_MESSAGE)
}

/// 把单行导入结果转为可持久化的明细结果。
///
/// # 参数
/// * `outcome` - 单行导入的业务结果
///
/// # 返回
/// 返回明细状态、原因码与结果对象。
fn record_row_outcome(outcome: Result<super::row::RowImportOutcome>) -> RecordedOutcome {
    match outcome {
        Ok(result) if result.skipped => {
            recorded(ItemStatus::Skipped, None, result.message, result.product_id)
        },
        Ok(result) => recorded(ItemStatus::Success, None, result.message, result.product_id),
        Err(error) => recorded(ItemStatus::Failed, Some("import_failed"), error.to_string(), None),
    }
}

/// 组装明细结果记录。
///
/// # 参数
/// * `status` - 明细状态
/// * `code` - 失败原因码，成功或跳过时为空
/// * `summary` - 结果说明
/// * `product_id` - 成功或跳过时关联的商品 ID
///
/// # 返回
/// 返回可写入明细实体的结果。
fn recorded(
    status: ItemStatus,
    code: Option<&str>,
    summary: String,
    product_id: Option<String>,
) -> RecordedOutcome {
    let (object_type, object_id) = match product_id {
        Some(id) => (Some("product".to_string()), Some(id)),
        None => (None, None),
    };
    RecordedOutcome { status, code: code.map(str::to_string), summary: Some(summary), object_type, object_id }
}

/// 本轮导入的行来源与字典缓存；执行和保存由逐行编排统一控制。
struct ProductImportRows<'a> {
    process: &'a ProductImportProcess,
    actor: &'a AuditActor,
    cache: &'a mut ImportDictionaryCache,
    entries: Option<&'a HashMap<u32, &'a RowManifestRow>>,
    workbook: Option<&'a (&'a Arc<Vec<u8>>, &'a ParsedProductSheet)>,
    legacy_cells: Option<HashMap<u32, &'a [String]>>,
}

#[async_trait]
impl ImportRowExecution for ProductImportRows<'_> {
    /// 使用本轮行来源与字典缓存执行单行导入。
    async fn import(&mut self, row_number: u32) -> RecordedOutcome {
        self.process
            .import_single_row(
                row_number,
                self.entries,
                self.workbook,
                &self.legacy_cells,
                self.actor,
                self.cache,
            )
            .await
    }

    /// 将当前行结果与累计任务进度交给原事务边界保存。
    async fn persist(&mut self, job: &mut BackgroundJob, item: BackgroundJobItem) -> Result<()> {
        self.process.persist_progress(job, item).await
    }
}
