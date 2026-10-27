use application_core::AuditActor;
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_supply::catalog_adoption::{
    AdoptionResult, AdoptionRow, CatalogAdoptionCommand, CatalogAdoptionExt, CatalogAdoptionPreview,
    CatalogAdoptionReceipt, ConfirmAdoptionRequest, adoption_identity,
};
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_api::SupplierApiService;
use erp_supply::service::supplier_offering::CommandPreparation;
use persistence_core::{Executor, NoTransaction, Transactional};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::SupplierCatalogAdoptionProcess;
use super::product::PreparedAdoption;
use super::request::offering_request;
use crate::adapters::{catalog_access, offering_access, scoped_offering_service};
use crate::audit::persist_log;
use crate::product_source_sync::initialize_source;
use crate::supply_governance::offering::MongoOfferingQualification;
use crate::{Error, Result};

#[derive(Serialize)]
pub struct AdoptionRowResult {
    pub item_id: String,
    pub status: &'static str,
    pub product_id: Option<String>,
    pub sku_id: Option<String>,
    pub offering_id: Option<String>,
    pub message: Option<String>,
}

#[derive(Serialize)]
pub struct AdoptionBatchResult {
    pub rows: Vec<AdoptionRowResult>,
}

impl From<AdoptionResult> for AdoptionRowResult {
    fn from(result: AdoptionResult) -> Self {
        Self {
            item_id: result.item_id,
            status: "SUCCEEDED",
            product_id: Some(result.product_id),
            sku_id: Some(result.sku_id),
            offering_id: Some(result.offering_id),
            message: Some("已纳入公司商品库，请完善销售条件后上架".into()),
        }
    }
}

impl SupplierCatalogAdoptionProcess {
    /// 分批确认冻结选品，每行独立事务，失败不留下半成品商品或供给。
    /// # 参数
    /// 连接、预览身份、原始操作号和逐行公司映射及采购条款。
    /// # 返回
    /// 每行真实成功结果或明确失败原因。
    /// # 错误
    /// 批次越权、过期、异载荷或非法选择直接拒绝；行错误逐项返回。
    pub async fn confirm(
        &self,
        connection: &str,
        preview_id: &str,
        request: ConfirmAdoptionRequest,
        actor: &AuditActor,
    ) -> Result<AdoptionBatchResult> {
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        self.ensure_permission(actor, "supplier_offering_cost:detail").await?;
        let preview = self
            .db
            .catalog_adoption_previews()
            .find_by_id(preview_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("选品预览不存在".into()))?;
        preview.ensure_request(connection, actor.id(), &request)?;
        let command_id = self.freeze_command(connection, preview_id, &request, actor).await?;
        if preview.expires_at <= Instant::now() {
            return self.replay_expired_batch(&request, actor).await;
        }
        let mut rows = Vec::with_capacity(request.rows.len());
        for row in request.rows {
            let item = preview
                .items
                .iter()
                .find(|item| item.base.id == row.item_id)
                .ok_or_else(|| Error::ValidationError("选品冻结行不存在".into()))?;
            let outcome = self.confirm_row(&preview, item, &row, &command_id, actor).await;
            rows.push(match outcome {
                Ok(result) => result.into(),
                Err(error) => AdoptionRowResult {
                    item_id: row.item_id,
                    status: "FAILED",
                    product_id: None,
                    sku_id: None,
                    offering_id: None,
                    message: Some(error.to_string()),
                },
            });
        }
        Ok(AdoptionBatchResult { rows })
    }

    async fn replay_expired_batch(
        &self,
        request: &ConfirmAdoptionRequest,
        actor: &AuditActor,
    ) -> Result<AdoptionBatchResult> {
        let mut rows = Vec::with_capacity(request.rows.len());
        for row in &request.rows {
            let result = self
                .replay(&row.item_id, row, actor, &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::ConflictError("选品预览已过期，未完成行须重新预览".into()))?;
            rows.push(result.into());
        }
        Ok(AdoptionBatchResult { rows })
    }

    async fn freeze_command(
        &self,
        connection: &str,
        preview_id: &str,
        request: &ConfirmAdoptionRequest,
        actor: &AuditActor,
    ) -> Result<String> {
        let id = adoption_identity(&[connection, actor.id(), &request.idempotency_key]);
        let payload = serde_json::to_vec(&(preview_id, request))
            .map_err(|_| Error::Internal("选品批次无法编码".into()))?;
        let fingerprint = hex::encode(Sha256::digest(payload));
        let repository = self.db.catalog_adoption_commands();
        if let Some(existing) = repository.find_by_id(&id, &mut NoTransaction).await? {
            if existing.base.is_deleted() || existing.fingerprint != fingerprint {
                return Err(Error::ConflictError("同一操作号已用于不同选品提交".into()));
            }
            return Ok(id);
        }
        let command = CatalogAdoptionCommand {
            base: BaseModel::new(id.clone()),
            connection_id: connection.into(),
            actor_id: actor.id().into(),
            fingerprint,
        };
        if let Err(error) = repository.create(&command, &mut NoTransaction).await {
            let existing = repository.find_by_id(&id, &mut NoTransaction).await?;
            if existing.is_none_or(|existing| {
                existing.base.is_deleted() || existing.fingerprint != command.fingerprint
            }) {
                return Err(error.into());
            }
        }
        Ok(id)
    }

    async fn confirm_row(
        &self,
        preview: &CatalogAdoptionPreview,
        frozen: &SupplierCatalogItem,
        row: &AdoptionRow,
        command: &str,
        actor: &AuditActor,
    ) -> Result<AdoptionResult> {
        if let Some(result) = self.replay(&frozen.base.id, row, actor, &mut NoTransaction).await? {
            return Ok(result);
        }
        let current = self.load_item(&preview.connection_id, &row.item_id, &mut NoTransaction).await?;
        preview.ensure_current_item(&current)?;
        let prepared = self.prepare_row(frozen, row, actor).await?;
        let cleanup = prepared.media.cleanup_handle();
        let process = self.clone();
        let (preview, row, command, actor) =
            (preview.clone(), row.clone(), command.to_owned(), actor.clone());
        let outcome = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.commit_row(&preview, &row, &command, prepared, &actor, executor).await
                })
            })
            .await;
        cleanup.after_transaction(&outcome).await;
        match outcome {
            Ok((result, used)) => {
                if !used {
                    cleanup.discard_uncommitted().await;
                }
                Ok(result)
            },
            Err(error) => Err(error),
        }
    }

    async fn commit_row(
        &self,
        preview: &CatalogAdoptionPreview,
        row: &AdoptionRow,
        command: &str,
        mut prepared: PreparedAdoption,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(AdoptionResult, bool)> {
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        if let Some(result) = self.replay(&row.item_id, row, actor, executor).await? {
            return Ok((result, false));
        }
        let mut item = self.load_item(&preview.connection_id, &row.item_id, executor).await?;
        preview.ensure_current_item(&item)?;
        SupplierApiService::new(self.db.clone()).load_connection(&preview.connection_id, executor).await?;
        let sku = self.materialize_target(&item, row, &mut prepared, actor, executor).await?;
        let per_item_command = adoption_identity(&[command, &row.item_id]);
        let req = offering_request(&item, row, &sku.base.id, &per_item_command)?;
        let service = scoped_offering_service(self.db.clone(), self.rbac.clone());
        let qualification = MongoOfferingQualification::new(self.db.clone());
        let offering = match service.prepare_create(&req, actor, &qualification, executor).await? {
            CommandPreparation::Apply(mut prepared) => {
                prepared.retain_api_observation(item.observed_at, item.availability_valid_until)?;
                service.persist_created(&prepared, executor).await?;
                prepared.result
            },
            CommandPreparation::Replay(result) => result,
        };
        let result = AdoptionResult {
            item_id: row.item_id.clone(),
            product_id: sku.product_id.to_string(),
            sku_id: sku.base.id.clone(),
            offering_id: offering.offering_id,
        };
        item.record_adoption(
            item.base.version,
            sku.base.id,
            sku.sku_no,
            result.product_id.clone(),
            result.offering_id.clone(),
        )?;
        if item.images != prepared.materialized_item.images
            || item.detail_images != prepared.materialized_item.detail_images
        {
            item.media_retry_at = Some(Instant::now().unix_secs());
        }
        item.refresh_fingerprint()?;
        self.db.supplier_catalog_items().update(&mut item, executor).await?;
        let mut initial_source = item.clone();
        initial_source.images = prepared.materialized_item.images;
        initial_source.detail_images = prepared.materialized_item.detail_images;
        initialize_source(&self.db, &initial_source, &result.product_id, actor, executor).await?;
        self.persist_result(&item.connection_id, &result, row, command, actor, executor).await?;
        Ok((result, true))
    }

    async fn persist_result(
        &self,
        connection: &str,
        result: &AdoptionResult,
        row: &AdoptionRow,
        command: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let receipt = CatalogAdoptionReceipt {
            base: BaseModel::new(result.item_id.clone()),
            connection_id: connection.into(),
            item_id: result.item_id.clone(),
            actor_id: actor.id().into(),
            command_id: command.into(),
            fingerprint: row.fingerprint()?,
            result: result.clone(),
        };
        self.db.catalog_adoption_receipts().create(&receipt, executor).await?;
        let audit =
            actor.clone().resource_log("supplier_catalog.adopt", "product", result.product_id.clone())?;
        persist_log(&self.db, &audit, executor).await?;
        Ok(())
    }

    async fn replay(
        &self,
        item_id: &str,
        row: &AdoptionRow,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<AdoptionResult>> {
        let Some(receipt) = self.db.catalog_adoption_receipts().find_by_id(item_id, executor).await? else {
            return Ok(None);
        };
        catalog_access(self.db.clone(), self.rbac.clone())
            .require_product(actor, "detail", &receipt.result.product_id, executor)
            .await?;
        offering_access(self.db.clone(), self.rbac.clone())
            .require_offering(actor, "list", &receipt.result.offering_id, executor)
            .await?;
        Ok(Some(receipt.replay(&row.fingerprint()?)?))
    }
}
