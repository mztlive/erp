use application_core::AuditActor;
use entity_core::BaseModel;
use erp_audit::AuditActorLogs;
use erp_audit::execution::{persist_log, recover_command};
use erp_catalog::CatalogExt;
use erp_catalog::source_sync::repository::ProductSourceExt;
use erp_core::common::time::Instant;
use erp_permissions::{catalog as catalog_permissions, supplier_offering};
use erp_supply::catalog_adoption::{
    AdoptionResult, AdoptionRow, AdoptionTarget, BatchMatchAnchor, CatalogAdoptionCommand,
    CatalogAdoptionExt, CatalogAdoptionPreview, CatalogAdoptionReceipt, ConfirmAdoptionRequest,
    adoption_identity,
};
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use erp_supply::service::supplier_offering::CommandPreparation;
use persistence_core::{Executor, Transactional};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::SupplierCatalogAdoptionProcess;
use super::product::PreparedAdoption;
use super::request::offering_request;
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
    pub retry_original: bool,
}

#[derive(Serialize)]
pub struct AdoptionBatchResult {
    pub rows: Vec<AdoptionRowResult>,
}

#[derive(Clone)]
pub(super) struct AdoptionExecution {
    /// 持久化旧式预览须复验创建人；批次生成的内部视图由批次入口授权。
    pub preview_id: Option<String>,
    pub preview: CatalogAdoptionPreview,
    pub source: SupplierCatalogItem,
    pub row: AdoptionRow,
    pub command: String,
    pub group_anchor: Option<BatchMatchAnchor>,
}

impl From<AdoptionResult> for AdoptionRowResult {
    fn from(result: AdoptionResult) -> Self {
        Self {
            item_id: result.item_id,
            status: "SUCCEEDED",
            retry_original: false,
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
        let (preview, command_id) =
            self.confirmation_snapshot(connection, preview_id, &request, actor).await?;
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
                    retry_original: matches!(error, Error::OutcomeUnknown(_)),
                    product_id: None,
                    sku_id: None,
                    offering_id: None,
                    message: Some(error.to_string()),
                },
            });
        }
        Ok(AdoptionBatchResult { rows })
    }

    /// 同一快照核验预览归属及连接范围，再冻结幂等操作。
    async fn confirmation_snapshot(
        &self,
        connection: &str,
        preview_id: &str,
        request: &ConfirmAdoptionRequest,
        actor: &AuditActor,
    ) -> Result<(CatalogAdoptionPreview, String)> {
        let command = confirmation_command(connection, preview_id, request, actor)?;
        let outcome = self.confirmation_attempt(connection, preview_id, request, actor, &command).await;
        let Err(error) = outcome else {
            return outcome;
        };
        recover_command(
            error,
            self.confirmation_replay(connection, preview_id, request, actor, &command).await,
        )
    }

    /// 首次确认在快照中检查并冻结原操作，不在失败事务中恢复。
    async fn confirmation_attempt(
        &self,
        connection: &str,
        preview_id: &str,
        request: &ConfirmAdoptionRequest,
        actor: &AuditActor,
        command: &CatalogAdoptionCommand,
    ) -> Result<(CatalogAdoptionPreview, String)> {
        let (source, target, request, actor, command) =
            (connection.to_owned(), preview_id.to_owned(), request.clone(), actor.clone(), command.clone());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move {
                let preview =
                    process.confirmation_preview(&source, &target, &request, &actor, executor).await?;
                let id = process.freeze_command(&command, executor).await?;
                Ok((preview, id))
            })
        })
        .await
    }

    /// 新快照只回读精确原操作，不重放尚不明确的写入。
    async fn confirmation_replay(
        &self,
        connection: &str,
        preview_id: &str,
        request: &ConfirmAdoptionRequest,
        actor: &AuditActor,
        command: &CatalogAdoptionCommand,
    ) -> Result<Option<(CatalogAdoptionPreview, String)>> {
        let (source, target, request, actor, expected) =
            (connection.to_owned(), preview_id.to_owned(), request.clone(), actor.clone(), command.clone());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move {
                let preview =
                    process.confirmation_preview(&source, &target, &request, &actor, executor).await?;
                let stored =
                    process.db.catalog_adoption_commands().find_by_id(&expected.base.id, executor).await?;
                stored.map(|stored| command_identity(&stored, &expected).map(|id| (preview, id))).transpose()
            })
        })
        .await
    }

    /// 原操作恢复仍核验当前预览归属与来源，不在失败事务内回读。
    async fn confirmation_preview(
        &self,
        connection: &str,
        preview_id: &str,
        request: &ConfirmAdoptionRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CatalogAdoptionPreview> {
        self.require_preview_owner(actor, preview_id, executor).await?;
        let preview = self
            .db
            .catalog_adoption_previews()
            .find_by_id(preview_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("选品预览不存在".into()))?;
        preview.ensure_request(connection, request)?;
        self.batch_access(connection, actor, executor).await?;
        Ok(preview)
    }

    async fn replay_expired_batch(
        &self,
        request: &ConfirmAdoptionRequest,
        actor: &AuditActor,
    ) -> Result<AdoptionBatchResult> {
        let (request, actor) = (request.clone(), actor.clone());
        self.with_snapshot(move |process, executor| {
            Box::pin(async move {
                let mut rows = Vec::with_capacity(request.rows.len());
                for row in &request.rows {
                    let result = process
                        .replay(&row.item_id, row, &actor, executor)
                        .await?
                        .ok_or_else(|| Error::ConflictError("选品预览已过期，未完成行须重新预览".into()))?;
                    rows.push(result.into());
                }
                Ok(AdoptionBatchResult { rows })
            })
        })
        .await
    }

    /// 同一事务读取并创建原操作；并发重复由入口在新快照中恢复。
    async fn freeze_command(
        &self,
        command: &CatalogAdoptionCommand,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        let repository = self.db.catalog_adoption_commands();
        if let Some(existing) = repository.find_by_id(&command.base.id, executor).await? {
            return command_identity(&existing, command);
        }
        repository.create(command, executor).await?;
        Ok(command.base.id.clone())
    }

    pub(super) async fn confirm_row(
        &self,
        preview: &CatalogAdoptionPreview,
        frozen: &SupplierCatalogItem,
        row: &AdoptionRow,
        command: &str,
        actor: &AuditActor,
    ) -> Result<AdoptionResult> {
        self.confirm_execution(
            AdoptionExecution {
                preview_id: Some(preview.base.id.clone()),
                preview: preview.clone(),
                source: frozen.clone(),
                row: row.clone(),
                command: command.into(),
                group_anchor: None,
            },
            actor,
        )
        .await
    }

    pub(super) async fn confirm_execution(
        &self,
        execution: AdoptionExecution,
        actor: &AuditActor,
    ) -> Result<AdoptionResult> {
        let AdoptionExecution { source: frozen, row, command, group_anchor, .. } = &execution;
        let (check, acting) = (execution.clone(), actor.clone());
        if let Some(result) = self
            .with_snapshot(move |process, executor| {
                Box::pin(async move {
                    process.require_execution_access(&check, &acting, executor).await?;
                    let result = process.replay(&check.row.item_id, &check.row, &acting, executor).await?;
                    if result.is_none() {
                        let current = process
                            .load_item(&check.preview.connection_id, &check.row.item_id, executor)
                            .await?;
                        check.preview.ensure_current_item(&current)?;
                    }
                    Ok(result)
                })
            })
            .await?
        {
            return Ok(result);
        }
        let prepared =
            self.prepare_row_with_anchor(frozen, row, actor, command, group_anchor.as_ref()).await?;
        let cleanup = prepared.media.cleanup_handle();
        let process = self.clone();
        let actor = actor.clone();
        let outcome = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { process.commit_row(execution, prepared, &actor, executor).await })
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

    /// 预执行和逐项写事务使用相同的预览归属与来源供应商授权。
    async fn require_execution_access(
        &self,
        execution: &AdoptionExecution,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if let Some(id) = &execution.preview_id {
            self.require_preview_owner(actor, id, executor).await?;
        }
        self.batch_access(&execution.preview.connection_id, actor, executor).await?;
        Ok(())
    }

    async fn commit_row(
        &self,
        execution: AdoptionExecution,
        mut prepared: PreparedAdoption,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(AdoptionResult, bool)> {
        let AdoptionExecution { preview, row, command, group_anchor, .. } = &execution;
        self.require_execution_access(&execution, actor, executor).await?;
        if let Some(result) = self.replay(&row.item_id, row, actor, executor).await? {
            return Ok((result, false));
        }
        let mut item = self.load_item(&preview.connection_id, &row.item_id, executor).await?;
        preview.ensure_current_item(&item)?;
        self.db.catalog().lock_match_name(&item.name, executor).await?;
        let mut resolved_row = self
            .automatic_row_with_anchor(&item, row, actor, command, group_anchor.as_ref(), executor)
            .await?;
        if let (
            AdoptionTarget::Create { brand_id, .. },
            AdoptionTarget::Create { brand_id: prepared_brand, .. },
        ) = (&mut resolved_row.target, &prepared.resolved_row.target)
            && brand_id.is_empty()
        {
            *brand_id = prepared_brand.clone();
        }
        let sku = self.materialize_target(&item, &resolved_row, &mut prepared, actor, executor).await?;
        let per_item_command = adoption_identity(&[command, &row.item_id]);
        let req = offering_request(&item, row, &sku.base.id, &per_item_command)?;
        let service = &self.services.offerings;
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
        self.record_source(&mut item, prepared, &result, &resolved_row.target, actor, executor).await?;
        self.persist_result(&item.connection_id, &result, row, command, actor, executor).await?;
        Ok((result, true))
    }

    async fn record_source(
        &self,
        item: &mut SupplierCatalogItem,
        prepared: PreparedAdoption,
        result: &AdoptionResult,
        target: &AdoptionTarget,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if item.images != prepared.materialized_item.images
            || item.detail_images != prepared.materialized_item.detail_images
        {
            item.media_retry_at = Some(Instant::now().unix_secs());
        }
        item.refresh_fingerprint()?;
        self.db.supplier_catalog_items().update(item, executor).await?;
        let mut initial_source = item.clone();
        initial_source.images = prepared.materialized_item.images;
        initial_source.detail_images = prepared.materialized_item.detail_images;
        if matches!(target, AdoptionTarget::Create { .. })
            && (prepared.creates_product
                || self.db.product_sources().find_by_id(&result.product_id, executor).await?.is_some())
        {
            initialize_source(
                &self.db,
                &self.services.catalog,
                &initial_source,
                &result.product_id,
                actor,
                executor,
            )
            .await?;
        }
        Ok(())
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

    pub(super) async fn replay(
        &self,
        item_id: &str,
        row: &AdoptionRow,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Option<AdoptionResult>> {
        let Some(receipt) = self.db.catalog_adoption_receipts().find_by_id(item_id, executor).await? else {
            return Ok(None);
        };
        self.services
            .catalog_access
            .require_product(
                actor,
                &catalog_permissions::PRODUCT_PRODUCT_DETAIL,
                &receipt.result.product_id,
                executor,
            )
            .await?;
        self.services
            .offering_access
            .require_offering(actor, &supplier_offering::LIST, &receipt.result.offering_id, executor)
            .await?;
        Ok(Some(receipt.replay(&row.fingerprint()?)?))
    }
}

/// 原操作身份固定连接、账号与全部请求载荷。
fn confirmation_command(
    connection: &str,
    preview_id: &str,
    request: &ConfirmAdoptionRequest,
    actor: &AuditActor,
) -> Result<CatalogAdoptionCommand> {
    let payload =
        serde_json::to_vec(&(preview_id, request)).map_err(|_| Error::Internal("选品批次无法编码".into()))?;
    Ok(CatalogAdoptionCommand {
        base: BaseModel::new(adoption_identity(&[connection, actor.id(), &request.idempotency_key])),
        connection_id: connection.into(),
        actor_id: actor.id().into(),
        fingerprint: hex::encode(Sha256::digest(payload)),
    })
}

/// 精确核验原操作，软删除或异载荷不能作为幂等成功返回。
fn command_identity(stored: &CatalogAdoptionCommand, expected: &CatalogAdoptionCommand) -> Result<String> {
    if stored.base.is_deleted()
        || stored.base.id != expected.base.id
        || stored.connection_id != expected.connection_id
        || stored.actor_id != expected.actor_id
        || stored.fingerprint != expected.fingerprint
    {
        return Err(Error::ConflictError("同一操作号已用于不同选品提交".into()));
    }
    Ok(stored.base.id.clone())
}

#[cfg(test)]
mod command_tests {
    use erp_core::AccountKind;

    use super::*;

    /// 构造生产确认操作，不访问数据库。
    fn command() -> CatalogAdoptionCommand {
        confirmation_command(
            "connection",
            "preview",
            &ConfirmAdoptionRequest {
                expected_version: 1,
                idempotency_key: "same-operation".into(),
                rows: vec![],
            },
            &AuditActor::new("actor".into(), "login".into(), AccountKind::Admin),
        )
        .unwrap()
    }

    /// 相同操作的并发回读使用原身份；业务版本差异不改变操作身份。
    #[test]
    fn concurrent_confirmation_recovery_preserves_original_operation() {
        let expected = command();
        let mut stored = expected.clone();
        stored.base.version += 1;
        assert_eq!(command_identity(&stored, &expected).unwrap(), expected.base.id);
    }

    /// 任何身份、载荷或软删除差异均拒绝，不能接受其他确认回执。
    #[test]
    fn recovery_rejects_deleted_or_mismatched_confirmation() {
        let expected = command();
        for mutation in 0..5 {
            let mut stored = expected.clone();
            match mutation {
                0 => stored.base.id = "other".into(),
                1 => stored.actor_id = "other".into(),
                2 => stored.connection_id = "other".into(),
                3 => stored.fingerprint = "other".into(),
                _ => stored.base.deleted_at = 1,
            }
            assert!(matches!(command_identity(&stored, &expected), Err(Error::ConflictError(_))));
        }
    }
}
