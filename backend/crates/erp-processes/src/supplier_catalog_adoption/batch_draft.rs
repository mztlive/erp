//! 服务端保存规则、资料补值和单项人工例外。

use std::collections::HashSet;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_supply::catalog_adoption::{
    BatchAdoptionExt, BatchItemStatus, BatchValuePatch, CatalogAdoptionBatch, CatalogAdoptionBatchItem,
    ValueOrigin,
};
use persistence_core::{NoTransaction, Transactional};

use super::{BatchItemEdit, BatchView, SaveBatchRequest, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 保存草稿并作废受修改项的旧核对结果。
    /// # 参数
    /// 来源连接、批次、页面已读版本、局部修改及当前用户。
    /// # 返回
    /// 已持久化的最新草稿；失败时不发布预览，已完成的逐项保存可重新读取。
    /// # 错误
    /// 越权、版本过期、修改已冻结项或字段错误时拒绝。
    pub async fn save_batch(
        &self,
        connection: &str,
        id: &str,
        request: SaveBatchRequest,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let mut desired = self.load_batch(connection, id, actor, &mut NoTransaction).await?;
        desired.ensure_version(request.expected_version)?;
        let mut rows = self.all_batch_items(id, &mut NoTransaction).await?;
        self.ensure_batch_rows_access(actor, &rows, &mut NoTransaction).await?;
        self.prepare_batch_changes(&mut desired, &mut rows, &request)?;
        let (reserved, token) =
            self.begin_batch_draft(connection, id, request.expected_version, actor).await?;
        let outcome = self.persist_batch_changes(&reserved, &desired, rows, &token, actor).await;
        if let Err(error) = self.release_batch_draft(connection, id, &token, actor).await {
            tracing::error!(batch_id = id, account = %actor.id(), error = %error, "supplier adoption draft release failed");
            if outcome.is_ok() {
                return Err(error);
            }
        }
        outcome?;
        self.batch(connection, id, actor).await
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
        let mut edited = HashSet::new();
        for edit in &request.items {
            if !edited.insert(&edit.item_id) {
                return Err(Error::ValidationError("同一来源规格不能重复调整".into()));
            }
            let row = rows
                .iter_mut()
                .find(|row| row.source.base.id == edit.item_id)
                .ok_or_else(|| Error::ValidationError("调整项不属于本批来源范围".into()))?;
            self.edit_batch_item(batch, row, edit.clone())?;
        }
        Ok(())
    }

    async fn persist_batch_changes(
        &self,
        reserved: &CatalogAdoptionBatch,
        desired: &CatalogAdoptionBatch,
        rows: Vec<CatalogAdoptionBatchItem>,
        token: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, connection, id, token, actor, desired) = (
            self.clone(),
            reserved.connection_id.clone(),
            reserved.base.id.clone(),
            token.to_owned(),
            actor.clone(),
            desired.clone(),
        );
        let preview = desired.preview_version;
        let (settings_process, settings_connection, settings_id, settings_token, settings_actor) =
            (process.clone(), connection.clone(), id.clone(), token.clone(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch = settings_process
                        .check_batch_draft(
                            &settings_connection,
                            &settings_id,
                            &settings_token,
                            &settings_actor,
                            executor,
                        )
                        .await?;
                    batch.rules = desired.rules;
                    batch.inactive_pricing = desired.inactive_pricing;
                    batch.fill_policy = desired.fill_policy;
                    batch.defaults = desired.defaults;
                    batch.step = desired.step;
                    batch.excluded_ids = desired.excluded_ids;
                    batch.draft_started_at = Some(Instant::now().unix_secs());
                    settings_process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await?;
        for row in rows {
            // 未知结果可由后台查证并更新；草稿保存不得重写这些不可编辑行。
            if matches!(
                row.status,
                BatchItemStatus::Unknown | BatchItemStatus::Succeeded | BatchItemStatus::Processing
            ) {
                continue;
            }
            self.persist_batch_draft_item(&connection, &id, &token, row, &actor).await?;
        }
        self.finish_saved_draft(&connection, &id, &token, preview, &actor).await?;
        Ok(())
    }

    async fn finish_saved_draft(
        &self,
        connection: &str,
        id: &str,
        token: &str,
        preview: Option<u64>,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, connection, id, token, actor) =
            (self.clone(), connection.to_owned(), id.to_owned(), token.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch =
                        process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                    batch.preview_version = preview;
                    batch.draft_operation = None;
                    batch.draft_started_at = None;
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    async fn persist_batch_draft_item(
        &self,
        connection: &str,
        id: &str,
        token: &str,
        mut row: CatalogAdoptionBatchItem,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, connection, id, token, actor) =
            (self.clone(), connection.to_owned(), id.to_owned(), token.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch =
                        process.check_batch_draft(&connection, &id, &token, &actor, executor).await?;
                    batch.draft_started_at = Some(Instant::now().unix_secs());
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    process.db.catalog_adoption_batch_items().update(&mut row, executor).await?;
                    Ok(())
                })
            })
            .await
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
            if excluded.contains(&row.source.base.id) {
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
