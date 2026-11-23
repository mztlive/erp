//! 加密输入、原子任务登记及按结果读取原始行。
use application_core::AuditActor;
use application_core::command::CommandFingerprint;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobId, BackgroundJobItem,
    BackgroundJobItemDraft, BackgroundJobItemId, BackgroundJobRegistration, BackgroundJobView, BulkJobExt,
    ItemStatusFilter, JobType,
};
use id_generator::next_id;
use persistence_core::Transactional;

use super::{
    CustomerImportProcess, CustomerImportRequest, CustomerImportResultRow, CustomerImportRow, DOMAIN,
    MAX_SOURCE_BYTES,
};
use crate::{Error, Result};

impl CustomerImportProcess {
    /// 加密保存来源，重验创建资格后原子登记任务和行。同一提交编号只接受原输入。
    pub async fn submit(
        &self,
        request: CustomerImportRequest,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        request.validate()?;
        let json = serde_json::to_string(&request).map_err(|_| Error::Internal("导入内容编码失败".into()))?;
        if json.len() > MAX_SOURCE_BYTES {
            return Err(Error::ValidationError("导入内容超过 5 MB，请拆分文件".into()));
        }

        let digest = self.source_digest(actor.id(), &json);
        let (job, items) = build_job(&request, actor.id(), &digest)?;
        if let Some(existing) = self.replay(&job, actor).await? {
            return Ok(existing.into());
        }

        let encrypted = self.codec.encrypt(&json)?;
        self.storage
            .save_with_content_type(
                source_key(&digest),
                encrypted.as_bytes(),
                Some("application/octet-stream"),
            )
            .await
            .map_err(|_| Error::Internal("保存客户导入文件失败，请重试".into()))?;

        let db = self.db.clone();
        let operator = actor.clone();
        let proposed = job.clone();
        let profile = self.profile.clone();
        let result = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    profile.require_create(&operator, executor).await?;
                    Ok::<_, Error>(db.bulk_job().create_job_with_items(&proposed, items, executor).await?)
                })
            })
            .await;

        match result {
            Ok(BackgroundJobRegistration::Created) => Ok(job.into()),
            Ok(BackgroundJobRegistration::ReplaySame(existing)) => Ok(check_replay(existing, &job)?.into()),
            Ok(BackgroundJobRegistration::ConflictDifferentPayload(_)) => Err(conflict()),
            Err(error @ (Error::Forbidden(_) | Error::Unauthenticated(_))) => Err(error),
            Err(error) => match self.replay(&job, actor).await? {
                Some(existing) => Ok(existing.into()),
                None => Err(error),
            },
        }
    }

    async fn replay(&self, proposed: &BackgroundJob, actor: &AuditActor) -> Result<Option<BackgroundJob>> {
        let request_id = proposed.request_id.clone();
        let proposed = proposed.clone();
        let db = self.db.clone();
        let profile = self.profile.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    profile.require_create(&actor, executor).await?;
                    db.background_jobs()
                        .find_by_request_id(&request_id, executor)
                        .await?
                        .map(|job| check_replay(job, &proposed))
                        .transpose()
                })
            })
            .await
    }

    /// 返回已结束任务中所选分类的全部原始行。交付前重验原提交人与当前创建权限。
    pub async fn result_rows(
        &self,
        id: &str,
        status: ItemStatusFilter,
        actor: &AuditActor,
    ) -> Result<Vec<CustomerImportResultRow>> {
        let job = self.access.snapshot(id, actor).await?;
        ensure_finished(&job)?;
        let (source, _) = self.read_source(&job).await?;
        let access = self.access.clone();
        let db = self.db.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let current = access.reload(&job, &actor, executor).await?;
                    ensure_finished(&current)?;
                    let items = db
                        .background_job_items()
                        .list_entities_by_job(&BackgroundJobId::new(&current.base.id), executor)
                        .await?;
                    result_rows(&source, items, status)
                })
            })
            .await
    }

    pub(super) async fn read_source(&self, job: &BackgroundJob) -> Result<(CustomerImportRequest, usize)> {
        let digest = job
            .domain_job_id
            .as_deref()
            .filter(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .ok_or_else(|| Error::Internal("导入任务缺少源数据".into()))?;
        let bytes = self
            .storage
            .read(source_key(digest))
            .await
            .map_err(|_| Error::Internal("读取客户导入源文件失败".into()))?;
        let encrypted = std::str::from_utf8(&bytes).map_err(|_| Error::Internal("导入源数据损坏".into()))?;
        let json = self.codec.decrypt(encrypted)?;
        if json.len() > MAX_SOURCE_BYTES || self.source_digest(&job.requested_by, &json) != digest {
            return Err(Error::Internal("导入源数据与任务不一致".into()));
        }
        let request: CustomerImportRequest =
            serde_json::from_str(&json).map_err(|_| Error::Internal("导入源数据损坏".into()))?;
        request.validate()?;
        if job.request_id != request_key(&request, &job.requested_by)
            || u64::try_from(request.rows.len()).ok() != Some(job.total_count)
        {
            return Err(Error::Internal("导入源数据与任务不一致".into()));
        }
        Ok((request, json.len()))
    }

    fn source_digest(&self, actor: &str, json: &str) -> String {
        CommandFingerprint::from_parts([
            hex::encode(self.codec.fingerprint_key()),
            DOMAIN.into(),
            actor.into(),
            json.into(),
        ])
        .digest_hex()
        .to_string()
    }
}

pub(super) fn source_row<'a>(
    source: &'a CustomerImportRequest,
    item: &BackgroundJobItem,
) -> Result<&'a CustomerImportRow> {
    let index = item.item_no.checked_sub(1).and_then(|n| usize::try_from(n).ok());
    index
        .and_then(|index| source.rows.get(index))
        .filter(|row| Some(row.row_number) == item.source_row_no)
        .ok_or_else(|| Error::Internal("导入源数据行号与任务不一致".into()))
}

fn result_rows(
    source: &CustomerImportRequest,
    items: Vec<BackgroundJobItem>,
    status: ItemStatusFilter,
) -> Result<Vec<CustomerImportResultRow>> {
    items
        .into_iter()
        .filter(|item| item.status == status.status())
        .map(|item| {
            Ok(CustomerImportResultRow {
                row: source_row(source, &item)?.clone(),
                message: item.result_summary.unwrap_or_else(|| "尚未处理".into()),
            })
        })
        .collect()
}

fn ensure_finished(job: &BackgroundJob) -> Result<()> {
    if job.finished_at.is_none() {
        return Err(Error::ConflictError("请等待任务结束后下载结果".into()));
    }
    Ok(())
}

fn source_key(digest: &str) -> String {
    format!("customer-import/{digest}.enc")
}

fn request_identity(request: &CustomerImportRequest, actor: &str) -> CommandFingerprint {
    CommandFingerprint::from_parts([actor.into(), request.request_id.clone()])
}

fn request_key(request: &CustomerImportRequest, actor: &str) -> String {
    format!("customer-import:{}", request_identity(request, actor).digest_hex())
}

fn build_job(
    source: &CustomerImportRequest,
    actor: &str,
    digest: &str,
) -> Result<(BackgroundJob, Vec<BackgroundJobItem>)> {
    let items = source
        .rows
        .iter()
        .map(|row| BackgroundJobItemDraft {
            id: BackgroundJobItemId::new(next_id()),
            object_type: Some("customer_import_row".into()),
            object_id: Some(row.legal_name.trim().chars().take(100).collect::<String>())
                .filter(|value| !value.is_empty())
                .or(Some("未填写客户名称".into())),
            expected_version: None,
            expected_hash: None,
            worksheet_name: None,
            source_row_no: Some(row.row_number),
            source_column_name: None,
        })
        .collect();
    Ok(BackgroundJobAggregate::new(
        BackgroundJobId::new(next_id()),
        BackgroundJobAggregateData {
            job_no: format!("CI-{}", request_identity(source, actor).digest_hex()),
            job_type: JobType::Import,
            domain_job_type: Some(DOMAIN.into()),
            domain_job_id: Some(digest.into()),
            selection_snapshot_id: None,
            requested_by: actor.into(),
            request_id: request_key(source, actor),
            input_file_asset_id: None,
            result_file_asset_id: None,
            declared_total_count: u64::try_from(source.rows.len())
                .map_err(|_| Error::ValidationError("导入行数超限".into()))?,
        },
        items,
    )?
    .into_parts())
}

fn check_replay(existing: BackgroundJob, proposed: &BackgroundJob) -> Result<BackgroundJob> {
    if existing.requested_by != proposed.requested_by
        || existing.domain_job_type != proposed.domain_job_type
        || existing.request_fingerprint.is_none()
        || existing.request_fingerprint != proposed.request_fingerprint
    {
        return Err(conflict());
    }
    Ok(existing)
}

fn conflict() -> Error {
    Error::ConflictError("该提交已用于其他导入内容，请重新选择文件".into())
}

#[cfg(test)]
mod tests {
    use erp_support::ItemStatus;
    use serde_json::json;

    use super::*;

    #[test]
    fn replay_is_bound_to_submitter_and_source() {
        let source = input();
        let (first, _) = build_job(&source, "sales-a", "digest").unwrap();
        let (replay, _) = build_job(&source, "sales-a", "digest").unwrap();
        let (changed, _) = build_job(&source, "sales-a", "different").unwrap();
        let (other, _) = build_job(&source, "sales-b", "digest").unwrap();
        assert_eq!(first.request_id, changed.request_id);
        assert_ne!(first.request_id, other.request_id);
        assert!(check_replay(first.clone(), &replay).is_ok());
        assert!(check_replay(first, &changed).is_err());
    }

    #[test]
    fn export_keeps_original_row_coordinates_and_selected_status() {
        let source = input();
        let (_, mut items) = build_job(&source, "sales-a", "digest").unwrap();
        items[1]
            .record_result(
                ItemStatus::Failed,
                Some("import_failed".into()),
                Some("已有主体".into()),
                None,
                None,
            )
            .unwrap();
        let failed = result_rows(&source, items.clone(), ItemStatusFilter::Failed).unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].row.row_number, 18);
        assert_eq!(failed[0].row.unified_credit_code.as_deref(), Some("001234567890123456"));
        assert_eq!(failed[0].message, "已有主体");
        let unprocessed = result_rows(&source, items.clone(), ItemStatusFilter::Unprocessed).unwrap();
        assert_eq!(unprocessed.len(), 1);
        assert_eq!(unprocessed[0].row.row_number, 7);
        items[0].source_row_no = Some(9);
        assert!(result_rows(&source, items, ItemStatusFilter::Unprocessed).is_err());
    }

    fn input() -> CustomerImportRequest {
        serde_json::from_value(json!({
            "request_id":"original", "file_name":"客户.xlsx", "rows":[
                {"row_number":7, "legal_name":"新企业"},
                {"row_number":18, "legal_name":"已有企业", "unified_credit_code":"001234567890123456"}
            ]
        }))
        .unwrap()
    }
}
