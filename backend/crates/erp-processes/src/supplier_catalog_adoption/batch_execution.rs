//! 冻结确认、后台逐项执行及原操作结果查证。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_identity::AccessControlExt;
use erp_supply::catalog_adoption::{
    AdoptionResult, BatchAdoptionExt, BatchConfirmation, BatchItemStatus, CatalogAdoptionBatch,
    CatalogAdoptionBatchItem, CatalogAdoptionCommand, CatalogAdoptionExt, adoption_identity,
    batch_item_by_source, due_batch_items, uncertain_batch_items,
};
use persistence_core::{Error as PersistenceError, Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};

use super::{BatchView, SubmitBatchRequest, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 冻结用户明确确认的就绪子集，并持久化后台任务；HTTP不执行入库循环。
    /// # 参数
    /// 来源连接、批次、当前预览版本、确认集合、原操作号和当前用户。
    /// # 返回
    /// 已排入后台的批次；未就绪项保留原状态。
    /// # 错误
    /// 越权、旧版本、来源或目标变化、异载荷重用操作号时拒绝。
    pub async fn submit_batch(
        &self,
        connection: &str,
        id: &str,
        request: SubmitBatchRequest,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let fingerprint = self.preflight_batch_submission(connection, id, &request, actor).await?;
        let (process, source_id, batch_id, acting) =
            (self.clone(), connection.to_owned(), id.to_owned(), actor.clone());
        let command = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process
                        .freeze_batch_header(&source_id, &batch_id, request, fingerprint, &acting, executor)
                        .await
                })
            })
            .await?;
        let batch = self.load_batch(connection, id, actor, &mut NoTransaction).await?;
        if !batch.published_operations.contains(&command)
            && let Err(error) = self.prepare_batch_operation(&batch, &command).await
        {
            if preparation_failed(&error) {
                self.abort_unpublished_operation(&batch, &command, &error.to_string()).await?;
            }
            return Err(error);
        }
        self.batch(connection, id, actor).await
    }

    async fn preflight_batch_submission(
        &self,
        connection: &str,
        id: &str,
        request: &SubmitBatchRequest,
        actor: &AuditActor,
    ) -> Result<String> {
        let batch = self.load_batch(connection, id, actor, &mut NoTransaction).await?;
        let rows = self.all_batch_items(id, &mut NoTransaction).await?;
        self.ensure_batch_rows_access(actor, &rows, &mut NoTransaction).await?;
        let command = adoption_identity(&[connection, actor.id(), &request.idempotency_key]);
        let fingerprint = submission_fingerprint(id, request, &rows)?;
        if self.db.catalog_adoption_commands().find_by_id(&command, &mut NoTransaction).await?.is_none() {
            batch.ensure_version(request.expected_version)?;
            batch.ensure_submission(request.preview_version, &request.item_ids)?;
            for item in &request.item_ids {
                let mut row = rows
                    .iter()
                    .find(|row| row.source.base.id == *item)
                    .cloned()
                    .ok_or_else(|| Error::ValidationError("确认项不属于本批冻结范围".into()))?;
                row.freeze(command.clone(), request.preview_version)?;
            }
        }
        Ok(fingerprint)
    }

    async fn freeze_batch_header(
        &self,
        connection: &str,
        id: &str,
        request: SubmitBatchRequest,
        fingerprint: String,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let mut batch = self.load_batch(connection, id, actor, executor).await?;
        let source = self.batch_access(connection, actor, executor).await?;
        self.ensure_source(&source)?;
        let command = adoption_identity(&[connection, actor.id(), &request.idempotency_key]);
        if let Some(existing) = self.db.catalog_adoption_commands().find_by_id(&command, executor).await? {
            if existing.base.is_deleted() || existing.fingerprint != fingerprint {
                return Err(Error::ConflictError("同一操作号已用于不同入库确认".into()));
            }
            return Ok(command);
        }
        batch.ensure_version(request.expected_version)?;
        batch.ensure_submission(request.preview_version, &request.item_ids)?;
        let record = CatalogAdoptionCommand {
            base: BaseModel::new(command.clone()),
            connection_id: connection.into(),
            actor_id: actor.id().into(),
            fingerprint: fingerprint.clone(),
        };
        self.db.catalog_adoption_commands().create(&record, executor).await?;
        batch.confirmations.push(BatchConfirmation {
            operation_id: command.clone(),
            actor_id: actor.id().into(),
            confirmed_at: Instant::now(),
            preview_version: request.preview_version,
            item_ids: request.item_ids,
            fingerprint,
        });
        batch.submitted = true;
        batch.execution_pending = true;
        batch.step = "RESULT".into();
        batch.preview_version = None;
        batch.draft_operation = Some(format!("submit:{command}"));
        batch.draft_started_at = Some(Instant::now().unix_secs());
        self.db.catalog_adoption_batches().update(&mut batch, executor).await?;
        Ok(command)
    }

    async fn prepare_batch_operation(&self, batch: &CatalogAdoptionBatch, command: &str) -> Result<()> {
        let confirmation = batch
            .confirmations
            .iter()
            .find(|value| value.operation_id == command)
            .cloned()
            .ok_or_else(|| Error::Internal("原入库确认记录缺失".into()))?;
        let actor = self.batch_actor(&confirmation.actor_id).await?;
        for item in &confirmation.item_ids {
            let (process, source, batch_id, item, command, actor, preview_version) = (
                self.clone(),
                batch.connection_id.clone(),
                batch.base.id.clone(),
                item.clone(),
                command.to_owned(),
                actor.clone(),
                confirmation.preview_version,
            );
            self.db
                .client()
                .clone()
                .with_transaction(move |executor| {
                    Box::pin(async move {
                        let mut batch = process.load_batch(&source, &batch_id, &actor, executor).await?;
                        if batch.published_operations.contains(&command) {
                            return Ok::<_, Error>(());
                        }
                        if batch.draft_operation.as_deref() != Some(format!("submit:{command}").as_str()) {
                            return Err(Error::ConflictError("原入库确认已中断，请重新核对".into()));
                        }
                        let mut row = batch_item_by_source(&process.db, &batch_id, &item, executor)
                            .await?
                            .ok_or_else(|| Error::Internal("原确认行缺失".into()))?;
                        if row.operation_id.as_deref() == Some(&command) {
                            return Ok(());
                        }
                        // 此处只冻结用户已经查看的载荷。来源、目标及供给资格由
                        // confirm_execution 在每行实际执行前及写事务内重验，避免
                        // 单行来源变化阻断同次确认中其他有效行。
                        row.freeze(command.clone(), preview_version)?;
                        row.submitted_by = Some(actor.id().into());
                        process.db.catalog_adoption_batch_items().update(&mut row, executor).await?;
                        batch.draft_started_at = Some(Instant::now().unix_secs());
                        process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                        Ok(())
                    })
                })
                .await?;
        }
        self.publish_batch_operation(batch, command, &actor).await
    }

    async fn publish_batch_operation(
        &self,
        original: &CatalogAdoptionBatch,
        command: &str,
        actor: &AuditActor,
    ) -> Result<()> {
        let (process, source, id, command, actor) = (
            self.clone(),
            original.connection_id.clone(),
            original.base.id.clone(),
            command.to_owned(),
            actor.clone(),
        );
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut batch = process.load_batch(&source, &id, &actor, executor).await?;
                    if !batch.published_operations.contains(&command) {
                        batch.published_operations.push(command);
                    }
                    batch.draft_operation = None;
                    batch.draft_started_at = None;
                    process.db.catalog_adoption_batches().update(&mut batch, executor).await?;
                    Ok(())
                })
            })
            .await
    }

    async fn abort_unpublished_operation(
        &self,
        original: &CatalogAdoptionBatch,
        command: &str,
        message: &str,
    ) -> Result<()> {
        let mut batch = self
            .db
            .catalog_adoption_batches()
            .find_by_id(&original.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("入库批次不存在".into()))?;
        if batch.published_operations.iter().any(|value| value == command) {
            return Ok(());
        }
        let confirmation = batch
            .confirmations
            .iter()
            .find(|value| value.operation_id == command)
            .ok_or_else(|| Error::Internal("原确认记录缺失".into()))?;
        let mut rows = self.all_batch_items(&batch.base.id, &mut NoTransaction).await?;
        for row in &mut rows {
            if confirmation.item_ids.contains(&row.source.base.id)
                && matches!(row.status, BatchItemStatus::Ready | BatchItemStatus::Draft)
            {
                row.status = BatchItemStatus::Failed;
                row.operation_id = None;
                row.preview_version = None;
                row.messages = vec![message.into()];
                self.db.catalog_adoption_batch_items().update(row, &mut NoTransaction).await?;
            }
        }
        if !batch.published_operations.iter().any(|value| value == command) {
            batch.published_operations.push(command.into());
        }
        batch.draft_operation = None;
        batch.draft_started_at = None;
        batch.preview_version = None;
        self.db.catalog_adoption_batches().update(&mut batch, &mut NoTransaction).await?;
        Ok(())
    }

    /// 后台处理一批已确认行；每行领取和业务写入均独立提交。
    /// # 参数
    /// `id` 为已经持久化的批次身份。
    /// # 返回
    /// 本轮实际领取的行数。
    /// # 错误
    /// 存储故障返回错误；逐项明确失败或待核实持久化后继续其他项。
    pub async fn process_batch(&self, id: &str) -> Result<usize> {
        let mut batch = self
            .db
            .catalog_adoption_batches()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .filter(|batch| batch.selection_complete && batch.execution_pending)
            .ok_or_else(|| Error::NotFound("无待执行入库任务".into()))?;
        for confirmation in batch.confirmations.clone() {
            if !batch.published_operations.contains(&confirmation.operation_id)
                && let Err(error) = self.prepare_batch_operation(&batch, &confirmation.operation_id).await
            {
                if preparation_failed(&error) {
                    self.abort_unpublished_operation(&batch, &confirmation.operation_id, &error.to_string())
                        .await?;
                }
                return Err(error);
            }
        }
        batch = self
            .db
            .catalog_adoption_batches()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("入库批次不存在".into()))?;
        self.reconcile_batch_rows(&batch, None).await?;
        let claimed = self.execute_batch_queue(&batch).await?;
        let rows = self.all_batch_items(id, &mut NoTransaction).await?;
        if !rows.iter().any(|row| {
            row.status == BatchItemStatus::Processing
                || row.status == BatchItemStatus::Unknown
                || (row.status == BatchItemStatus::Ready && row.operation_id.is_some())
        }) {
            batch.execution_pending = false;
            self.db.catalog_adoption_batches().update(&mut batch, &mut NoTransaction).await?;
        }
        Ok(claimed)
    }

    async fn execute_batch_queue(&self, batch: &CatalogAdoptionBatch) -> Result<usize> {
        let pending = due_batch_items(&self.db, &batch.base.id, &mut NoTransaction).await?;
        let mut claimed = 0;
        for mut row in pending {
            if !row.operation_id.as_ref().is_some_and(|command| batch.published_operations.contains(command))
            {
                continue;
            }
            row.begin()?;
            match self.db.catalog_adoption_batch_items().update(&mut row, &mut NoTransaction).await {
                Ok(()) => {},
                Err(PersistenceError::OptimisticLockingError) => continue,
                Err(error) => return Err(error.into()),
            }
            claimed += 1;
            let result = self.execute_batch_row(batch, &row).await;
            record_execution_result(&mut row, result)?;
            self.db.catalog_adoption_batch_items().update(&mut row, &mut NoTransaction).await?;
        }
        Ok(claimed)
    }

    async fn execute_batch_row(
        &self,
        batch: &CatalogAdoptionBatch,
        row: &CatalogAdoptionBatchItem,
    ) -> Result<AdoptionResult> {
        let actor = self.batch_execution_actor(row).await?;
        self.batch_access(&batch.connection_id, &actor, &mut NoTransaction).await?;
        self.confirm_batch_row(batch, row, &actor).await
    }

    async fn batch_execution_actor(&self, row: &CatalogAdoptionBatchItem) -> Result<AuditActor> {
        let id = row
            .submitted_by
            .as_deref()
            .ok_or_else(|| Error::Forbidden("入库确认人缺失，请人工核查原操作".into()))?;
        self.batch_actor(id).await
    }

    async fn batch_actor(&self, id: &str) -> Result<AuditActor> {
        let account = self
            .db
            .accounts()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .filter(|account| account.is_active_backoffice())
            .ok_or_else(|| Error::Forbidden("入库确认账号已失效，请恢复授权后查询原操作".into()))?;
        Ok(AuditActor::new(account.base.id.clone(), account.secret.account().into(), account.kind)
            .with_actor_name_snapshot(Some(account.name))?)
    }

    /// 仅查询已冻结原操作的永久回执；查不到时维持待核实，禁止重新入库。
    /// # 参数
    /// 来源连接、批次和当前查询用户。
    /// # 返回
    /// 已查证结果与仍待核实项。
    /// # 错误
    /// 当前用户越权或读取持久化失败时拒绝。
    pub async fn reconcile_batch(&self, connection: &str, id: &str, actor: &AuditActor) -> Result<BatchView> {
        let batch = self.load_batch(connection, id, actor, &mut NoTransaction).await?;
        self.reconcile_batch_rows(&batch, Some(actor)).await?;
        self.batch(connection, id, actor).await
    }

    async fn reconcile_batch_rows(
        &self,
        batch: &CatalogAdoptionBatch,
        query_actor: Option<&AuditActor>,
    ) -> Result<()> {
        let mut after = String::new();
        loop {
            let rows = uncertain_batch_items(
                &self.db,
                &batch.base.id,
                &after,
                Instant::now().unix_secs() - 1800,
                &mut NoTransaction,
            )
            .await?;
            let Some(last) = rows.last() else { break };
            after = last.base.id.clone();
            for mut row in rows {
                if row.status == BatchItemStatus::Processing {
                    row.mark_unknown("执行中断，正在查询原入库结果".into())?;
                }
                match self.reconcile_batch_row(batch, &row, query_actor).await {
                    Ok(Some(result)) => row.complete(result)?,
                    Ok(None) => {
                        row.messages = vec!["尚未查得原操作结果，请稍后继续查询；当前不能重新提交".into()]
                    },
                    Err(error) => row.messages = vec![error.to_string()],
                }
                self.db.catalog_adoption_batch_items().update(&mut row, &mut NoTransaction).await?;
            }
        }
        Ok(())
    }

    async fn reconcile_batch_row(
        &self,
        batch: &CatalogAdoptionBatch,
        row: &CatalogAdoptionBatchItem,
        query_actor: Option<&AuditActor>,
    ) -> Result<Option<AdoptionResult>> {
        let actor = match query_actor {
            Some(actor) => actor.clone(),
            None => self.batch_execution_actor(row).await?,
        };
        self.batch_access(&batch.connection_id, &actor, &mut NoTransaction).await?;
        let original =
            row.frozen_row.as_ref().ok_or_else(|| Error::Internal("原入库资料缺失，禁止重复提交".into()))?;
        self.replay(&row.source.base.id, original, &actor, &mut NoTransaction).await
    }
}

fn submission_fingerprint(
    id: &str,
    request: &SubmitBatchRequest,
    rows: &[CatalogAdoptionBatchItem],
) -> Result<String> {
    if request.idempotency_key.trim().is_empty() || request.idempotency_key.len() > 128 {
        return Err(Error::ValidationError("请提供有效入库操作号".into()));
    }
    let fingerprints = request
        .item_ids
        .iter()
        .map(|item| {
            rows.iter()
                .find(|row| row.source.base.id == *item)
                .and_then(|row| row.frozen_row.as_ref())
                .ok_or_else(|| Error::ConflictError("确认项尚未完成核对".into()))?
                .fingerprint()
                .map_err(Error::from)
        })
        .collect::<Result<Vec<_>>>()?;
    let payload = serde_json::to_vec(&(id, request.preview_version, &request.item_ids, fingerprints))
        .map_err(|_| Error::Internal("入库确认无法编码".into()))?;
    Ok(hex::encode(Sha256::digest(payload)))
}

fn unknown_outcome(error: &Error) -> bool {
    matches!(
        error,
        Error::OutcomeUnknown(_) | Error::RepositoryError(PersistenceError::CommitOutcomeUnknown(_))
    )
}

fn record_execution_result(row: &mut CatalogAdoptionBatchItem, result: Result<AdoptionResult>) -> Result<()> {
    match result {
        Ok(result) => row.complete(result)?,
        Err(error) if unknown_outcome(&error) => {
            row.mark_unknown("操作结果暂无法确认，正在查询原入库结果，请勿重复提交".into())?
        },
        Err(error) => row.mark_failed(error.to_string())?,
    }
    Ok(())
}

fn preparation_failed(error: &Error) -> bool {
    matches!(
        error,
        Error::ValidationError(_)
            | Error::InvalidInput(_)
            | Error::BusinessLogicError(_)
            | Error::ConflictError(_)
            | Error::Forbidden(_)
            | Error::NotFound(_)
    )
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_supply::catalog_adoption::{AdoptionMapping, AdoptionRow, AdoptionTarget, BatchSalesValues};
    use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
    use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
    use erp_supply::ports::connector::common::{Snapshot, SourceRevision, SourceStamp, SupplierSku};
    use erp_supply::ports::connector::offer::{Offer, OfferDetails};
    use serde_json::json;

    use super::super::batch_preview::batch_preview;
    use super::*;

    fn row() -> CatalogAdoptionBatchItem {
        let source = SupplierCatalogItem::observed(
            "connection",
            "supplier",
            "job",
            Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(1),
                },
                value: Offer {
                    sku: SupplierSku { product_id: Some("product".into()), spec_id: "spec".into() },
                    name: "大米".into(),
                    specification: "5kg".into(),
                    unit: Some("袋".into()),
                    quote: None,
                    details: OfferDetails::default(),
                },
            },
        )
        .unwrap();
        let mapping = AdoptionMapping {
            category_id: "category".into(),
            unit_id: "unit".into(),
            product_kind: "PHYSICAL".into(),
            company_name: None,
            company_description: None,
        };
        let terms = SupplierOfferingTermsWrite {
            dropship_supply_price_gross: "100".into(),
            bulk_supply_price_gross: None,
            input_tax_rate: "0.13".into(),
            bulk_minimum_order_quantity: None,
            product_capabilities: vec![],
            valid_from: "2026-10-27".into(),
            valid_to: None,
        };
        let frozen = AdoptionRow {
            item_id: source.base.id.clone(),
            target: AdoptionTarget::Auto { mapping: mapping.clone(), expected_match: "verified".into() },
            terms: terms.clone(),
            sales_prices: None,
            delivery_template_id: None,
        };
        CatalogAdoptionBatchItem {
            base: BaseModel::fake(),
            batch_id: "batch".into(),
            source,
            previous_source: None,
            initially_adopted: false,
            mapping,
            terms: Some(terms),
            sales_prices: BatchSalesValues::default(),
            delivery_template_id: None,
            field_origins: Default::default(),
            status: BatchItemStatus::Ready,
            messages: vec![],
            warnings: vec![],
            warnings_confirmed: false,
            resolution: None,
            frozen_row: Some(frozen),
            result: None,
            operation_id: None,
            processing_started_at: None,
            preview_version: Some(2),
            submitted_by: None,
        }
    }

    #[test]
    fn confirmed_identity_tracks_preview_subset_and_price_payload() {
        let mut rows = vec![row()];
        let mut request = SubmitBatchRequest {
            expected_version: 5,
            preview_version: 2,
            item_ids: vec![rows[0].source.base.id.clone()],
            idempotency_key: "original".into(),
        };
        let original = submission_fingerprint("batch", &request, &rows).unwrap();
        request.expected_version = 6;
        assert_eq!(original, submission_fingerprint("batch", &request, &rows).unwrap());
        rows[0].frozen_row.as_mut().unwrap().terms.input_tax_rate = "0.09".into();
        assert_ne!(original, submission_fingerprint("batch", &request, &rows).unwrap());
        request.preview_version = 3;
        assert_ne!(original, submission_fingerprint("batch", &request, &rows).unwrap());
        request.item_ids = vec!["unknown".into()];
        assert!(submission_fingerprint("batch", &request, &rows).is_err());
        request.idempotency_key = String::new();
        assert!(submission_fingerprint("batch", &request, &rows).is_err());
    }

    #[test]
    fn persisted_batch_source_review_uses_representable_time_and_rejects_changed_quote() {
        let row = row();
        let mut value = serde_json::to_value(BaseModel::fake()).unwrap();
        value.as_object_mut().unwrap().extend(
            json!({
                "batch_no":"SCA20261027-000001", "connection_id":"connection", "supplier_id":"supplier",
                "actor_id":"creator", "selection_filter":null, "selected_count":1, "excluded_ids":[],
                "rules":{"mode":"SAME","rule":{"method":"UNSET"},"minimum_quantity":null},
                "fill_policy":{"overwrite":false,"preserve_manual":true}, "defaults":{},
                "step":"REVIEW", "preview_version":2, "submitted":false
            })
            .as_object()
            .unwrap()
            .clone(),
        );
        let batch: CatalogAdoptionBatch = serde_json::from_value(value).unwrap();
        let preview = batch_preview(&batch, &row);
        assert_eq!(preview.connection_id, "connection");
        assert!(preview.ensure_current_item(&row.source).is_ok());
        let mut changed = row.source.clone();
        changed.purchase_price = Some("101.00".into());
        assert!(preview.ensure_current_item(&changed).is_err());
    }

    #[test]
    fn unknown_and_storage_failures_cannot_become_explicit_retryable_failures() {
        let mongo = mongodb::error::Error::custom("unconfirmed");
        let unknown = Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(mongo));
        assert!(unknown_outcome(&unknown));
        assert!(!preparation_failed(&unknown));
        let conflict = Error::RepositoryError(PersistenceError::OptimisticLockingError);
        assert!(!preparation_failed(&conflict));
        assert!(preparation_failed(&Error::ValidationError("来源已变化".into())));
        assert!(!unknown_outcome(&Error::ValidationError("来源已变化".into())));
    }

    #[test]
    fn changed_source_fails_only_its_row_and_unknown_keeps_original_identity() {
        let (mut changed, mut valid, mut unknown) = (row(), row(), row());
        for item in [&mut changed, &mut valid, &mut unknown] {
            item.freeze("original".into(), 2).unwrap();
            item.begin().unwrap();
        }
        let failure = Error::ConflictError("来源报价已变化".into());
        let reason = failure.to_string();
        record_execution_result(&mut changed, Err(failure)).unwrap();
        let valid_item_id = valid.source.base.id.clone();
        record_execution_result(
            &mut valid,
            Ok(AdoptionResult {
                item_id: valid_item_id,
                product_id: "product".into(),
                sku_id: "sku".into(),
                offering_id: "offering".into(),
            }),
        )
        .unwrap();
        record_execution_result(
            &mut unknown,
            Err(Error::OutcomeUnknown(PersistenceError::CommitOutcomeUnknown(
                mongodb::error::Error::custom("uncertain commit"),
            ))),
        )
        .unwrap();
        assert_eq!(changed.status, BatchItemStatus::Failed);
        assert_eq!(changed.messages, [reason]);
        assert!(changed.editable());
        assert_eq!(valid.status, BatchItemStatus::Succeeded);
        assert_eq!(valid.result.unwrap().offering_id, "offering");
        assert_eq!(unknown.status, BatchItemStatus::Unknown);
        assert_eq!(unknown.operation_id.as_deref(), Some("original"));
        assert!(!unknown.editable());
        assert!(unknown.freeze("replacement".into(), 2).is_err());
    }
}
