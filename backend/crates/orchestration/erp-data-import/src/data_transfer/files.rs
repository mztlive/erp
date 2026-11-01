use std::time::Duration;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_support::{
    BackgroundJob, BackgroundJobView, BulkJobExt, FileAsset, FileAssetExt, FileAssetId, JobStatus,
    RegisterFileAssetRequest, RetentionClass, SensitivityClass, content_fingerprint,
};
use id_generator::next_id;
use persistence_core::NoTransaction;
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::time::timeout;

use super::submit::DOMAIN;
use super::types::Envelope;
use super::{DataTransferProcess, TransferResult};
use crate::{Error, Result};

impl DataTransferProcess {
    /// 查询本人任务，供业务页在无全局任务管理权限时恢复进度。
    /// # 参数
    /// 当前认证人及任务 ID。
    /// # 返回
    /// 本人文件任务状态。
    /// # 错误
    /// 越权或不存在时拒绝。
    pub async fn status(&self, id: &str, actor: &AuditActor) -> Result<BackgroundJobView> {
        let job = self
            .db
            .background_jobs()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务不存在".into()))?;
        if job.requested_by != actor.id() || job.domain_job_type.as_deref() != Some(DOMAIN) {
            return Err(Error::Forbidden("只能查询本人任务".into()));
        }
        Ok(job.into())
    }

    pub(super) async fn save_private<T: Serialize>(
        &self,
        key: &str,
        value: &T,
        actor: &str,
    ) -> Result<FileAsset> {
        let json = serde_json::to_string(value).map_err(|_| Error::Internal("任务内容编码失败".into()))?;
        if json.len() > 96 * 1024 * 1024 {
            return Err(Error::ValidationError("任务内容过大，请缩小处理范围".into()));
        }
        let encrypted = self.codec.encrypt(&json)?;
        timeout(
            Duration::from_secs(60),
            self.storage.save_with_content_type(
                key.to_string(),
                encrypted.as_bytes(),
                Some("application/octet-stream"),
            ),
        )
        .await
        .map_err(|_| Error::Internal("任务文件保存超时，请查询原任务".into()))?
        .map_err(|_| Error::Internal("任务文件保存失败，请查询原任务后重试".into()))?;
        let expires = Instant::now().unix_secs() + 7 * 86400;
        Ok(FileAsset::new(
            FileAssetId::new(next_id()),
            RegisterFileAssetRequest {
                storage_object_key: key.into(),
                file_name: "后台任务私有文件.enc".into(),
                content_type: "application/octet-stream".into(),
                byte_size: u64::try_from(encrypted.len())
                    .map_err(|_| Error::Internal("文件大小溢出".into()))?,
                content_hmac: content_fingerprint(&json, self.codec.fingerprint_key()),
                sensitivity_class: SensitivityClass::HighlySensitive,
                retention_class: RetentionClass::SevenDays,
                expires_at: Some(
                    u64::try_from(expires).map_err(|_| Error::Internal("文件有效期非法".into()))?,
                ),
            }
            .into_data(actor)?,
        )?)
    }
    pub(super) async fn read_private<T: DeserializeOwned>(&self, id: &str, owner: &str) -> Result<T> {
        let asset = self
            .db
            .file_assets()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("任务文件不存在".into()))?;
        if asset.created_by != owner
            || asset.destroyed_at.is_some()
            || asset.expires_at.is_none_or(|at| at <= Instant::now())
        {
            return Err(Error::Forbidden("任务文件已过期或无权读取，请重新提交".into()));
        }
        let bytes = timeout(Duration::from_secs(60), self.storage.read(&asset.storage_object_key))
            .await
            .map_err(|_| Error::Internal("任务文件读取超时，请重试".into()))?
            .map_err(|_| Error::Internal("任务文件读取失败，请重试".into()))?;
        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Internal("任务文件损坏".into()))?;
        let json = self.codec.decrypt(text)?;
        if content_fingerprint(&json, self.codec.fingerprint_key()) != asset.content_hmac.as_str() {
            return Err(Error::Internal("任务文件校验失败，请重新提交".into()));
        }
        serde_json::from_str(&json).map_err(|_| Error::Internal("任务文件格式无效".into()))
    }
    pub(super) async fn source(&self, job: &BackgroundJob) -> Result<Envelope> {
        let id = job.input_file_asset_id.as_ref().ok_or_else(|| Error::NotFound("任务输入缺失".into()))?;
        self.read_private(id.as_ref(), &job.requested_by).await
    }
    /// 返回当前身份仍获授权且未过期的结果；导出允许非原提交人下载。
    /// # 参数
    /// 后台任务 ID 和当前认证人。
    /// # 返回
    /// 受控文件或导入预览结果。
    /// # 错误
    /// 非本人读取导入、未完成、过期、撤权、范围变化或文件不可用时拒绝。
    pub async fn result(&self, id: &str, actor: &AuditActor) -> Result<TransferResult> {
        let job = self.transfer_job(id).await?;
        if !matches!(job.status, JobStatus::Succeeded | JobStatus::Failed | JobStatus::PartiallySucceeded)
            || job.finished_at.is_none()
            || job.result_expires_at.is_none_or(|at| at <= Instant::now())
        {
            return Err(Error::BusinessLogicError("任务尚未完成或结果已过期，请查询原任务".into()));
        }
        let source = self.source(&job).await?;
        source.kind.authorize_reader(&job.requested_by, actor)?;
        let asset =
            job.result_file_asset_id.as_ref().ok_or_else(|| Error::NotFound("任务结果不存在".into()))?;

        let mut result: TransferResult = self.read_private(asset.as_ref(), &job.requested_by).await?;
        self.handler.authorize_result(source.kind, &source.input, &result.authorization, actor).await?;
        if !result.metadata.is_object() {
            result.metadata = json!({});
        }
        result.metadata["completed_at"] = json!(job.finished_at);
        result.metadata["expires_at"] = json!(job.result_expires_at);
        let decoded = if result.content_type == "application/json" {
            Some(
                serde_json::from_slice(&result.bytes()?)
                    .map_err(|_| Error::Internal("任务结果格式无效".into()))?,
            )
        } else {
            None
        };
        let compact = self.context_input(&job).await?;
        let mut context = self.handler.context(compact.kind, &compact.input, decoded.as_ref(), actor).await?;
        if job.requested_by != actor.id() {
            context.retain_classification();
        }
        result.metadata["kind"] = json!(source.kind);
        result.metadata["context"] = context.context;
        Ok(result)
    }
}
