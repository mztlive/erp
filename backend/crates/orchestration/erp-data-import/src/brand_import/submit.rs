//! 登记加密输入和后台任务；相同请求只能重放同一载荷。
use application_core::AuditActor;
use application_core::command::CommandFingerprint;
use erp_catalog::dto::BrandImportJobRequest;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobId, BackgroundJobItem,
    BackgroundJobItemDraft, BackgroundJobItemId, BackgroundJobRegistration, BackgroundJobView, BulkJobExt,
    JobType,
};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};

use super::{BRAND_IMPORT_DOMAIN, BrandImportProcess, job_actor};
use crate::{Error, Result};

impl BrandImportProcess {
    /// 加密保存输入并原子登记任务及行明细，不执行品牌写入。
    ///
    /// # 参数
    /// * `request` - 已解析的导入请求。
    /// * `actor` - 提交人，参与源内容指纹。
    ///
    /// # 返回
    /// 返回新建任务；同一提交身份且载荷相同则返回已有任务。
    ///
    /// # 错误
    /// 请求校验失败、内容超过 5 MB、编码或保存失败、事务失败，或同一提交身份对应不同载荷时返回错误。
    pub async fn submit(
        &self,
        request: BrandImportJobRequest,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        request.key()?;
        self.authorize(actor, &mut NoTransaction).await?;
        let json = serde_json::to_string(&request).map_err(|_| Error::Internal("导入内容编码失败".into()))?;
        if json.len() > 5 * 1024 * 1024 {
            return Err(Error::ValidationError("导入内容不能超过 5 MB".into()));
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
            .map_err(|_| Error::Internal("保存品牌导入文件失败，请重试".into()))?;
        self.persist(job, items).await
    }

    /// 查询原提交并验证操作者与完整内容；旧记录无指纹时拒绝重放。
    async fn replay(&self, proposed: &BackgroundJob) -> Result<Option<BackgroundJob>> {
        let existing =
            self.db.background_jobs().find_by_request_id(&proposed.request_id, &mut NoTransaction).await?;
        if let Some(job) = &existing {
            ensure_replay(job, proposed)?;
        }
        Ok(existing)
    }

    /// 任务与行明细在同一事务内落库；唯一竞争和未知提交结果回读原任务。
    async fn persist(&self, job: BackgroundJob, items: Vec<BackgroundJobItem>) -> Result<BackgroundJobView> {
        let db = self.db.clone();
        let copy = job.clone();
        let process = self.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.authorize(&job_actor(&copy), executor).await?;
                    Ok::<_, Error>(db.bulk_job().create_job_with_items(&copy, items, executor).await?)
                })
            })
            .await;
        match result {
            Ok(BackgroundJobRegistration::Created) => Ok(job.into()),
            Ok(BackgroundJobRegistration::ReplaySame(existing)) => {
                ensure_replay(&existing, &job)?;
                Ok(existing.into())
            },
            Ok(BackgroundJobRegistration::ConflictDifferentPayload(_)) => Err(conflict()),
            Err(error @ (Error::Forbidden(_) | Error::Unauthenticated(_))) => Err(error),
            Err(error) => {
                if let Some(existing) = self.replay(&job).await? {
                    return Ok(existing.into());
                }
                Err(error)
            },
        }
    }
}

/// 用服务端生成的摘要组成加密对象路径，不接受客户端路径。
///
/// # 参数
/// * `source` - 源内容摘要的十六进制文本。
///
/// # 返回
/// 返回 `brand-import/{source}.enc`。
///
/// # 错误
/// 不返回错误。
pub(super) fn source_key(source: &str) -> String {
    format!("brand-import/{source}.enc")
}

/// 构造有界任务聚合；源内容摘要写入任务身份，行明细覆盖全部模板行。
///
/// # 参数
/// * `request` - 已校验的导入请求。
/// * `actor` - 提交人账号。
/// * `source` - 源内容摘要。
///
/// # 返回
/// 返回待落库的任务和行明细。
///
/// # 错误
/// 任务聚合构造失败时返回错误。
pub(super) fn build_job(
    request: &BrandImportJobRequest,
    actor: &str,
    source: &str,
) -> Result<(BackgroundJob, Vec<BackgroundJobItem>)> {
    let drafts = request
        .rows
        .iter()
        .map(|row| BackgroundJobItemDraft {
            id: BackgroundJobItemId::new(next_id()),
            object_type: Some("brand_import_row".into()),
            object_id: Some(row.name.trim().chars().take(30).collect::<String>())
                .filter(|s| !s.is_empty())
                .or(Some("未填写品牌名称".into())),
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
                "BI-{}",
                CommandFingerprint::from_parts([actor.to_string(), request.request_id.clone()]).digest_hex()
            ),
            job_type: JobType::Import,
            domain_job_type: Some(BRAND_IMPORT_DOMAIN.into()),
            domain_job_id: Some(source.into()),
            selection_snapshot_id: None,
            requested_by: actor.into(),
            request_id: format!(
                "brand-import:{}",
                CommandFingerprint::from_parts([actor.into(), request.request_id.clone()]).digest_hex()
            ),
            input_file_asset_id: None,
            result_file_asset_id: None,
            declared_total_count: u64::try_from(request.rows.len())
                .map_err(|_| Error::ValidationError("导入行数超限".into()))?,
        },
        drafts,
    )?
    .into_parts())
}

/// 同一提交身份不得切换操作者、业务类型或源数据；旧记录没有指纹时也拒绝。
///
/// # 参数
/// * `existing` - 已落库的任务。
/// * `proposed` - 本次请求构造的任务。
///
/// # 返回
/// 操作者、业务类型和请求指纹都一致时返回。
///
/// # 错误
/// 任一字段不一致或已有任务没有指纹时返回冲突。
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
    use erp_catalog::dto::BrandImportRow;

    use super::*;
    pub(crate) fn request() -> BrandImportJobRequest {
        BrandImportJobRequest {
            request_id: "12345678-1234-4234-8234-123456789abc".into(),
            file_name: "品牌.xlsx".into(),
            rows: vec![BrandImportRow {
                row_number: 9,
                name: "品牌".into(),
                brand_code: String::new(),
                error: None,
            }],
        }
    }
    pub(crate) fn fixture() -> (BackgroundJob, Vec<BackgroundJobItem>) {
        build_job(&request(), "actor-1", &"a".repeat(64)).unwrap()
    }
    #[test]
    fn persists_all_rows_and_replays_only_identical_identity_and_source() {
        let (job, items) = fixture();
        let (mut same, _) = fixture();
        assert_ne!(job.base.id, same.base.id);
        assert!(ensure_replay(&job, &same).is_ok());
        assert_eq!(items[0].source_row_no, Some(9));
        assert_eq!(items[0].object_type.as_deref(), Some("brand_import_row"));
        assert_eq!(job.total_count, 1);
        assert!(items[0].status.is_none());
        same.request_fingerprint = None;
        assert!(ensure_replay(&same, &job).is_err());
        let (mut changed, _) = fixture();
        changed.requested_by = "other".into();
        assert!(ensure_replay(&changed, &job).is_err());
        let (different, _) = build_job(&request(), "actor-1", &"b".repeat(64)).unwrap();
        assert!(ensure_replay(&different, &job).is_err());
        let mut with_errors = request();
        with_errors.rows[0].error = Some("公式需转成文本".into());
        let (_, items) = build_job(&with_errors, "actor-1", &"c".repeat(64)).unwrap();
        assert_eq!(items.len(), 1);
    }
}
