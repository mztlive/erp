//! 商城销售单快照落盘编排（INT-R16）。
//!
//! Repository 批量提供 exact 键与 latest 最小事实，并在调用方事务内批量插入、
//! 单调推进来源单水位；Entity [`SnapshotIngestPlan`] 按事实身份去重再按版本
//! 时间分类。Service 保留作业状态、进度、审计与事务边界。

use database::{AccessControlExt, Executor, MallSyncExt, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::ids::{MallSalesOrderSnapshotId, MallSalesSyncJobId};
use entities::mall_sync::{
    ExternalOrderKey, MallSalesOrderSnapshot, MallSalesOrderSnapshotData, MallSalesSyncJob,
    SnapshotFactIdentity, SnapshotIngestPlan,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::{
    IngestMallSalesOrderSnapshotsRequest, IngestMallSalesOrderSnapshotsResult, SnapshotItemRequest,
};
use super::MallSyncService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 水位/作业 CAS 冲突后换新会话重试的上限。
const INGEST_TX_RETRY_LIMIT: u32 = 8;

impl MallSyncService {
    /// 落盘一页商城销售单快照（幂等）。
    ///
    /// 按事实键去重：重复推送（相同 `source_updated_at`）与早于最新快照的迟到
    /// 数据直接跳过（§6.13），不产生重复快照；同时推进作业处理计数。
    /// 快照内容创建后不可修改，映射状态保持待映射。
    ///
    /// # 参数
    /// * `req` - 落盘请求（作业 + 本页快照）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回本页落盘与跳过计数。
    ///
    /// # 错误
    /// * `NotFound` - 同步作业不存在
    /// * `BusinessLogicError` - 作业不在运行中
    /// * `ConflictError` - 并发作业版本冲突
    ///
    /// # 约束
    /// 批内 exact 重复与迟到不得使整批失败；并发旧版本由水位 CAS 拒绝落盘。
    pub async fn ingest_snapshots(
        &self,
        req: IngestMallSalesOrderSnapshotsRequest,
        actor: &AuditActor,
    ) -> Result<IngestMallSalesOrderSnapshotsResult> {
        req.validate()?;
        let job = self.load_running_sync_job(&req.sync_job_id).await?;
        self.persist_snapshot_page(job, req, actor).await
    }

    /// 装载仍接受快照的同步作业。
    ///
    /// # 参数
    /// * `sync_job_id` - 同步作业 ID
    ///
    /// # 返回
    /// 返回运行中的作业。
    ///
    /// # 错误
    /// 作业不存在或未在运行中时返回错误。
    async fn load_running_sync_job(&self, sync_job_id: &MallSalesSyncJobId) -> Result<MallSalesSyncJob> {
        let job = self
            .db
            .mall_sales_sync_jobs()
            .find_by_id(sync_job_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("同步作业不存在".to_string()))?;
        ensure_job_accepts_snapshots(&job)?;
        Ok(job)
    }

    /// 在事务内重分类、夺取水位并落盘接受项。
    ///
    /// # 参数
    /// * `job` - 预读的运行中作业
    /// * `req` - 落盘请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回本页实际落盘结果。
    ///
    /// # 错误
    /// 作业状态变化、版本 CAS 失败或写入失败时返回错误。
    async fn persist_snapshot_page(
        &self,
        job: MallSalesSyncJob,
        req: IngestMallSalesOrderSnapshotsRequest,
        actor: &AuditActor,
    ) -> Result<IngestMallSalesOrderSnapshotsResult> {
        let now = Instant::now();
        let audit = actor.clone().resource_log(
            "mall_sales_order_snapshot.create",
            "mall_sales_order_snapshot",
            job.base.id.clone(),
        )?;
        persist_snapshot_page_with_retry(
            self.db.client(),
            &self.db,
            &job.base.id,
            &job.source_system_id,
            &req.items,
            now,
            &audit,
        )
        .await
    }
}

/// 水位 E11000 / 作业 CAS / 瞬态冲突时中止当前会话并换新会话重试。
///
/// # 参数
/// * `client` - MongoDB 客户端
/// * `db` - 数据库
/// * `job_id` - 同步作业 ID
/// * `source_system_id` - 来源商城
/// * `items` - 本页入参
/// * `now` - 本页观察时间
/// * `audit` - 审计日志
///
/// # 返回
/// 返回实际落盘结果。
///
/// # 错误
/// 重试耗尽或不可重试错误时返回原错误。
async fn persist_snapshot_page_with_retry(
    client: &mongodb::Client,
    db: &mongodb::Database,
    job_id: &str,
    source_system_id: &entities::ids::SourceSystemId,
    items: &[SnapshotItemRequest],
    now: Instant,
    audit: &entities::AuditLog,
) -> Result<IngestMallSalesOrderSnapshotsResult> {
    let mut attempts = 0;
    loop {
        attempts += 1;
        match persist_snapshot_page_once(client, db, job_id, source_system_id, items, now, audit).await {
            Ok(result) => return Ok(result),
            Err(error) if attempts < INGEST_TX_RETRY_LIMIT && ingest_tx_should_retry(&error) => continue,
            Err(error) => return Err(error),
        }
    }
}

/// 开启新会话执行一次落盘事务。
///
/// # 参数
/// * `client` - MongoDB 客户端
/// * `db` - 数据库
/// * `job_id` - 同步作业 ID
/// * `source_system_id` - 来源商城
/// * `items` - 本页入参
/// * `now` - 本页观察时间
/// * `audit` - 审计日志
///
/// # 返回
/// 返回实际落盘结果。
///
/// # 错误
/// 作业状态变化、水位唯一键冲突或写入失败时返回错误；失败会话由
/// `with_transaction` 中止，不得再使用。
async fn persist_snapshot_page_once(
    client: &mongodb::Client,
    db: &mongodb::Database,
    job_id: &str,
    source_system_id: &entities::ids::SourceSystemId,
    items: &[SnapshotItemRequest],
    now: Instant,
    audit: &entities::AuditLog,
) -> Result<IngestMallSalesOrderSnapshotsResult> {
    let db = db.clone();
    let job_id = job_id.to_string();
    let source_system_id = source_system_id.clone();
    let items = items.to_vec();
    let audit = audit.clone();
    client
        .with_transaction(move |session| {
            let db = db.clone();
            let job_id = job_id.clone();
            let source_system_id = source_system_id.clone();
            let items = items.clone();
            let audit = audit.clone();
            Box::pin(async move {
                persist_snapshot_page_in_session(
                    &db,
                    &job_id,
                    &source_system_id,
                    &items,
                    now,
                    &audit,
                    session,
                )
                .await
            })
        })
        .await
}

/// 在调用方会话内重验作业、分类、夺取水位并写入。
///
/// # 参数
/// * `db` - 数据库
/// * `job_id` - 同步作业 ID
/// * `source_system_id` - 来源商城
/// * `items` - 本页入参
/// * `now` - 本页观察时间
/// * `audit` - 审计日志
/// * `session` - 事务会话
///
/// # 返回
/// 返回实际落盘结果。
///
/// # 错误
/// 作业不存在/未运行、CAS 失败或写入失败时返回错误。
async fn persist_snapshot_page_in_session(
    db: &mongodb::Database,
    job_id: &str,
    source_system_id: &entities::ids::SourceSystemId,
    items: &[SnapshotItemRequest],
    now: Instant,
    audit: &entities::AuditLog,
    session: &mut mongodb::ClientSession,
) -> Result<IngestMallSalesOrderSnapshotsResult> {
    let mut job = db
        .mall_sales_sync_jobs()
        .find_by_id(job_id, session)
        .await?
        .ok_or_else(|| Error::NotFound("同步作业不存在".to_string()))?;
    ensure_job_accepts_snapshots(&job)?;
    let candidates = candidate_identities(source_system_id, items);
    let plan = classify_snapshot_page_on(db, &candidates, session).await?;
    let snapshots = build_accepted_snapshots(&job, items, &plan, now)?;
    let claims: Vec<SnapshotFactIdentity> = snapshots
        .iter()
        .map(SnapshotFactIdentity::from_snapshot)
        .collect();
    let snapshots = keep_claimed_snapshots(
        snapshots,
        db.mall_sync().claim_snapshot_watermarks(&claims, session).await?,
    );
    write_accepted_snapshot_page(db, &mut job, &snapshots, items.len(), audit, session).await
}

/// 按仓储最小事实分类候选项。
///
/// # 参数
/// * `db` - 数据库
/// * `candidates` - 本页事实身份
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回分类计划。
///
/// # 错误
/// 仓储读取失败时返回错误。
async fn classify_snapshot_page_on(
    db: &mongodb::Database,
    candidates: &[SnapshotFactIdentity],
    executor: &mut dyn Executor,
) -> Result<SnapshotIngestPlan> {
    let scope = db.mall_sync().snapshot_ingest_scope(candidates, executor).await?;
    Ok(SnapshotIngestPlan::classify(
        candidates,
        &scope.exact_keys,
        &scope.latest,
    ))
}

/// 写入接受的快照、作业进度与审计；全跳过时仍累计页进度。
///
/// # 参数
/// * `db` - 数据库
/// * `job` - 事务内作业
/// * `snapshots` - 已夺取水位的快照
/// * `item_count` - 本页原始条数
/// * `audit` - 审计日志
/// * `session` - 事务会话
///
/// # 返回
/// 返回落盘结果。
///
/// # 错误
/// 写入或作业 CAS 失败时返回错误。
async fn write_accepted_snapshot_page(
    db: &mongodb::Database,
    job: &mut MallSalesSyncJob,
    snapshots: &[MallSalesOrderSnapshot],
    item_count: usize,
    audit: &entities::AuditLog,
    session: &mut mongodb::ClientSession,
) -> Result<IngestMallSalesOrderSnapshotsResult> {
    let accepted = snapshots.len() as u64;
    let (pages, items, skipped) = snapshot_page_progress(item_count as u64, accepted);
    if !snapshots.is_empty() {
        db.mall_sync().create_snapshots(snapshots, session).await?;
    }
    job.record_progress(pages, items, 0)?;
    db.mall_sales_sync_jobs().update(job, session).await?;
    db.audit_logs().create(audit, session).await?;
    Ok(IngestMallSalesOrderSnapshotsResult {
        accepted,
        skipped,
        snapshot_ids: snapshots
            .iter()
            .map(|snapshot| snapshot.base.id.clone())
            .collect(),
    })
}

/// 校验作业仍接受快照落盘。
///
/// # 参数
/// * `job` - 同步作业
///
/// # 返回
/// 运行中返回 `Ok(())`。
///
/// # 错误
/// 非运行中返回 `BusinessLogicError`。
fn ensure_job_accepts_snapshots(job: &MallSalesSyncJob) -> Result<()> {
    if job.accepts_snapshots() {
        Ok(())
    } else {
        Err(Error::BusinessLogicError(
            "同步作业不在运行中，禁止落盘快照".to_string(),
        ))
    }
}

/// 由请求行构造事实身份列表。
///
/// # 参数
/// * `source_system_id` - 来源商城
/// * `items` - 本页入参
///
/// # 返回
/// 返回与 `items` 等长的事实身份。
fn candidate_identities(
    source_system_id: &entities::ids::SourceSystemId,
    items: &[SnapshotItemRequest],
) -> Vec<SnapshotFactIdentity> {
    items
        .iter()
        .map(|item| {
            SnapshotFactIdentity::new(
                source_system_id.clone(),
                ExternalOrderKey::from_trimmed(&item.external_order_no),
                item.source_updated_at,
            )
        })
        .collect()
}

/// 为 Accept 项构造快照实体。
///
/// # 参数
/// * `job` - 来源作业
/// * `items` - 本页入参
/// * `plan` - 分类计划
/// * `now` - 本页观察时间
///
/// # 返回
/// 返回待插入快照。
///
/// # 错误
/// 实体校验失败时返回错误。
fn build_accepted_snapshots(
    job: &MallSalesSyncJob,
    items: &[SnapshotItemRequest],
    plan: &SnapshotIngestPlan,
    now: Instant,
) -> Result<Vec<MallSalesOrderSnapshot>> {
    let mut snapshots = Vec::new();
    for index in plan.accepted_indexes() {
        snapshots.push(snapshot_from_item(job, &items[index], now)?);
    }
    Ok(snapshots)
}

/// 由单条入参构造快照实体。
///
/// # 参数
/// * `job` - 来源作业
/// * `item` - 快照入参
/// * `now` - 观察时间
///
/// # 返回
/// 返回待映射快照。
///
/// # 错误
/// 必填文本为空或超长时返回错误。
fn snapshot_from_item(
    job: &MallSalesSyncJob,
    item: &SnapshotItemRequest,
    now: Instant,
) -> Result<MallSalesOrderSnapshot> {
    Ok(MallSalesOrderSnapshot::new(
        MallSalesOrderSnapshotId::new(next_id()),
        MallSalesOrderSnapshotData {
            source_system_id: job.source_system_id.clone(),
            external_order_no: item.external_order_no.clone(),
            source_updated_at: item.source_updated_at,
            content_hash: item.content_hash.clone(),
            source_status_code: item.source_status_code.clone(),
            normalized_snapshot: item.normalized_snapshot.clone(),
            raw_payload_reference: item.raw_payload_reference.clone(),
            observed_at: now,
            sync_job_id: MallSalesSyncJobId::new(job.base.id.clone()),
        },
    )?)
}

/// 仅保留夺取水位成功的快照。
///
/// # 参数
/// * `snapshots` - 分类为 Accept 的快照
/// * `claimed` - 与快照等长的水位夺得标记
///
/// # 返回
/// 返回应落盘快照。
fn keep_claimed_snapshots(
    snapshots: Vec<MallSalesOrderSnapshot>,
    claimed: Vec<bool>,
) -> Vec<MallSalesOrderSnapshot> {
    snapshots
        .into_iter()
        .zip(claimed)
        .filter_map(|(snapshot, won)| won.then_some(snapshot))
        .collect()
}

/// 计算本页应记入作业的页数、处理条数与跳过条数。
///
/// # 参数
/// * `item_count` - 本页原始条数
/// * `accepted` - 实际落盘条数
///
/// # 返回
/// 返回 `(pages, items, skipped)`；`items` 含跳过项，全跳过页仍记 1 页。
fn snapshot_page_progress(item_count: u64, accepted: u64) -> (u64, u64, u64) {
    let skipped = item_count.saturating_sub(accepted);
    (1, accepted + skipped, skipped)
}

/// 水位唯一键或作业 CAS / 瞬态冲突应换新会话重试。
///
/// # 参数
/// * `error` - 本轮事务错误
///
/// # 返回
/// 可重试返回 `true`。
fn ingest_tx_should_retry(error: &Error) -> bool {
    matches!(error, Error::TransientTransaction(_) | Error::ConflictError(_))
}

#[cfg(test)]
mod tests {
    use super::{
        candidate_identities, ingest_tx_should_retry, keep_claimed_snapshots, snapshot_page_progress,
    };
    use entities::common::time::Instant;
    use entities::ids::{MallSalesOrderSnapshotId, MallSalesSyncJobId, SourceSystemId};
    use entities::mall_sync::{
        ExternalOrderKey, MallSalesOrderSnapshot, MallSalesOrderSnapshotData, SnapshotFactIdentity,
        SnapshotIngestDecision, SnapshotIngestPlan,
    };

    use crate::errors::Error;
    use crate::mall_sync::dto::SnapshotItemRequest;

    fn item(order: &str, secs: i64) -> SnapshotItemRequest {
        SnapshotItemRequest {
            external_order_no: order.to_string(),
            source_updated_at: Instant::from_unix_secs(secs),
            content_hash: None,
            source_status_code: "EFFECTIVE".to_string(),
            normalized_snapshot: "{\"sell_order\":\"x\"}".to_string(),
            raw_payload_reference: None,
        }
    }

    fn snapshot(id: &str, order: &str, secs: i64) -> MallSalesOrderSnapshot {
        MallSalesOrderSnapshot::new(
            MallSalesOrderSnapshotId::new(id),
            MallSalesOrderSnapshotData {
                source_system_id: SourceSystemId::new("sys-mall"),
                external_order_no: order.to_string(),
                source_updated_at: Instant::from_unix_secs(secs),
                content_hash: None,
                source_status_code: "EFFECTIVE".to_string(),
                normalized_snapshot: "{\"sell_order\":\"x\"}".to_string(),
                raw_payload_reference: None,
                observed_at: Instant::from_unix_secs(secs + 1),
                sync_job_id: MallSalesSyncJobId::new("j-1"),
            },
        )
        .unwrap()
    }

    #[test]
    fn candidate_identities_trim_order_no_and_keep_source() {
        let source = SourceSystemId::new("sys-mall");
        let identities = candidate_identities(&source, &[item(" SO-1 ", 10), item("SO-2", 20)]);
        assert_eq!(
            identities,
            vec![
                SnapshotFactIdentity::new(
                    source.clone(),
                    ExternalOrderKey::from_trimmed("SO-1"),
                    Instant::from_unix_secs(10),
                ),
                SnapshotFactIdentity::new(
                    source,
                    ExternalOrderKey::from_trimmed("SO-2"),
                    Instant::from_unix_secs(20),
                ),
            ]
        );
    }

    #[test]
    fn keep_claimed_snapshots_drops_lost_watermark_races() {
        let kept = keep_claimed_snapshots(
            vec![snapshot("s-new", "SO-1", 20), snapshot("s-old", "SO-2", 10)],
            vec![true, false],
        );
        assert_eq!(kept.len(), 1);
        assert_eq!(kept[0].base.id, "s-new");
    }

    #[test]
    fn snapshot_page_progress_counts_skipped_pages() {
        assert_eq!(
            snapshot_page_progress(5, 0),
            (1, 5, 5),
            "全量幂等重复仍记处理条数"
        );
        assert_eq!(snapshot_page_progress(4, 0), (1, 4, 4), "迟到页仍记处理条数");
        assert_eq!(
            snapshot_page_progress(4, 3),
            (1, 4, 1),
            "水位未夺得的兄弟计入跳过"
        );
        assert_eq!(snapshot_page_progress(5, 2), (1, 5, 3));
    }

    #[test]
    fn ingest_tx_retries_conflict_and_transient_only() {
        assert!(ingest_tx_should_retry(&Error::ConflictError(
            "数据已存在，请勿重复提交".to_string()
        )));
        assert!(!ingest_tx_should_retry(&Error::NotFound(
            "同步作业不存在".to_string()
        )));
        assert!(!ingest_tx_should_retry(&Error::BusinessLogicError(
            "同步作业不在运行中，禁止落盘快照".to_string()
        )));
    }

    #[test]
    fn ingest_plan_duplicate_and_stale_count_as_skipped() {
        let source = SourceSystemId::new("sys-mall");
        let candidates =
            candidate_identities(&source, &[item("SO-1", 10), item("SO-1", 10), item("SO-1", 5)]);
        let plan = SnapshotIngestPlan::classify(&candidates, &[], &[]);
        assert_eq!(
            plan.decisions(),
            &[
                SnapshotIngestDecision::Accept,
                SnapshotIngestDecision::Duplicate,
                SnapshotIngestDecision::Stale,
            ]
        );
        assert_eq!(plan.skipped_count(), 2);
    }
}
