//! 域 D31 `mall_backfill` 服务编排（W30 历史消费回填）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 回填执行（§6.17：作业状态推进 + 后台任务登记（D04）+ 逐项回填明细 +
//!   进度统计原子写入）→ `database::Transactional::with_transaction`；
//! - 作业创建/列表查询单集合 → `&mut NoTransaction`（审计独立写入）。
//!
//! 幂等（§6.17）：每个切换的正式回填批次必须覆盖 `[range_start, T)`；重跑沿
//! 原任务和原范围续跑，禁止重叠批次；`(job_id, business_fact_key)` 唯一，
//! 重复键计为去重（`Duplicate`）不重复建项；`START` 的 `idempotency_key` 与
//! 正式任务唯一绑定（D04 `background_job.request_id` 唯一）。
//!
//! 跨域协作只调对方 Repository（P3-service-api.md §2）：
//! - D29 `MallOrderExt`：范围内关键事实与消费成本评估（逐项成本口径）；
//! - D28 `CardInstanceExt`：切换 `T`（`range_end` 必须等于 `T`）；
//! - D04 `BulkJobExt`：`background_job` 统一后台任务登记。

use database::{
    AccessControlExt, BulkJobExt, CardInstanceExt, MallBackfillExt, MallOrderExt, NoTransaction,
    Transactional,
};
use entities::common::time::Instant;
use entities::ids::{BackgroundJobId, MallConsumptionBackfillItemId, MallConsumptionBackfillJobId};
use entities::mall_backfill::{
    BackfillCostBasis, BackfillItemClassification, BackfillItemResult, BackfillJobStatus, BackfillWindow,
    MallConsumptionBackfillItem, MallConsumptionBackfillItemData, MallConsumptionBackfillJob,
    MallConsumptionBackfillJobData,
};
use entities::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use std::str::FromStr;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    BackfillCommand, BackfillCommandRequest, BackfillCommandResultView, BackfillItemListParams,
    BackfillItemView, BackfillJobDetailView, BackfillJobListParams, BackfillJobView,
    CreateBackfillJobRequest, PageView,
};

/// 回填作业列表筛选条件类型（经 `MallBackfillExt` 关联类型跨 crate 可达）。
type BackfillJobFilter = <mongodb::Database as MallBackfillExt>::MallConsumptionBackfillJobFilter;
/// 回填明细列表筛选条件类型。
type BackfillItemFilter = <mongodb::Database as MallBackfillExt>::MallConsumptionBackfillItemFilter;
/// 历史消费回填服务：作业管理、执行（START/RESUME）与明细查询。
pub struct MallBackfillService {
    db: Database,
}

impl MallBackfillService {
    /// 创建服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询回填作业列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`mall_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn backfill_job_list(
        &self,
        params: &BackfillJobListParams,
    ) -> Result<PageView<BackfillJobView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = BackfillJobFilter {
            mall_id: query.mall_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_consumption_backfill_jobs()
            .search_backfill_jobs(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| BackfillJobView {
                id: row.id,
                mall_id: row.mall_id,
                cutover_id: row.cutover_id.to_string(),
                range_start: row.range_start.unix_secs() as u64,
                range_end: row.range_end.unix_secs() as u64,
                status: row.status,
                total_count: row.total_count,
                total_amount: row.total_amount.to_string(),
                deduplicated_count: 0,
                actual_count: 0,
                standard_count: 0,
                none_count: 0,
                unattributed_count: 0,
                report_file_id: None,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建回填作业草稿（W30 创建回填任务）。
    ///
    /// 校验切换记录已启用且 `range_end` 等于 `T`、范围非空、同一商城无
    /// 覆盖重叠范围的正式批次（§6.17：禁止另一正式批次制造重叠）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建作业视图。
    ///
    /// # 错误
    /// * `NotFound` - 切换记录不存在或未启用
    /// * `BusinessLogicError` - `range_end` 不等于 `T`、存在重叠正式批次
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_backfill_job(
        &self,
        req: CreateBackfillJobRequest,
        actor: &AuditActor,
    ) -> Result<BackfillJobView> {
        req.validate()?;
        let cutover = self
            .db
            .mall_consumption_cutovers()
            .find_by_id(&req.cutover_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("切换记录不存在".to_string()))?;
        let window = BackfillWindow::for_cutover(
            Instant::from_unix_secs(req.range_start as i64),
            Instant::from_unix_secs(req.range_end as i64),
            cutover.status.is_enabled(),
            cutover.enabled_at,
        )?;
        let existing = self
            .db
            .mall_consumption_backfill_jobs()
            .list_overlapping_for_mall(&cutover.mall_id, window.start(), window.end(), &mut NoTransaction)
            .await?;
        let overlapping = existing.iter().any(|job| job.blocks_overlapping_batch(window));
        if overlapping {
            return Err(Error::BusinessLogicError(
                "已存在覆盖重叠范围的正式回填批次，请续跑原任务".to_string(),
            ));
        }

        let job = MallConsumptionBackfillJob::new(
            MallConsumptionBackfillJobId::new(next_id()),
            MallConsumptionBackfillJobData {
                mall_id: cutover.mall_id.clone(),
                cutover_id: req.cutover_id.clone(),
                range_start: window.start(),
                range_end: window.end(),
                total_count: req.total_count,
                total_amount: Amount::from_str(&req.total_amount)?,
            },
        )?;
        let audit = actor.clone().resource_log(
            "mall_consumption_backfill_job.create",
            "mall_consumption_backfill_job",
            job.base.id.clone(),
        )?;
        let job_for_tx = job.clone();
        crate::transaction::run_audited(&self.db, audit, move |db, session| {
            Box::pin(async move {
                db.mall_consumption_backfill_jobs()
                    .create(&job_for_tx, session)
                    .await?;
                Ok(())
            })
        })
        .await?;

        Ok(backfill_job_view(&job))
    }

    /// 查询回填作业详情（含明细总笔数）。
    ///
    /// # 参数
    /// * `id` - 作业 ID
    ///
    /// # 返回
    /// 返回作业详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 作业不存在
    pub async fn backfill_job_detail(&self, id: &str) -> Result<BackfillJobDetailView> {
        let job = self
            .db
            .mall_consumption_backfill_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("回填作业不存在".to_string()))?;
        let item_total = self
            .db
            .mall_consumption_backfill_items()
            .search_backfill_items(
                &BackfillItemFilter {
                    job_id: Some(job.base.id.clone().into()),
                    result: None,
                    cost_basis: None,
                    page: 1,
                    page_size: 1,
                    sort_by: None,
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await?
            .total;
        Ok(BackfillJobDetailView {
            job: backfill_job_view(&job),
            item_total_count: item_total,
        })
    }

    /// 分页查询回填明细列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`job_id`/`result`/`cost_basis` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn backfill_item_list(
        &self,
        params: &BackfillItemListParams,
    ) -> Result<PageView<BackfillItemView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = BackfillItemFilter {
            job_id: query.job_id,
            result: query.result,
            cost_basis: query.cost_basis,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .mall_consumption_backfill_items()
            .search_backfill_items(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| BackfillItemView {
                id: row.id,
                job_id: row.job_id.to_string(),
                business_fact_key: row.business_fact_key,
                source_event_reference: row.source_event_reference,
                mall_order_fact_id: row.mall_order_fact_id.map(|id| id.to_string()),
                result: row.result,
                cost_basis: row.cost_basis,
                error_code: None,
                error_detail: None,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 执行回填命令（`START`/`RESUME`，乐观锁 + 幂等键）。
    ///
    /// 一个事务内写入：作业状态推进 + `background_job` 登记（D04，`request_id`
    /// 幂等）+ 范围内关键事实的逐项回填明细 + 进度统计 + 审计。重复提交
    /// （同一 `idempotency_key`）返回既有任务结果，不重复启动。
    ///
    /// # 参数
    /// * `id` - 作业 ID
    /// * `req` - 命令请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回命令结果视图。
    ///
    /// # 错误
    /// * `NotFound` - 作业不存在
    /// * `ConflictError` - 期望版本过期或重复启动
    /// * `BusinessLogicError` - 命令与当前状态不匹配
    /// * `ValidationError` - 请求体校验失败
    pub async fn submit_backfill_command(
        &self,
        id: &str,
        req: BackfillCommandRequest,
        actor: &AuditActor,
    ) -> Result<BackfillCommandResultView> {
        req.validate()?;
        let job = self
            .db
            .mall_consumption_backfill_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("回填作业不存在".to_string()))?;
        if !job.has_version(req.version) {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        match req.command {
            BackfillCommand::Start if !job.status.accepts_start() => {
                return Err(Error::BusinessLogicError(format!(
                    "当前状态 {} 不允许开始回填",
                    job.status.label()
                )));
            }
            BackfillCommand::Resume if !job.status.accepts_resume() => {
                return Err(Error::BusinessLogicError(format!(
                    "当前状态 {} 不允许续跑",
                    job.status.label()
                )));
            }
            _ => {}
        }
        // 幂等：同一幂等键已登记后台任务时返回既有结果（结果未知时按原任务查询）。
        if self
            .db
            .background_jobs()
            .find_by_request_id(&req.idempotency_key, &mut NoTransaction)
            .await?
            .is_some()
        {
            return Ok(BackfillCommandResultView {
                status: "COMMITTED".to_string(),
                job_id: job.base.id.clone(),
                job_no: job.base.id.clone(),
                operation_id: req.operation_id,
                idempotency_key: req.idempotency_key,
                next_step: "重复提交命中既有任务，按原任务查询进度".to_string(),
            });
        }

        let audit = actor.clone().resource_log(
            "mall_consumption_backfill_job.submit",
            "mall_consumption_backfill_job",
            job.base.id.clone(),
        )?;
        let now = Instant::now();
        let job_id = job.base.id.clone();
        let job_id_for_tx = job_id.clone();
        let mall_id = job.mall_id.clone();
        let range_start = job.range_start;
        let range_end = job.range_end;
        let expected_version = req.version;
        let command = req.command;
        let idempotency_key = req.idempotency_key.clone();
        let idempotency_key_for_tx = idempotency_key.clone();
        let operation_id = req.operation_id.clone();
        let actor_id = actor.id().to_string();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut job = db
                        .mall_consumption_backfill_jobs()
                        .find_by_id(&job_id_for_tx, session)
                        .await?
                        .ok_or_else(|| Error::NotFound("回填作业不存在".to_string()))?;
                    if !job.has_version(expected_version) {
                        return Err(Error::ConflictError(
                            "数据已被其他请求修改，请刷新后重试".to_string(),
                        ));
                    }
                    match command {
                        BackfillCommand::Start => job.start()?,
                        BackfillCommand::Resume => job.resume()?,
                    }
                    let facts = db
                        .mall_order_facts()
                        .list_by_mall_and_occurred_range(&mall_id, range_start, range_end, session)
                        .await?;
                    let mut background = entities::bulk_job::BackgroundJob::new(
                        BackgroundJobId::new(next_id()),
                        entities::bulk_job::BackgroundJobData {
                            job_no: format!("HB-{job_id_for_tx}"),
                            job_type: entities::bulk_job::JobType::Backfill,
                            domain_job_type: Some("mall_consumption_backfill".to_string()),
                            domain_job_id: Some(job_id_for_tx.clone()),
                            selection_snapshot_id: None,
                            requested_by: actor_id.clone(),
                            request_id: idempotency_key_for_tx.clone(),
                            input_file_asset_id: None,
                            result_file_asset_id: None,
                            total_count: facts.len() as u64,
                        },
                    )?;
                    background.start(now)?;
                    let mut deduplicated = 0u64;
                    let mut actual = 0u64;
                    let mut standard = 0u64;
                    let mut none = 0u64;
                    let mut unattributed = 0u64;
                    let mut success = 0u64;
                    for fact in facts {
                        if db
                            .mall_consumption_backfill_items()
                            .find_by_job_and_key(
                                &job_id_for_tx.clone().into(),
                                &fact.business_fact_key,
                                session,
                            )
                            .await?
                            .is_some()
                        {
                            deduplicated += 1;
                            background.record_progress(0, 1, 0, now)?;
                            continue;
                        }
                        let classification = BackfillItemClassification::from_mall_fact(
                            fact.fact_type,
                            fact.processing_status,
                        );
                        let result = classification.result;
                        let basis = classification.cost_basis;
                        let item = MallConsumptionBackfillItem::new(
                            MallConsumptionBackfillItemId::new(next_id()),
                            MallConsumptionBackfillItemData {
                                job_id: job_id_for_tx.clone().into(),
                                business_fact_key: fact.business_fact_key.clone(),
                                source_event_reference: fact.source_event_id.clone(),
                                inbox_message_id: fact.inbox_message_id.clone(),
                                mall_order_fact_id: Some(fact.base.id.clone().into()),
                                result,
                                cost_basis: basis,
                                error_code: None,
                                error_detail: None,
                            },
                        )?;
                        db.mall_consumption_backfill_items()
                            .create(&item, session)
                            .await?;
                        success += 1;
                        match basis {
                            BackfillCostBasis::Actual => actual += 1,
                            BackfillCostBasis::Standard => standard += 1,
                            BackfillCostBasis::None => none += 1,
                        }
                        if result == BackfillItemResult::PendingAttribution {
                            unattributed += 1;
                        }
                    }
                    background.record_progress(success, 0, 0, now)?;
                    background.mark_succeeded(now)?;
                    db.background_jobs().update(&mut background, session).await?;

                    job.update_progress(deduplicated, actual, standard, none, unattributed, None)?;
                    job.transition_to(BackfillJobStatus::Completed)?;
                    db.mall_consumption_backfill_jobs()
                        .update(&mut job, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(BackfillCommandResultView {
            status: "COMMITTED".to_string(),
            job_id: job_id.clone(),
            job_no: job_id,
            operation_id,
            idempotency_key,
            next_step: "在任务详情查看处理进度与失败明细".to_string(),
        })
    }
}

/// 从作业实体构造视图（含进度统计）。
///
/// # 参数
/// * `job` - 作业实体
///
/// # 返回
/// 返回作业视图。
fn backfill_job_view(job: &MallConsumptionBackfillJob) -> BackfillJobView {
    BackfillJobView {
        id: job.base.id.clone(),
        mall_id: job.mall_id.clone(),
        cutover_id: job.cutover_id.to_string(),
        range_start: job.range_start.unix_secs() as u64,
        range_end: job.range_end.unix_secs() as u64,
        status: job.status,
        total_count: job.total_count,
        total_amount: job.total_amount.to_string(),
        deduplicated_count: job.deduplicated_count,
        actual_count: job.actual_count,
        standard_count: job.standard_count,
        none_count: job.none_count,
        unattributed_count: job.unattributed_count,
        report_file_id: job.report_file_id.as_ref().map(|id| id.to_string()),
        version: job.base.version,
        created_at: job.base.created_at,
    }
}
