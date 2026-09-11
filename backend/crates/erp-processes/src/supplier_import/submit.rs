//! 登记加密输入和后台任务；相同请求只能重放同一载荷。
use super::{SupplierImportProcess, SUPPLIER_IMPORT_DOMAIN};
use crate::{Error, Result};
use application_core::{command::CommandFingerprint, AuditActor};
use erp_supplier::dto::import_job::SupplierImportJobRequest;
use erp_support::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobId, BackgroundJobItem,
    BackgroundJobItemDraft, BackgroundJobItemId, BackgroundJobRegistration, BackgroundJobView, BulkJobExt,
    JobType,
};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};

impl SupplierImportProcess {
    /// 加密保存输入并原子登记任务及行明细，不执行供应商写入。
    ///
    /// 返回新任务或同载荷重放；参数、存储、事务错误及异载荷冲突向调用方返回。
    pub async fn submit(
        &self,
        request: SupplierImportJobRequest,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        request.validate()?;
        let json = serde_json::to_string(&request).map_err(|_| Error::Internal("导入内容编码失败".into()))?;
        if json.len() > 10 * 1024 * 1024 {
            return Err(Error::ValidationError("导入内容不能超过 10 MB".into()));
        }
        let source = CommandFingerprint::from_parts([
            hex::encode(self.codec.fingerprint_key()),
            actor.id().to_string(),
            json.clone(),
        ])
        .digest_hex()
        .to_string();
        let (job, items) = build_job(&request, actor.id(), &source)?;
        if let Some(existing) = self.replay(&job).await? {
            return Ok(existing.into());
        }
        let encrypted = self.codec.encrypt(&json)?;
        self.storage
            .save_with_content_type(
                source_key(&source),
                encrypted.as_bytes(),
                Some("application/octet-stream"),
            )
            .await
            .map_err(|_| Error::Internal("保存供应商导入文件失败，请重试".into()))?;
        self.persist(job, items).await
    }

    /// 查询原提交并验证操作者与完整内容；旧记录无指纹时拒绝重放。
    async fn replay(&self, proposed: &BackgroundJob) -> Result<Option<BackgroundJob>> {
        let existing = self
            .db
            .background_jobs()
            .find_by_request_id(&proposed.request_id, &mut NoTransaction)
            .await?;
        if let Some(job) = &existing {
            ensure_replay(job, proposed)?;
        }
        Ok(existing)
    }

    /// 任务与行明细在同一事务内落库；唯一竞争和未知提交结果回读原任务。
    async fn persist(&self, job: BackgroundJob, items: Vec<BackgroundJobItem>) -> Result<BackgroundJobView> {
        let db = self.db.clone();
        let copy = job.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |session| {
                Box::pin(async move { db.bulk_job().create_job_with_items(&copy, items, session).await })
            })
            .await;
        match result {
            Ok(BackgroundJobRegistration::Created) => Ok(job.into()),
            Ok(BackgroundJobRegistration::ReplaySame(existing)) => {
                ensure_replay(&existing, &job)?;
                Ok(existing.into())
            }
            Ok(BackgroundJobRegistration::ConflictDifferentPayload(_)) => Err(conflict()),
            Err(error) => {
                if let Some(existing) = self.replay(&job).await? {
                    return Ok(existing.into());
                }
                Err(error.into())
            }
        }
    }
}

/// 加密对象路径仅使用服务端生成的摘要，不接受客户端路径。
pub(super) fn source_key(source: &str) -> String {
    format!("supplier-import/{source}.enc")
}

/// 构造有界任务聚合；源内容摘要参与父子指纹，覆盖全部模板字段。
pub(super) fn build_job(
    request: &SupplierImportJobRequest,
    actor: &str,
    source: &str,
) -> Result<(BackgroundJob, Vec<BackgroundJobItem>)> {
    let drafts = request
        .rows
        .iter()
        .map(|row| BackgroundJobItemDraft {
            id: BackgroundJobItemId::new(next_id()),
            object_type: Some("supplier_import_row".into()),
            object_id: Some(row.cell(1).chars().take(30).collect::<String>())
                .filter(|s| !s.is_empty())
                .or(Some("未填写供应商名称".into())),
            expected_version: None,
            expected_hash: None,
            worksheet_name: Some(request.file_name.chars().take(30).collect()),
            source_row_no: Some(row.row_number),
            source_column_name: None,
        })
        .collect();
    Ok(BackgroundJobAggregate::new(
        BackgroundJobId::new(next_id()),
        BackgroundJobAggregateData {
            job_no: format!(
                "SI-{}",
                CommandFingerprint::from_parts([actor.to_string(), request.request_id.clone()]).digest_hex()
            ),
            job_type: JobType::Import,
            domain_job_type: Some(SUPPLIER_IMPORT_DOMAIN.into()),
            domain_job_id: Some(source.into()),
            selection_snapshot_id: None,
            requested_by: actor.into(),
            request_id: format!(
                "supplier-import:{}",
                CommandFingerprint::from_parts([actor.into(), request.request_id.clone()]).digest_hex()
            ),
            input_file_asset_id: None,
            result_file_asset_id: None,
            declared_total_count: request.rows.len() as u64,
        },
        drafts,
    )?
    .into_parts())
}

/// 同一提交身份不得切换操作者、业务类型或源数据。
pub(super) fn ensure_replay(existing: &BackgroundJob, proposed: &BackgroundJob) -> Result<()> {
    if existing.requested_by != proposed.requested_by
        || existing.domain_job_type != proposed.domain_job_type
        || existing.request_fingerprint.is_none()
        || existing.request_fingerprint != proposed.request_fingerprint
    {
        return Err(conflict());
    }
    Ok(())
}
fn conflict() -> Error {
    Error::ConflictError("该提交已用于其他导入内容，请重新选择文件".into())
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use erp_core::common::time::BusinessDate;
    use erp_supplier::dto::import::SupplierImportRow;
    pub(crate) fn fixture() -> (BackgroundJob, Vec<BackgroundJobItem>) {
        let request = SupplierImportJobRequest {
            request_id: "req-1".into(),
            file_name: "供应商.xlsx".into(),
            rows: vec![SupplierImportRow {
                row_number: 9,
                cells: vec!["供应商".into(); 23],
                party_no: "PTY-1".into(),
                supplier_no: "SUP-1".into(),
                effective_from: BusinessDate::from_ymd(2026, 9, 11).unwrap(),
                parse_errors: vec![],
            }],
        };
        build_job(&request, "actor-1", &"a".repeat(64)).unwrap()
    }
    #[test]
    fn creates_pending_items_and_replays_only_identical_payload() {
        let (job, items) = fixture();
        let (mut same, _) = fixture();
        assert_ne!(job.base.id, same.base.id);
        assert!(ensure_replay(&job, &same).is_ok());
        assert_eq!(items[0].source_row_no, Some(9));
        assert_eq!(job.total_count, 1);
        assert!(items[0].status.is_none());
        same.request_fingerprint = None;
        assert!(ensure_replay(&same, &job).is_err());
        let (mut changed, _) = fixture();
        changed.requested_by = "other".into();
        assert!(ensure_replay(&changed, &job).is_err());
        changed = fixture().0;
        changed.request_fingerprint = Some(CommandFingerprint::from_parts(["changed source".into()]));
        assert!(ensure_replay(&changed, &job).is_err());
    }
}
