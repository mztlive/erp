//! 持久化准备队列：先占用册版本，再后台执行；过期、重启和迟到结果均有持久化边界。
use super::{IdempotencyStoreInput, SalesSelectionService};
use crate::dto::sales_selection::{PrepareSalesSelectionRequest, SalesSelectionBookletView};
use crate::entity::sales_selection::{
    normalize_idempotency_key, request_hash, FirstNonEmptyMemberImage, IdempotencyOperation,
    PackageImageGenerator, PrepareKind, PrepareStage, PrepareTaskStatus, SalesSelectionBooklet,
    SalesSelectionDisplayItem, SalesSelectionPoolMember, SalesSelectionPrepareTask,
    SalesSelectionPrepareTaskData, TierSearchReport,
};
use crate::ports::sales_selection::{SelectionCatalogPort, SelectionImagePort};
use crate::{repository::SalesSelectionExt, Error, Result};
use erp_core::{
    common::time::{BusinessDate, Instant},
    ids::{SalesSelectionBookletId, SalesSelectionPrepareTaskId},
};
use persistence_core::{Executor, NoTransaction, Transactional};
use std::sync::Arc;

/// 完整计算结果仅在通过任务身份、运行代数、册版本和期限检查后提交。
struct Prepared {
    book: SalesSelectionBooklet,
    batch: String,
    items: Vec<SalesSelectionDisplayItem>,
    members: Vec<SalesSelectionPoolMember>,
    reports: Vec<TierSearchReport>,
}

impl SalesSelectionService {
    /// 原子建立排队任务和准备中状态，立即返回，不在请求内执行外部 I/O。
    /// # 参数
    /// req 为完整命令，actor_id 为当前用户；端口由 worker 执行时使用。
    /// # 返回
    /// 返回可轮询的准备中详情；同请求重试返回已建立的任务事实。
    /// # 错误
    /// 旧版本、非法规则或事务失败时整次拒绝。
    pub async fn start_prepare(
        &self,
        req: PrepareSalesSelectionRequest,
        actor_id: &str,
        _catalog: &dyn SelectionCatalogPort,
        _images: &dyn SelectionImagePort,
        _generator: &dyn PackageImageGenerator,
    ) -> Result<SalesSelectionBookletView> {
        let service = Self::new(self.db.clone());
        let actor = actor_id.to_string();
        self.db
            .client()
            .with_transaction(move |tx| {
                Box::pin(async move { service.enqueue_prepare(req, &actor, tx).await })
            })
            .await
    }

    /// 校验规则时只修改副本，原有效规则一直保留到任务成功。
    async fn enqueue_prepare(
        &self,
        req: PrepareSalesSelectionRequest,
        actor: &str,
        tx: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let key = normalize_idempotency_key(&req.idempotency_key)?;
        let json = serde_json::to_string(&req).map_err(|e| Error::Internal(e.to_string()))?;
        let hash = request_hash(&json);
        if let Some(view) = self
            .replay_idempotency(IdempotencyOperation::Prepare, actor, &key, &hash, tx)
            .await?
        {
            return Ok(view);
        }
        let mut book = self.load_booklet(&req.booklet_id, tx).await?;
        book.ensure_version(req.expected_version)
            .map_err(|error| Error::selection_conflict(error.to_string()))?;
        validate_prepare_request(&book, &req)?;
        let id = SalesSelectionPrepareTaskId::new(id_generator::next_id());
        book.begin_prepare(id.as_ref(), req.kind, actor)?;
        self.db.sales_selection_booklets().update(&mut book, tx).await?;
        let task = queued_task(id, &book, &req, actor, json, key, hash);
        self.db.sales_selection_prepare_tasks().create(&task, tx).await?;
        self.queued_view(&book, &task, tx).await
    }

    /// 将可轮询详情和原请求的幂等结果一起持久化。
    async fn queued_view(
        &self,
        book: &SalesSelectionBooklet,
        task: &SalesSelectionPrepareTask,
        tx: &mut dyn Executor,
    ) -> Result<SalesSelectionBookletView> {
        let view = self.detail_view(book, None, tx).await?;
        self.store_idempotency(
            IdempotencyStoreInput {
                operation: IdempotencyOperation::Prepare,
                scope_id: &task.actor_id,
                key: &task.idempotency_key,
                hash: &task.request_hash,
                result: &view,
                token_version: None,
                booklet_id: Some(task.booklet_id.clone()),
            },
            tx,
        )
        .await?;
        Ok(view)
    }

    /// 领取排队任务并执行；运行中任务在重启后于截止时间明确失败，不重新取得商品快照。
    /// # 返回
    /// 本次处理的任务和到期关闭数量。
    /// # 错误
    /// 仓储错误返回给 worker；单任务计算失败会原子记录并恢复业务状态。
    pub async fn run_due_prepare_tasks(
        &self,
        catalog: &dyn SelectionCatalogPort,
        images: &dyn SelectionImagePort,
    ) -> Result<u32> {
        let mut tasks = self
            .db
            .sales_selection_prepare_tasks()
            .list_active(&mut NoTransaction)
            .await?;
        tasks.sort_by_key(|task| task.deadline_at);
        let mut count = self.close_expired_published().await?;
        for task in tasks {
            count += u32::from(self.run_queued_task(task, catalog, images).await?);
        }
        Ok(count)
    }

    /// 过期先失败；乐观锁只允许一个 worker 领取排队任务。
    async fn run_queued_task(
        &self,
        mut task: SalesSelectionPrepareTask,
        catalog: &dyn SelectionCatalogPort,
        images: &dyn SelectionImagePort,
    ) -> Result<bool> {
        if task.is_deadline_passed(Instant::now()) {
            self.fail_task(&task, "准备任务已超过时限").await?;
            return Ok(true);
        }
        if task.status != PrepareTaskStatus::Queued {
            return Ok(false);
        }
        task.mark_running(Instant::now())?;
        match self
            .db
            .sales_selection_prepare_tasks()
            .update(&mut task, &mut NoTransaction)
            .await
        {
            Ok(_) => (),
            Err(persistence_core::Error::OptimisticLockingError) => return Ok(false),
            Err(error) => return Err(error.into()),
        }
        self.execute_task(task, catalog, images).await?;
        Ok(true)
    }

    /// 外部 I/O 和 CPU 计算位于事务之外，deadline 包含排队耗时。
    async fn execute_task(
        &self,
        task: SalesSelectionPrepareTask,
        catalog: &dyn SelectionCatalogPort,
        images: &dyn SelectionImagePort,
    ) -> Result<()> {
        let remaining = task
            .deadline_at
            .unix_secs()
            .saturating_sub(Instant::now().unix_secs())
            .max(0) as u64;
        let outcome = tokio::time::timeout(
            std::time::Duration::from_secs(remaining),
            self.calculate_task(&task, catalog, images),
        )
        .await;
        match outcome {
            Ok(Ok(prepared)) => {
                if let Err(error) = self.commit_task(&task, prepared).await {
                    tracing::warn!(task_id = %task.base.id, error = %error, "准备结果未能提交");
                    self.fail_task(&task, "准备结果未能保存，请重新准备").await?;
                }
            }
            Ok(Err(error)) => self.fail_task(&task, &error.to_string()).await?,
            Err(_) => self.fail_task(&task, "准备任务已超过时限").await?,
        }
        Ok(())
    }

    /// 用任务保存的命令和当前有效批次计算，不覆盖原规则。
    async fn calculate_task(
        &self,
        task: &SalesSelectionPrepareTask,
        catalog: &dyn SelectionCatalogPort,
        images: &dyn SelectionImagePort,
    ) -> Result<Prepared> {
        let req: PrepareSalesSelectionRequest = serde_json::from_str(&task.request_json)
            .map_err(|_| Error::selection_prepare_failed("准备任务缺少有效命令，请重新准备"))?;
        let mut book = self
            .load_booklet(task.booklet_id.as_ref(), &mut NoTransaction)
            .await?;
        ensure_task_book(task, &book)?;
        if matches!(req.kind, PrepareKind::FirstPrepare | PrepareKind::RePrepare) {
            super::prepare::apply_reprepare_request(&mut book, &req)?;
        }
        let batch = self.resolve_batch_id(&book, req.kind);
        let snapshots = self.freeze_pool(&book, &batch, req.kind, catalog, images).await?;
        self.prepare_progress(&book, PrepareStage::Search, 0).await?;
        let (items, members, reports) = self
            .build_displays(
                &book,
                &batch,
                &snapshots,
                &req,
                Arc::new(FirstNonEmptyMemberImage),
            )
            .await?;
        self.prepare_progress(&book, PrepareStage::Write, reports.len() as u32)
            .await?;
        Ok(Prepared {
            book,
            batch,
            items,
            members,
            reports,
        })
    }

    /// 阶段更新也校验任务仍活动且册版本未漂移。
    pub(super) async fn prepare_progress(
        &self,
        book: &SalesSelectionBooklet,
        stage: PrepareStage,
        completed: u32,
    ) -> Result<()> {
        let id = book
            .active_task_id
            .as_deref()
            .ok_or_else(|| Error::selection_prepare_failed("准备任务已结束"))?;
        let mut task = self
            .db
            .sales_selection_prepare_tasks()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("准备任务不存在".into()))?;
        task.ensure_writable_run(task.run_version, Instant::now())?;
        ensure_task_book(&task, book)?;
        task.heartbeat(stage, completed, Instant::now());
        self.db
            .sales_selection_prepare_tasks()
            .update(&mut task, &mut NoTransaction)
            .await?;
        Ok(())
    }

    /// 迟到、重复或过期结果均不能替换有效结果。
    async fn commit_task(&self, run: &SalesSelectionPrepareTask, prepared: Prepared) -> Result<()> {
        let service = Self::new(self.db.clone());
        let run = run.clone();
        self.db
            .client()
            .with_transaction(move |tx| {
                Box::pin(async move { service.commit_task_in(&run, prepared, tx).await })
            })
            .await
    }

    /// 在同一事务重新校验当前任务和册，再替换目标范围。
    async fn commit_task_in(
        &self,
        run: &SalesSelectionPrepareTask,
        mut prepared: Prepared,
        tx: &mut dyn Executor,
    ) -> Result<()> {
        let mut task = self
            .db
            .sales_selection_prepare_tasks()
            .find_by_id(&run.base.id, tx)
            .await?
            .ok_or_else(|| Error::NotFound("准备任务不存在".into()))?;
        task.ensure_writable_run(run.run_version, Instant::now())?;
        let current = self.load_booklet(run.booklet_id.as_ref(), tx).await?;
        ensure_task_book(&task, &current)?;
        prepared.finish(&mut task)?;
        crate::repository::sales_selection::SalesSelectionDomainRepository::new(&self.db)
            .persist_prepare_success(&task, &mut prepared.book, &prepared.members, &prepared.items, tx)
            .await?;
        Ok(())
    }

    /// 失败恢复与任务结束共同提交。
    async fn fail_task(&self, run: &SalesSelectionPrepareTask, reason: &str) -> Result<()> {
        let service = Self::new(self.db.clone());
        let (run, reason) = (run.clone(), reason.to_string());
        self.db
            .client()
            .with_transaction(move |tx| Box::pin(async move { service.fail_task_in(&run, reason, tx).await }))
            .await
    }

    /// 已完成或被替代的任务不再触碰册，历史任务使用系统操作人。
    async fn fail_task_in(
        &self,
        run: &SalesSelectionPrepareTask,
        reason: String,
        tx: &mut dyn Executor,
    ) -> Result<()> {
        let Some(mut task) = self
            .db
            .sales_selection_prepare_tasks()
            .find_by_id(&run.base.id, tx)
            .await?
        else {
            return Ok(());
        };
        if !task.status.is_active() || task.run_version != run.run_version {
            return Ok(());
        }
        let mut book = self.load_booklet(task.booklet_id.as_ref(), tx).await?;
        if ensure_task_book(&task, &book).is_ok() {
            let actor = task.actor_id.as_str();
            book.fail_prepare(&reason, if actor.is_empty() { "system" } else { actor })?;
            self.db.sales_selection_booklets().update(&mut book, tx).await?;
        }
        task.mark_failed(reason, Vec::new(), Instant::now());
        self.db
            .sales_selection_prepare_tasks()
            .update(&mut task, tx)
            .await?;
        Ok(())
    }
}

/// 创建前校验任务类型、档位范围与新规则；失败不占用册。
fn validate_prepare_request(book: &SalesSelectionBooklet, req: &PrepareSalesSelectionRequest) -> Result<()> {
    if req.kind == PrepareKind::RegeneratedTiers
        && (req.tier_ids.is_empty()
            || req
                .tier_ids
                .iter()
                .any(|id| !book.tiers.iter().any(|tier| &tier.tier_id == id)))
    {
        return Err(Error::ValidationError("请选择本册需要重生成的档位".into()));
    }
    if req.kind.reuses_current_batch()
        && (req.pool_filter.is_some() || req.sku_ids.is_some() || !req.tiers.is_empty())
    {
        return Err(Error::ValidationError(
            "重生成不能修改商品池或档位规则，请整册重新准备".into(),
        ));
    }
    if matches!(req.kind, PrepareKind::FirstPrepare | PrepareKind::RePrepare) {
        super::prepare::apply_reprepare_request(&mut book.clone(), req)?;
    }
    Ok(())
}

/// 任务身份和启动后的册版本共同构成写入权限。
fn ensure_task_book(task: &SalesSelectionPrepareTask, book: &SalesSelectionBooklet) -> Result<()> {
    if book.active_task_id.as_deref() != Some(task.base.id.as_str())
        || book.base.version != task.booklet_version
    {
        return Err(Error::selection_prepare_failed("准备任务已被其他操作替代"));
    }
    Ok(())
}

/// 构造完整可恢复命令；册版本必须是进入准备中之后的版本。
fn queued_task(
    id: SalesSelectionPrepareTaskId,
    book: &SalesSelectionBooklet,
    req: &PrepareSalesSelectionRequest,
    actor: &str,
    json: String,
    key: String,
    hash: String,
) -> SalesSelectionPrepareTask {
    let mut task = SalesSelectionPrepareTask::queued(
        id,
        SalesSelectionPrepareTaskData {
            booklet_id: SalesSelectionBookletId::new(&req.booklet_id),
            booklet_version: book.base.version,
            kind: req.kind,
            tier_ids: req.tier_ids.clone(),
            idempotency_key: key,
            request_hash: hash,
            seed: req.seed,
            now: Instant::now(),
        },
    );
    task.request_json = json;
    task.actor_id = actor.to_string();
    task
}

impl Prepared {
    /// 重生成保留原冻结时点，重新准备采用本次时点；成功才提交新规则。
    fn finish(&mut self, task: &mut SalesSelectionPrepareTask) -> Result<()> {
        let date = self
            .book
            .eligibility_as_of
            .filter(|_| task.kind.reuses_current_batch())
            .unwrap_or_else(BusinessDate::today);
        let frozen_at = self
            .book
            .prepared_at
            .filter(|_| task.kind.reuses_current_batch())
            .unwrap_or_else(Instant::now);
        self.book
            .complete_prepare(&self.batch, date, frozen_at, &task.actor_id)?;
        task.mark_succeeded(
            self.batch.clone(),
            std::mem::take(&mut self.reports),
            Instant::now(),
        );
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::sales_selection::{
        PoolFilterSnapshot, PoolSource, PoolSourceKind, SalesSelectionBookletData, SelectionForm, SubmitMode,
    };
    use erp_core::ids::CustomerAccountId;

    /// 构造无外部依赖的有效单品册。
    fn book() -> SalesSelectionBooklet {
        SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("book"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("customer"),
                customer_no: "C1".into(),
                customer_name: "客户".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::ByQuantity,
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot::default()),
                    None,
                )
                .unwrap(),
                tiers: vec![],
                created_by: "sales".into(),
            },
        )
        .unwrap()
    }

    /// 任务命令的初始版本来自用户实际读取的册。
    fn request() -> PrepareSalesSelectionRequest {
        serde_json::from_value(serde_json::json!({"booklet_id":"book","idempotency_key":"key","expected_version":1,"kind":"FIRST_PREPARE"})).unwrap()
    }

    #[test]
    fn queued_command_keeps_original_rules_and_fences_changed_book() {
        let mut book = book();
        let req = request();
        let source = book.pool_source.clone();
        validate_prepare_request(&book, &req).unwrap();
        book.begin_prepare("task", req.kind, "sales").unwrap();
        let task = queued_task(
            SalesSelectionPrepareTaskId::new("task"),
            &book,
            &req,
            "sales",
            serde_json::to_string(&req).unwrap(),
            "key".into(),
            "hash".into(),
        );
        assert_eq!(book.pool_source, source);
        assert!(ensure_task_book(&task, &book).is_ok());
        book.base.version += 1;
        assert!(ensure_task_book(&task, &book).is_err());
        book.base.version -= 1;
        book.active_task_id = Some("new-task".into());
        assert!(ensure_task_book(&task, &book).is_err());
    }

    #[test]
    fn regeneration_rejects_empty_target_and_source_changes() {
        let book = book();
        let mut req = request();
        req.kind = PrepareKind::RegeneratedTiers;
        assert!(validate_prepare_request(&book, &req).is_err());
        req.tier_ids = vec!["foreign-tier".into()];
        assert!(validate_prepare_request(&book, &req).is_err());
        req.kind = PrepareKind::RegeneratedAll;
        req.pool_filter = Some(PoolFilterSnapshot::default());
        assert!(validate_prepare_request(&book, &req).is_err());
    }
}
