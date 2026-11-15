//! 将批量保存和核对登记到统一后台执行器，HTTP 只返回受理摘要。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_supply::catalog_adoption::{
    BatchAdoptionExt, BatchWork, BatchWorkPhase, BatchWorkRequest, BatchWorkStatus, CatalogAdoptionBatch,
    adoption_identity, has_inflight_batch_items, work_requests,
};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use serde::{Deserialize, Serialize};

use super::batch::batch_summary;
use super::{BatchVersionRequest, BatchView, SaveBatchRequest, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind", content = "request")]
pub(super) enum WorkPayload {
    Save(Box<SaveBatchRequest>),
    Review(BatchVersionRequest),
}

impl WorkPayload {
    fn version(&self) -> u64 {
        match self {
            Self::Save(request) => request.expected_version,
            Self::Review(request) => request.expected_version,
        }
    }
    fn phase(&self) -> BatchWorkPhase {
        match self {
            Self::Save(request)
                if request.review_after_save
                    && request.rules.is_none()
                    && request.defaults.is_none()
                    && request.fill_policy.is_none()
                    && request.inactive_pricing.is_none()
                    && request.excluded_ids.is_none()
                    && request.items.is_empty() =>
            {
                BatchWorkPhase::Reviewing
            },
            Self::Save(_) => BatchWorkPhase::Saving,
            Self::Review(_) => BatchWorkPhase::Reviewing,
        }
    }
}

impl SupplierCatalogAdoptionProcess {
    /// 登记原版本对应的唯一处理任务，不加载或重写全部明细。
    pub(super) async fn enqueue_batch_work(
        &self,
        connection: &str,
        id: &str,
        payload: WorkPayload,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let (process, connection, id, actor) =
            (self.clone(), connection.to_owned(), id.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch = process.load_batch(&connection, &id, &actor, executor).await?;
                    let key = adoption_identity(&["draft", &id, actor.id(), &payload.version().to_string()]);
                    let encoded =
                        serde_json::to_value(&payload).map_err(|error| Error::Internal(error.to_string()))?;
                    let fingerprint = adoption_identity(&[&encoded.to_string()]);
                    if let Some(existing) = work_requests(&process.db).find_by_id(&key, executor).await? {
                        if existing.fingerprint != fingerprint {
                            return Err(Error::ConflictError("原版本已用于其他修改，请重新读取批次".into()));
                        }
                        return Ok(batch_summary(batch));
                    }
                    batch.ensure_version(payload.version())?;
                    batch.ensure_draft_available(Instant::now().unix_secs())?;
                    if has_inflight_batch_items(&process.db, &id, executor).await? {
                        return Err(Error::ConflictError("入库正在执行，请先查看结果".into()));
                    }
                    process.register_work(&mut batch, &payload, key.clone(), &actor)?;
                    let request = BatchWorkRequest {
                        base: BaseModel::new(key),
                        batch_id: id,
                        fingerprint,
                        payload: encoded,
                    };
                    work_requests(&process.db).create(&request, executor).await?;
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(batch_summary(batch))
                })
            })
            .await
    }

    /// 只登记状态及原预览代次；规则在后台完成保存后才发布。
    fn register_work(
        &self,
        batch: &mut CatalogAdoptionBatch,
        payload: &WorkPayload,
        key: String,
        actor: &AuditActor,
    ) -> Result<()> {
        let review_after_save = match payload {
            WorkPayload::Save(request) => {
                self.validate_draft_request(request)?;
                request.review_after_save
            },
            WorkPayload::Review(_) => false,
        };
        batch.work = Some(BatchWork {
            id: key.clone(),
            claimed: false,
            actor_id: actor.id().into(),
            phase: payload.phase(),
            status: BatchWorkStatus::Running,
            completed: 0,
            total: batch.selected_count,
            after: String::new(),
            generation: batch
                .base
                .version
                .checked_add(1)
                .ok_or_else(|| Error::ValidationError("批次版本超限".into()))?,
            previous_preview: batch.preview_version,
            review_after_save,
            error: None,
        });
        batch.draft_operation = Some(key);
        batch.draft_started_at = Some(Instant::now().unix_secs());
        batch.invalidate_preview();
        Ok(())
    }

    /// 领取持久化任务；过期处理者的标记被替换后不能再写入。
    async fn claim_batch_work(&self, id: &str) -> Result<Option<CatalogAdoptionBatch>> {
        let (process, id) = (self.clone(), id.to_owned());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let Some(mut batch) =
                        process.db.catalog_adoption_batches().find_by_id(&id, executor).await?
                    else {
                        return Ok(None);
                    };
                    let Some(work) = batch.work.as_mut() else { return Ok(None) };
                    let now = Instant::now().unix_secs();
                    if work.status != BatchWorkStatus::Running
                        || (work.claimed
                            && batch
                                .draft_started_at
                                .is_some_and(|started| now.saturating_sub(started) < 300))
                    {
                        return Ok(None);
                    }
                    work.claimed = true;
                    batch.draft_operation = Some(next_id());
                    batch.draft_started_at = Some(now);
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(Some(batch))
                })
            })
            .await
    }

    /// 执行保存或核对，进度由每个短事务持久化。
    /// # 参数
    /// `id` 为批次身份。
    /// # 返回
    /// 本轮处理结束或任务已由其他处理者领取。
    /// # 错误
    /// 失败记录到原任务，不发布部分核对结果。
    pub async fn process_batch_work(&self, id: &str) -> Result<()> {
        let Some(batch) = self.claim_batch_work(id).await? else { return Ok(()) };
        let token = batch.draft_operation.as_deref().ok_or_else(|| Error::Internal("处理标记缺失".into()))?;
        let result = self.execute_batch_work(&batch, token).await;
        if let Err(error) = &result {
            self.fail_batch_work(&batch, token, error).await?;
        }
        result
    }

    /// 恢复原请求及真实操作人，保存后按同一任务继续核对。
    async fn execute_batch_work(&self, batch: &CatalogAdoptionBatch, token: &str) -> Result<()> {
        let (phase, payload, actor) = self.batch_work_request(batch, token).await?;
        if phase == BatchWorkPhase::Saving {
            let WorkPayload::Save(save) = &payload else {
                return Err(Error::Internal("保存任务类型不一致".into()));
            };
            self.save_batch_work(batch, token, save, &actor).await?;
        }
        let (connection, id, acting) = (batch.connection_id.clone(), batch.base.id.clone(), actor.clone());
        let current = self
            .with_snapshot(move |process, executor| {
                Box::pin(async move { process.load_batch(&connection, &id, &acting, executor).await })
            })
            .await?;
        if current.work.as_ref().is_some_and(|work| work.status == BatchWorkStatus::Running) {
            let ids = match &payload {
                WorkPayload::Review(request) => request.item_ids.as_deref(),
                WorkPayload::Save(_) => None,
            };
            self.preview_batch_members(&current, token, ids, &actor).await?;
        }
        Ok(())
    }

    /// 在当前授权快照内读取原处理请求并核对真实操作人及处理标记。
    async fn batch_work_request(
        &self,
        batch: &CatalogAdoptionBatch,
        token: &str,
    ) -> Result<(BatchWorkPhase, WorkPayload, AuditActor)> {
        let work = batch.work.as_ref().ok_or_else(|| Error::Internal("处理任务缺失".into()))?;
        let (connection, id, actor_id, token) =
            (batch.connection_id.clone(), batch.base.id.clone(), work.actor_id.clone(), token.to_owned());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move {
                let actor = process.batch_actor(&actor_id, executor).await?;
                let current = process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                let work = current.work.as_ref().ok_or_else(|| Error::Internal("处理任务缺失".into()))?;
                let request = work_requests(&process.db)
                    .find_by_id(&work.id, executor)
                    .await?
                    .ok_or_else(|| Error::Internal("原处理请求缺失".into()))?;
                let payload = serde_json::from_value(request.payload)
                    .map_err(|error| Error::Internal(error.to_string()))?;
                Ok((work.phase, payload, actor))
            })
        })
        .await
    }

    /// 在只读匹配阶段续期当前处理标记，防止长批次被错误接管。
    pub(super) async fn renew_batch_work(
        &self,
        original: &CatalogAdoptionBatch,
        token: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let (connection, id, token, actor) =
            (original.connection_id.clone(), original.base.id.clone(), token.to_owned(), actor.clone());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move {
                let mut current =
                    process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                current.draft_started_at = Some(Instant::now().unix_secs());
                process.db.catalog_adoption_batches().update(&mut current, executor).await?;
                Ok(())
            })
        })
        .await
    }

    /// 失败只覆盖仍由原处理标记持有的任务，保留原游标。
    async fn fail_batch_work(
        &self,
        original: &CatalogAdoptionBatch,
        token: &str,
        error: &Error,
    ) -> Result<()> {
        let Some(mut batch) =
            self.db.catalog_adoption_batches().find_by_id(&original.base.id, &mut NoTransaction).await?
        else {
            return Ok(());
        };
        if batch.draft_operation.as_deref() != Some(token) {
            return Ok(());
        }
        if let Some(work) = &mut batch.work {
            work.status = BatchWorkStatus::Failed;
            work.error = Some(match error {
                Error::ValidationError(message)
                | Error::ConflictError(message)
                | Error::Forbidden(message)
                | Error::BusinessLogicError(message) => message.clone(),
                _ => "处理暂时中断，已完成部分已保留，请重试；持续失败请联系管理员".into(),
            });
        }
        self.db.catalog_adoption_batches().update(&mut batch, &mut NoTransaction).await?;
        Ok(())
    }

    /// 重试原任务或结束尚未写入的失败保存，拒绝丢弃半批保存。
    /// # 参数
    /// 连接、批次、原任务身份、是否重试与认证用户。
    /// # 返回
    /// 操作后的批次摘要。
    /// # 错误
    /// 越权、任务已变化或不安全的结束请求时拒绝。
    pub async fn control_batch_work(
        &self,
        connection: &str,
        id: &str,
        work_id: &str,
        retry: bool,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let (process, connection, id, work_id, actor) =
            (self.clone(), connection.to_owned(), id.to_owned(), work_id.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch = process.load_batch(&connection, &id, &actor, executor).await?;
                    let work = batch
                        .work
                        .as_mut()
                        .filter(|work| work.id == work_id)
                        .ok_or_else(|| Error::ConflictError("处理任务已变化，请重新读取".into()))?;
                    if work.status != BatchWorkStatus::Failed {
                        return Err(Error::ConflictError("只能操作已中断的任务".into()));
                    }
                    if retry {
                        work.retry()?;
                        work.claimed = false;
                        batch.draft_operation = Some(work.id.clone());
                    } else {
                        if work.phase == BatchWorkPhase::Saving && work.completed > 0 {
                            return Err(Error::ConflictError(
                                "已有部分草稿写入，请先重试完成本次保存".into(),
                            ));
                        }
                        work.status = BatchWorkStatus::Completed;
                        batch.draft_operation = None;
                        batch.draft_started_at = None;
                    }
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(batch_summary(batch))
                })
            })
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_next_step_starts_review_without_rewriting_rows() {
        let mut request: SaveBatchRequest = serde_json::from_value(serde_json::json!({
            "expected_version":8,"review_after_save":true,"step":"REVIEW"
        }))
        .unwrap();
        assert_eq!(WorkPayload::Save(Box::new(request.clone())).phase(), BatchWorkPhase::Reviewing);
        request.defaults = Some(Default::default());
        assert_eq!(WorkPayload::Save(Box::new(request.clone())).phase(), BatchWorkPhase::Saving);
        request.defaults = None;
        request.review_after_save = false;
        assert_eq!(WorkPayload::Save(Box::new(request)).phase(), BatchWorkPhase::Saving);
    }
}
