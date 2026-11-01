//! 原确认查证；未受理时先封存原操作号，避免迟到请求与新确认并行执行。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_supply::catalog_adoption::{CatalogAdoptionCommand, CatalogAdoptionExt, adoption_identity};
use persistence_core::{Executor, Transactional};

use super::batch_request::BatchSubmissionStatus;
use super::{BatchSubmissionView, SubmitBatchRequest, SupplierCatalogAdoptionProcess};
use crate::{Error, Result};

impl SupplierCatalogAdoptionProcess {
    /// 查证原确认是否受理，未受理时原子封存操作号；不执行商品入库。
    /// # 参数
    /// 连接、批次、浏览器保留的完整原确认及当前认证用户。
    /// # 返回
    /// 已受理或已封存未受理的确定结果，以及当前批次内容。
    /// # 错误
    /// 越权、身份不一致、并发冲突或提交结果未知时保持待核实并返回错误。
    pub async fn query_batch_submission(
        &self,
        connection: &str,
        id: &str,
        request: SubmitBatchRequest,
        actor: &AuditActor,
    ) -> Result<BatchSubmissionView> {
        let view = self.batch(connection, id, actor).await?;
        request.unaccepted_fingerprint(id)?;
        if request.item_ids.iter().any(|id| !view.rows.iter().any(|row| row.source.base.id == *id)) {
            return Err(Error::ValidationError("原确认项不属于本批范围".into()));
        }
        let (process, source, batch_id, acting) =
            (self.clone(), connection.to_owned(), id.to_owned(), actor.clone());
        let status = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.resolve_batch_submission(&source, &batch_id, &request, &acting, executor).await
                })
            })
            .await?;
        Ok(BatchSubmissionView { status, view: self.batch(connection, id, actor).await? })
    }

    async fn resolve_batch_submission(
        &self,
        connection: &str,
        id: &str,
        request: &SubmitBatchRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BatchSubmissionStatus> {
        let batch = self.load_batch(connection, id, actor, executor).await?;
        let fingerprint = request.unaccepted_fingerprint(id)?;
        let command = adoption_identity(&[connection, actor.id(), &request.idempotency_key]);
        let existing = self.db.catalog_adoption_commands().find_by_id(&command, executor).await?;
        if let Some(confirmation) = batch.confirmations.iter().find(|entry| entry.operation_id == command) {
            request.ensure_confirmation(confirmation, actor.id())?;
            if existing.is_none_or(|record| {
                record.base.is_deleted() || record.fingerprint != confirmation.fingerprint
            }) {
                return Err(Error::ConflictError("原入库确认记录不完整，请继续核查".into()));
            }
            return Ok(BatchSubmissionStatus::Accepted);
        }
        if let Some(existing) = existing {
            if existing.base.is_deleted() || existing.fingerprint != fingerprint {
                return Err(Error::ConflictError("原操作号已用于其他确认，请核对原请求".into()));
            }
        } else {
            let record = CatalogAdoptionCommand {
                base: BaseModel::new(command),
                connection_id: connection.into(),
                actor_id: actor.id().into(),
                fingerprint,
            };
            self.db.catalog_adoption_commands().create(&record, executor).await?;
        }
        Ok(BatchSubmissionStatus::NotAccepted)
    }
}
