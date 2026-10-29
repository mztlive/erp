//! 快速冻结身份清单并登记后台准备；不在 HTTP 请求中逐项生成草稿。

use std::collections::HashSet;

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::BusinessDate;
use erp_supply::catalog_adoption::{
    BatchAdoptionExt, BatchDefaults, BatchPreparation, BatchPricing, CatalogAdoptionBatch, FillPolicy,
    SELECTION_PAGE_SIZE, SelectionPage, adoption_identity, insert_selection_pages, selection_by_request,
};
use erp_supply::repository::supplier_catalog;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind, next_id};
use persistence_core::{Executor, NoTransaction, Transactional};

use super::batch::batch_view;
use super::{BatchView, CreateBatchRequest, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

struct SelectionRegistration {
    request_key: String,
    batch_no: String,
    fingerprint: String,
}

impl SupplierCatalogAdoptionProcess {
    /// 冻结轻量成员清单并登记准备任务；相同操作号返回原批次。
    /// # 参数
    /// 连接、选择请求与当前认证用户。
    /// # 返回
    /// 已持久化的批次摘要，不返回全量草稿明细。
    /// # 错误
    /// 越权、空结果、异载荷复用或存储失败时拒绝。
    pub async fn create_batch(
        &self,
        connection: &str,
        request: CreateBatchRequest,
        actor: &AuditActor,
    ) -> Result<BatchView> {
        let fingerprint = request.fingerprint()?;
        self.batch_access(connection, actor, &mut NoTransaction).await?;
        let id = adoption_identity(&["selection", connection, actor.id(), &request.idempotency_key]);
        if let Some(batch) =
            self.existing_selection(connection, &id, &fingerprint, actor, &mut NoTransaction).await?
        {
            return Ok(batch_view(batch, vec![]));
        }
        let batch_no = DocumentNumberGenerator::new(self.db.clone())
            .next_number(
                DocumentNumberKind::SupplierCatalogAdoption,
                BusinessDate::today().as_naive_date(),
                &mut NoTransaction,
            )
            .await
            .map_err(|error| Error::Internal(error.to_string()))?;
        let (process, connection, actor) = (self.clone(), connection.to_owned(), actor.clone());
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    if let Some(batch) =
                        process.existing_selection(&connection, &id, &fingerprint, &actor, executor).await?
                    {
                        return Ok(batch_view(batch, vec![]));
                    }
                    let batch = process
                        .register_selection(
                            &connection,
                            request,
                            &actor,
                            SelectionRegistration { request_key: id, batch_no, fingerprint },
                            executor,
                        )
                        .await?;
                    Ok(batch_view(batch, vec![]))
                })
            })
            .await
    }

    /// 在原事务内核验操作号与已保存选择的归属及载荷。
    async fn existing_selection(
        &self,
        connection: &str,
        id: &str,
        fingerprint: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<CatalogAdoptionBatch>> {
        let Some(batch) = selection_by_request(&self.db, id, executor).await? else { return Ok(None) };
        let source = self.batch_access(connection, actor, executor).await?;
        if batch.supplier_id != source.supplier_id.as_ref()
            || batch.connection_id != connection
            || batch.actor_id != actor.id()
            || batch.preparation.as_ref().is_none_or(|state| state.fingerprint != fingerprint)
        {
            return Err(Error::ConflictError("原选择操作号已用于其他范围，请重新选择".into()));
        }
        Ok(Some(batch))
    }

    /// 保存身份集合及批次头，所有写入与一致性读取原子提交。
    async fn register_selection(
        &self,
        connection: &str,
        request: CreateBatchRequest,
        actor: &AuditActor,
        registration: SelectionRegistration,
        executor: &mut dyn Executor,
    ) -> Result<CatalogAdoptionBatch> {
        let source = self.batch_access(connection, actor, executor).await?;
        self.ensure_source(&source)?;
        let ids = supplier_catalog::selection_ids(
            &self.db,
            connection,
            request.filter.as_ref(),
            request.item_ids.as_deref(),
            executor,
        )
        .await?;
        let members: HashSet<_> = ids.iter().collect();
        if ids.is_empty()
            || request.item_ids.as_ref().is_some_and(|values| values.len() != ids.len())
            || request.excluded_ids.iter().any(|value| !members.contains(value))
        {
            return Err(Error::ValidationError("当前选择为空、来源不可访问或排除项不属于本次范围".into()));
        }
        let batch = selection_batch(
            connection,
            source.supplier_id.to_string(),
            actor.id(),
            request,
            ids.len(),
            registration,
        );
        let pages = ids
            .chunks(SELECTION_PAGE_SIZE)
            .enumerate()
            .map(|(page, ids)| {
                Ok(SelectionPage {
                    batch_id: batch.base.id.clone(),
                    page: u32::try_from(page).map_err(|_| Error::ValidationError("选择范围过大".into()))?,
                    item_ids: ids.to_vec(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        self.db.catalog_adoption_batches().create(&batch, executor).await?;
        insert_selection_pages(&self.db, &pages, executor).await?;
        Ok(batch)
    }
}

/// 组装尚未生成草稿的批次；所有成员输入已由受理事务核验。
fn selection_batch(
    connection: &str,
    supplier_id: String,
    actor: &str,
    request: CreateBatchRequest,
    selected_count: usize,
    registration: SelectionRegistration,
) -> CatalogAdoptionBatch {
    CatalogAdoptionBatch {
        base: BaseModel::new(next_id()),
        batch_no: registration.batch_no,
        connection_id: connection.into(),
        supplier_id,
        actor_id: actor.into(),
        selection_filter: request.filter,
        selected_count,
        excluded_ids: request.excluded_ids,
        rules: BatchPricing::default(),
        fill_policy: FillPolicy::default(),
        defaults: BatchDefaults::default(),
        step: "PRICING".into(),
        preview_version: None,
        submitted: false,
        selection_complete: false,
        preparation: Some(BatchPreparation::new(registration.request_key, registration.fingerprint)),
        execution_pending: false,
        draft_operation: None,
        draft_started_at: None,
        inactive_pricing: None,
        published_operations: vec![],
        confirmations: vec![],
    }
}
