//! 服务端保存规则、资料补值和单项人工例外。

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_supply::catalog_adoption::{
    BatchAdoptionExt, BatchItemStatus, BatchValuePatch, BatchWorkStatus, CatalogAdoptionBatch,
    CatalogAdoptionBatchItem, ValueOrigin, has_inflight_batch_items, update_draft_chunk,
};
use persistence_core::Transactional;
use sha2::{Digest, Sha256};

use super::batch::batch_summary;
use super::batch_work::WorkPayload;
use super::{BatchItemEdit, BatchView, SaveBatchRequest, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 保存草稿并作废受修改项的旧核对结果。
    ///
    /// 来源连接、批次、页面已读版本、局部修改及当前用户。
    ///
    /// 无明细修改时返回已保存步骤；批量修改返回已受理任务摘要。
    ///
    /// 越权、版本过期、修改已冻结项或字段错误时拒绝。
    pub async fn save_batch(
        &self,
        connection: &str,
        id: &str,
        request: SaveBatchRequest,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        self.validate_draft_request(&request)?;
        if request.rules.is_none()
            && request.fill_policy.is_none()
            && request.defaults.is_none()
            && request.excluded_ids.is_none()
            && request.items.is_empty()
            && !request.review_after_save
        {
            return self.save_batch_step(connection, id, request, actor).await;
        }
        self.enqueue_batch_work(connection, id, WorkPayload::Save(Box::new(request)), actor).await
    }

    // 切换步骤或保存未生效定价输入只更新批次头，保留已有核对结果。
    async fn save_batch_step(
        &self,
        connection: &str,
        id: &str,
        request: SaveBatchRequest,
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
                    batch.ensure_version(request.expected_version)?;
                    batch.ensure_draft_available(Instant::now().unix_secs())?;
                    if has_inflight_batch_items(&process.db, &id, executor).await? {
                        return Err(Error::ConflictError("入库正在执行，请先查看结果".into()));
                    }
                    process.update_batch_settings(&mut batch, &request, &[])?;
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(batch_summary(batch))
                })
            })
            .await
    }

    // 在受理前校验请求结构，避免明显无效的输入占用后台任务。
    pub(super) fn validate_draft_request(&self, request: &SaveBatchRequest) -> Result<()> {
        if let Some(defaults) = &request.defaults {
            defaults.validate()?;
        }
        if request
            .step
            .as_deref()
            .is_some_and(|step| !["PRICING", "DETAILS", "REVIEW", "RESULT", "RECOVERY"].contains(&step))
        {
            return Err(Error::ValidationError("入库步骤无效".into()));
        }
        let mut seen = HashSet::new();
        if request.items.iter().any(|edit| edit.item_id.trim().is_empty() || !seen.insert(&edit.item_id)) {
            return Err(Error::ValidationError("单项调整身份不能为空或重复".into()));
        }
        Ok(())
    }

    // 后台验证完整修改后，仅分块写入实际发生变化的草稿。
    pub(super) async fn save_batch_work(
        &self,
        batch: &CatalogAdoptionBatch,
        token: &str,
        request: &SaveBatchRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        let (batch, mut rows) = self.batch_work_rows(batch, token, actor).await?;
        let work = batch.work.as_ref().ok_or_else(|| Error::Internal("草稿任务缺失".into()))?;
        let before = rows.iter().map(draft_digest).collect::<Result<Vec<_>>>()?;
        let mut desired = batch.clone();
        desired.preview_version = work.previous_preview;
        self.prepare_batch_changes(&mut desired, &mut rows, request)?;
        let pending =
            rows.into_iter().zip(before).filter(|(row, _)| row.base.id > work.after).collect::<Vec<_>>();

        for chunk in pending.chunks(200) {
            let changed = chunk
                .iter()
                .filter_map(|(row, digest)| match draft_digest(row) {
                    Ok(current) if current == *digest => None,
                    Ok(_) => Some(Ok(row.clone())),
                    Err(error) => Some(Err(error)),
                })
                .collect::<Result<Vec<_>>>()?;
            self.persist_draft_chunk(
                &batch,
                token,
                &changed,
                chunk.len(),
                &chunk.last().expect("非空分块").0.base.id,
                actor,
            )
            .await?;
        }

        self.finish_saved_work(&batch, &desired, token, actor).await
    }

    // 在同一快照内核对处理标记、当前权限和完整草稿资料。
    async fn batch_work_rows(
        &self,
        batch: &CatalogAdoptionBatch,
        token: &str,
        actor: &AuditActor,
    ) -> Result<(CatalogAdoptionBatch, Vec<CatalogAdoptionBatchItem>)> {
        let (connection, id, operation, acting) =
            (batch.connection_id.clone(), batch.base.id.clone(), token.to_owned(), actor.clone());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move {
                let batch =
                    process.check_batch_draft(&connection, &id, &operation, &acting, executor).await?;
                let rows = process.all_batch_items(&id, executor).await?;
                process.ensure_batch_rows_access(&acting, &rows, executor).await?;
                Ok((batch, rows))
            })
        })
        .await
    }

    // 同一短事务批量更新有变化的行并保存扫描进度。
    async fn persist_draft_chunk(
        &self,
        original: &CatalogAdoptionBatch,
        token: &str,
        rows: &[CatalogAdoptionBatchItem],
        count: usize,
        after: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, connection, id, token, rows, after, actor) = (
            self.clone(),
            original.connection_id.clone(),
            original.base.id.clone(),
            token.to_owned(),
            rows.to_vec(),
            after.to_owned(),
            actor.clone(),
        );
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch =
                        process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                    process.ensure_batch_rows_access(&actor, &rows, executor).await?;

                    batch
                        .work
                        .as_mut()
                        .ok_or_else(|| Error::Internal("草稿任务缺失".into()))?
                        .advance(count, after)?;
                    batch.draft_started_at = Some(Instant::now().unix_secs());
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    update_draft_chunk(&process.db, &rows, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    // 全部草稿保存完成后发布规则；需要核对时沿同一任务继续。
    async fn finish_saved_work(
        &self,
        original: &CatalogAdoptionBatch,
        desired: &CatalogAdoptionBatch,
        token: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, connection, id, token, desired, actor) = (
            self.clone(),
            original.connection_id.clone(),
            original.base.id.clone(),
            token.to_owned(),
            desired.clone(),
            actor.clone(),
        );
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch =
                        process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                    let work = batch.work.as_mut().ok_or_else(|| Error::Internal("草稿任务缺失".into()))?;
                    if work.completed != work.total {
                        return Err(Error::ConflictError("草稿保存尚未完成".into()));
                    }

                    batch.rules = desired.rules;
                    batch.inactive_pricing = desired.inactive_pricing;
                    batch.fill_policy = desired.fill_policy;
                    batch.defaults = desired.defaults;
                    batch.step = desired.step;
                    batch.excluded_ids = desired.excluded_ids;

                    if work.review_after_save {
                        work.start_review()?;
                    } else {
                        work.status = BatchWorkStatus::Completed;
                        batch.preview_version = desired.preview_version;
                        batch.draft_operation = None;
                        batch.draft_started_at = None;
                    }
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    fn prepare_batch_changes(
        &self,
        batch: &mut CatalogAdoptionBatch,
        rows: &mut [CatalogAdoptionBatchItem],
        request: &SaveBatchRequest,
    ) -> Result<()> {
        self.update_batch_settings(batch, request, rows)?;
        self.update_exclusions(batch, rows, request.excluded_ids.as_deref())?;
        let changed_rules =
            request.rules.is_some() || request.defaults.is_some() || request.fill_policy.is_some();
        if changed_rules
            || request.excluded_ids.is_some()
            || request.items.iter().any(BatchItemEdit::changes_content)
        {
            batch.invalidate_preview();
        }

        for row in rows.iter_mut() {
            if changed_rules && row.editable() && row.status != BatchItemStatus::Excluded {
                row.invalidate_preview()?;
                row.apply_defaults(&batch.defaults, batch.fill_policy)?;
                let existing = row.resolution.as_ref().is_some_and(|value| value.kind == "EXISTING");
                match row.sales_prices.apply(
                    &batch.rules,
                    batch.fill_policy,
                    row.source.purchase_quote().ok(),
                    existing,
                ) {
                    Ok(values) => row.sales_prices = values,
                    Err(error) => row.messages = vec![error.to_string()],
                }
            }
        }

        let row_indices: HashMap<_, _> =
            rows.iter().enumerate().map(|(index, row)| (row.source.base.id.clone(), index)).collect();
        let mut edited = HashSet::new();
        for edit in &request.items {
            if !edited.insert(&edit.item_id) {
                return Err(Error::ValidationError("同一来源规格不能重复调整".into()));
            }
            let index = row_indices
                .get(&edit.item_id)
                .ok_or_else(|| Error::ValidationError("调整项不属于本批来源范围".into()))?;
            self.edit_batch_item(batch, &mut rows[*index], edit.clone())?;
        }
        Ok(())
    }

    fn update_batch_settings(
        &self,
        batch: &mut CatalogAdoptionBatch,
        request: &SaveBatchRequest,
        rows: &[CatalogAdoptionBatchItem],
    ) -> Result<()> {
        if rows.iter().any(|row| {
            row.status == BatchItemStatus::Processing
                || (row.status == BatchItemStatus::Ready && row.operation_id.is_some())
        }) {
            return Err(Error::ConflictError("本批正在执行，请完成后处理剩余项".into()));
        }

        if let Some(rules) = &request.rules {
            batch.rules = rules.clone();
        }
        if let Some(rules) = &request.inactive_pricing {
            batch.inactive_pricing = Some(rules.clone());
        }
        if let Some(policy) = request.fill_policy {
            batch.fill_policy = policy;
        }
        if let Some(defaults) = &request.defaults {
            defaults.validate()?;
            batch.defaults = defaults.clone();
        }
        if let Some(step) = &request.step {
            if !["PRICING", "DETAILS", "REVIEW", "RESULT", "RECOVERY"].contains(&step.as_str()) {
                return Err(Error::ValidationError("入库步骤无效".into()));
            }
            batch.step = step.clone();
        }
        Ok(())
    }

    fn update_exclusions(
        &self,
        batch: &mut CatalogAdoptionBatch,
        rows: &mut [CatalogAdoptionBatchItem],
        excluded: Option<&[String]>,
    ) -> Result<()> {
        let Some(excluded) = excluded else { return Ok(()) };
        if batch.submitted {
            return Err(Error::ConflictError("已提交批次不能修改选择范围".into()));
        }
        let ids: HashSet<_> = rows.iter().map(|row| row.source.base.id.as_str()).collect();
        let unique: HashSet<_> = excluded.iter().collect();
        if unique.len() != excluded.len() || excluded.iter().any(|id| !ids.contains(id.as_str())) {
            return Err(Error::ValidationError("排除项必须属于本批且不能重复".into()));
        }

        batch.excluded_ids = excluded.to_vec();
        for row in rows {
            if unique.contains(&row.source.base.id) {
                row.status = BatchItemStatus::Excluded;
            } else if row.status == BatchItemStatus::Excluded {
                row.status =
                    if row.initially_adopted { BatchItemStatus::Skipped } else { BatchItemStatus::Draft };
            }
            row.preview_version = None;
        }
        Ok(())
    }

    fn edit_batch_item(
        &self,
        batch: &CatalogAdoptionBatch,
        row: &mut CatalogAdoptionBatchItem,
        edit: BatchItemEdit,
    ) -> Result<()> {
        if !row.editable() {
            return Err(Error::ConflictError("该项已完成或结果待核实，不能修改原操作".into()));
        }
        let existing = row.resolution.as_ref().is_some_and(|value| value.kind == "EXISTING");
        let content_changed = edit.changes_content();
        if content_changed {
            row.invalidate_preview()?;
        }

        if let Some(mapping) = edit.mapping {
            row.edit_mapping(mapping)?;
        }
        if let Some(terms) = edit.terms {
            row.edit_terms(terms)?;
        }

        if let Some(prices) = edit.sales_edit {
            row.sales_prices = row.sales_prices.edit(&prices, existing)?;
        }
        if edit.restore_pricing {
            row.sales_prices = row.sales_prices.restore(
                &batch.rules,
                row.source.purchase_quote().ok(),
                row.resolution.as_ref().is_some_and(|value| value.kind == "EXISTING"),
            )?;
        }

        match edit.delivery_template_id {
            BatchValuePatch::Keep => {},
            BatchValuePatch::Set(value) => {
                row.delivery_template_id = Some(value);
                row.field_origins.insert("delivery_template_id".into(), ValueOrigin::Manual);
            },
            BatchValuePatch::Clear => {
                row.delivery_template_id = None;
                row.field_origins.insert("delivery_template_id".into(), ValueOrigin::Cleared);
            },
        }
        if let Some(confirmed) = edit.warnings_confirmed {
            row.warnings_confirmed = confirmed;
        }
        Ok(())
    }
}

// 只比较业务草稿是否实际变化，避免每次步骤切换重写整批。
fn draft_digest(row: &CatalogAdoptionBatchItem) -> Result<Vec<u8>> {
    let encoded = serde_json::to_vec(row).map_err(|error| Error::Internal(error.to_string()))?;
    Ok(Sha256::digest(encoded).to_vec())
}
