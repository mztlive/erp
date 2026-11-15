//! 登记加密输入和后台任务；相同请求只能重放同一载荷。
use application_core::AuditActor;
use application_core::command::CommandFingerprint;
use erp_catalog::dto::BrandImportJobRequest;
use erp_identity::AuthorizationService;
use erp_support::repository::prelude::*;
use erp_support::{
    BackgroundJob, BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobId, BackgroundJobItem,
    BackgroundJobItemDraft, BackgroundJobItemId, BackgroundJobRegistration, BackgroundJobView, BulkJobExt,
    JobType,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use super::{BRAND_IMPORT_DOMAIN, BrandImportProcess, authorize};
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
        if let Some(existing) = self.replay(&job, actor).await? {
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
        self.persist(job, items, actor).await
    }

    /// 在同一快照内先证明当前入口资格，再读取并核验原提交。
    async fn replay(&self, proposed: &BackgroundJob, actor: &AuditActor) -> Result<Option<BackgroundJob>> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let proposed = proposed.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    replay_registration(&db, &authorization, &actor, &proposed, executor).await
                })
            })
            .await
    }

    /// 任务与行明细在同一事务内落库；唯一竞争和未知提交结果回读原任务。
    async fn persist(
        &self,
        job: BackgroundJob,
        items: Vec<BackgroundJobItem>,
        actor: &AuditActor,
    ) -> Result<BackgroundJobView> {
        let db = self.db.clone();
        let copy = job.clone();
        let authorization = self.authorization.clone();
        let operator = actor.clone();
        let result = db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorize(&authorization, &operator, executor).await?;
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
                if let Some(existing) = self.replay(&job, actor).await? {
                    return Ok(existing.into());
                }
                Err(error)
            },
        }
    }
}

/// 首次回放与提交结果未知后的查证使用相同授权及载荷校验。
async fn replay_registration(
    db: &Database,
    authorization: &AuthorizationService,
    actor: &AuditActor,
    proposed: &BackgroundJob,
    executor: &mut dyn Executor,
) -> Result<Option<BackgroundJob>> {
    authorize(authorization, actor, executor).await?;
    let existing = db.background_jobs().find_by_request_id(&proposed.request_id, executor).await?;
    checked_replay(existing, proposed)
}

/// 返回经过原提交身份和完整输入指纹校验的既有任务，保留其最新状态。
fn checked_replay(
    existing: Option<BackgroundJob>,
    proposed: &BackgroundJob,
) -> Result<Option<BackgroundJob>> {
    if let Some(job) = &existing {
        ensure_replay(job, proposed)?;
    }
    Ok(existing)
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
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

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

    /// 回放交付已有任务的真实状态及版本，不使用本次候选任务覆盖原结果。
    #[test]
    fn replay_returns_existing_job_state_and_rejects_another_payload_for_the_same_request() {
        let (mut existing, _) = fixture();
        existing.cancel(Instant::from_unix_secs(100)).unwrap();
        existing.base.version = 7;
        let (proposed, _) = fixture();
        assert_eq!(existing.request_id, proposed.request_id);
        assert_ne!(existing.base.id, proposed.base.id);
        let replayed = checked_replay(Some(existing.clone()), &proposed).unwrap().unwrap();
        assert_eq!(replayed.base.id, existing.base.id);
        assert_eq!(replayed.base.version, 7);
        assert_eq!(replayed.status, existing.status);
        assert_eq!(replayed.finished_at, existing.finished_at);
        assert!(checked_replay(None, &proposed).unwrap().is_none());

        let (different, _) = build_job(&request(), "actor-1", &"b".repeat(64)).unwrap();
        assert_eq!(existing.request_id, different.request_id);
        let error = checked_replay(Some(existing), &different).unwrap_err();
        assert!(matches!(error, Error::ConflictError(reason)
            if reason == "该提交已用于其他导入内容，请重新选择文件"));
    }

    /// 身份取证失败必须在读取原回执前结束；非快照执行器不能用于该回放。
    #[tokio::test]
    async fn replay_authorizes_in_the_original_snapshot_before_reading_any_job() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("brand_import_replay_authorization");
        let authorization = unavailable_authorization();
        let actor = AuditActor::new("actor-1".into(), "operator".into(), AccountKind::Admin);
        let (proposed, _) = fixture();
        let error =
            replay_registration(&db, &authorization, &actor, &proposed, &mut SnapshotExecutor::default())
                .await
                .expect_err("授权设施失败时不得读取回执");
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
        let error = replay_registration(&db, &authorization, &actor, &proposed, &mut ReadExecutor::default())
            .await
            .expect_err("回放必须使用同一快照授权和读取");
        assert!(matches!(error, Error::ValidationError(reason) if reason == "写入授权必须使用快照事务"));
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
