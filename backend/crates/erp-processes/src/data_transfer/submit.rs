use application_core::AuditActor;
use application_core::command::CommandFingerprint;
use erp_audit::{AuditActorLogs, AuditLog};
use erp_core::ids::{BackgroundJobId, BackgroundJobItemId};
use erp_support::repository::bulk_job::BackgroundJobInput;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobItem,
    BackgroundJobItemDraft, BackgroundJobView, BulkJobExt, FileAsset, FileAssetExt, JobType,
    content_fingerprint,
};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use serde_json::Value;
use sha2::{Digest, Sha256};

use super::context_input::ContextInput;
use super::types::Envelope;
use super::{DataTransferProcess, TransferKind, TransferRequest};
use crate::audit::persist_log;
use crate::{Error, Result};

pub(super) const DOMAIN: &str = "DATA_TRANSFER";
impl DataTransferProcess {
    /// 校验并保存私有输入，原子登记任务与源文件资产。
    /// # 参数
    /// 稳定操作号、业务参数和已认证提交人。
    /// # 返回
    /// 新任务或同内容重放的原任务。
    /// # 错误
    /// 非法输入、异内容重试、授权或持久化失败时拒绝。
    pub async fn submit(&self, request: TransferRequest, actor: &AuditActor) -> Result<BackgroundJobView> {
        validate_request(&request)?;
        let encoded =
            serde_json::to_string(&request).map_err(|_| Error::ValidationError("任务参数无法编码".into()))?;
        let digest = content_fingerprint(&encoded, self.codec.fingerprint_key());
        let identity = CommandFingerprint::from_parts([actor.id().into(), request.request_id.clone()]);
        let request_id = format!("transfer:{}", identity.digest_hex());
        if let Some(existing) = self.replay(&request_id, &digest, actor.id()).await? {
            return Ok(existing.into());
        }
        let input = self.handler.prepare(request.kind, request.input, actor).await?;
        let envelope = Envelope {
            kind: request.kind,
            input,
            account: actor.account().into(),
            account_kind: actor.kind(),
        };
        let id = next_id();
        let source =
            self.save_private(&format!("data-transfer/{id}/source.enc"), &envelope, actor.id()).await?;
        let (job, items) =
            build_job(id, request_id, digest, source.base.id.clone(), request.kind, &envelope.input, actor)?;
        let context = ContextInput::new(
            &envelope,
            source.expires_at.ok_or_else(|| Error::Internal("任务输入有效期缺失".into()))?,
        )
        .record()?;
        match self.persist_submission(job, items, source, context, actor).await {
            Ok(view) => Ok(view),
            Err(error) => {
                if let Some(existing) = self
                    .replay(
                        &format!("transfer:{}", identity.digest_hex()),
                        &content_fingerprint(&encoded, self.codec.fingerprint_key()),
                        actor.id(),
                    )
                    .await?
                {
                    return Ok(existing.into());
                }
                Err(error)
            },
        }
    }
    async fn replay(&self, request_id: &str, digest: &str, actor: &str) -> Result<Option<BackgroundJob>> {
        let found = self.db.background_jobs().find_by_request_id(request_id, &mut NoTransaction).await?;
        if found.as_ref().is_some_and(|job| {
            job.requested_by != actor
                || job.domain_job_type.as_deref() != Some(DOMAIN)
                || job.domain_job_id.as_deref() != Some(digest)
        }) {
            return Err(Error::ConflictError("该操作号已用于其他内容，请核对原任务".into()));
        }
        Ok(found)
    }
    async fn persist_submission(
        &self,
        job: BackgroundJob,
        items: Vec<BackgroundJobItem>,
        source: FileAsset,
        context: BackgroundJobInput,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        let audit = submission_log(&job, actor)?;
        let db = self.db.clone();
        let copy = job.clone();
        db.client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.file_assets().create(&source, executor).await?;
                    db.bulk_job().create_job_with_items(&copy, items, executor).await?;
                    db.bulk_job()
                        .create_job_input(&BackgroundJobId::new(&copy.base.id), &context, executor)
                        .await?;
                    persist_log(&db, &audit, executor).await?;
                    Ok::<_, Error>(())
                })
            })
            .await?;
        Ok(job.into())
    }
}
fn submission_log(job: &BackgroundJob, actor: &AuditActor) -> Result<AuditLog> {
    Ok(actor.clone().resource_log("background_job.create", "background_job", job.base.id.clone())?)
}
fn validate_request(request: &TransferRequest) -> Result<()> {
    if request.request_id.trim().is_empty()
        || request.request_id.len() > 128
        || request.request_id.chars().any(char::is_control)
        || !request.input.is_object()
        || serde_json::to_vec(&request.input).map_or(true, |value| value.len() > 75 * 1024 * 1024)
    {
        return Err(Error::ValidationError("操作号或任务输入无效，单批参数不得超过 75 MiB".into()));
    }
    Ok(())
}
fn build_job(
    id: String,
    request: String,
    digest: String,
    asset: String,
    kind: TransferKind,
    input: &Value,
    actor: &AuditActor,
) -> Result<(BackgroundJob, Vec<BackgroundJobItem>)> {
    let drafts = row_drafts(kind, input);
    let total = drafts.as_ref().map_or(1, Vec::len);
    Ok(BackgroundJobAggregate::new(
        BackgroundJobId::new(&id),
        BackgroundJobAggregateData {
            job_no: format!("{} · DT-{id}", kind.label()),
            job_type: if kind.is_export() { JobType::Export } else { JobType::Import },
            domain_job_type: Some(DOMAIN.into()),
            domain_job_id: Some(digest.clone()),
            selection_snapshot_id: None,
            requested_by: actor.id().into(),
            request_id: request,
            input_file_asset_id: Some(asset.into()),
            result_file_asset_id: None,
            declared_total_count: u64::try_from(total)
                .map_err(|_| Error::ValidationError("任务行数超限".into()))?,
        },
        drafts.unwrap_or_else(|| {
            vec![BackgroundJobItemDraft {
                id: BackgroundJobItemId::new(next_id()),
                object_type: Some("data_transfer".into()),
                object_id: Some(id),
                expected_version: None,
                expected_hash: None,
                worksheet_name: None,
                source_row_no: None,
                source_column_name: None,
            }]
        }),
    )?
    .into_parts())
}

fn row_drafts(kind: TransferKind, input: &Value) -> Option<Vec<BackgroundJobItemDraft>> {
    let rows = if matches!(kind, TransferKind::SupplyBatch | TransferKind::PortalBatch) {
        input
            .get("rows")
            .or_else(|| input.get("request").and_then(|r| r.get("rows")))
            .and_then(Value::as_array)
    } else {
        None
    };
    rows.map(|rows| {
        rows.iter()
            .map(|row| BackgroundJobItemDraft {
                id: BackgroundJobItemId::new(next_id()),
                object_type: Some("data_transfer_row".into()),
                object_id: row["row_id"].as_str().map(row_identity),
                expected_version: None,
                expected_hash: None,
                worksheet_name: None,
                source_row_no: None,
                source_column_name: None,
            })
            .collect::<Vec<_>>()
    })
}

pub(super) fn row_identity(id: &str) -> String {
    hex::encode(Sha256::digest(id.as_bytes()))
}

#[cfg(test)]
pub(super) fn test_batch() -> (BackgroundJob, Vec<BackgroundJobItem>) {
    let actor = AuditActor::new("user".into(), "tester".into(), erp_core::AccountKind::Admin);
    build_job(
        "job".into(),
        "request".into(),
        "hash".into(),
        "file".into(),
        TransferKind::SupplyBatch,
        &serde_json::json!({"rows":[{"row_id":"r1"},{"row_id":"r2"}]}),
        &actor,
    )
    .unwrap()
}
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::data_transfer::TransferKind;
    #[test]
    fn task_registration_uses_a_registered_safe_audit_action() {
        let (job, _) = test_batch();
        let actor = AuditActor::new("user".into(), "tester".into(), erp_core::AccountKind::Admin);
        let log = submission_log(&job, &actor).unwrap();
        assert_eq!(log.action, "background_job.create");
        assert_eq!(log.resource_id.as_deref(), Some("job"));
        assert!(log.structured_event.is_some());
    }
    #[test]
    fn task_count_comes_from_rows_and_stable_identity_is_hashed() {
        let (job, items) = test_batch();
        assert_eq!(job.total_count, 2);
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].object_id.as_deref(), Some(row_identity("r1").as_str()));
        assert_ne!(row_identity("r1"), row_identity("r2"));
    }
    #[test]
    fn request_requires_stable_nonempty_identity_and_object() {
        let request = |id: &str, input| TransferRequest {
            request_id: id.into(),
            kind: TransferKind::SalesOrders,
            input,
        };
        assert!(validate_request(&request("", json!({}))).is_err());
        assert!(validate_request(&request("x", json!([]))).is_err());
        assert!(validate_request(&request("x", json!({"customer_id":"c"}))).is_ok());
    }
}
