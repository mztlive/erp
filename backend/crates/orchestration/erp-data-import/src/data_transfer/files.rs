use std::time::Duration;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_support::{
    BackgroundJob, BackgroundJobView, FileAsset, FileAssetExt, FileAssetId, JobStatus,
    RegisterFileAssetRequest, RetentionClass, SensitivityClass, content_fingerprint,
};
use id_generator::next_id;
use persistence_core::{Executor, ReadExecutor, Transactional};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::json;
use tokio::time::timeout;

use super::access::same_job;
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
        Ok(self.owned_job(id, actor).await?.into())
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
    /// 后台执行使用的私有文件读取；外部入口通过 reader_private 完成资格检查。
    /// # 参数
    /// id、owner 为执行任务绑定的文件及原提交人。
    /// # 返回
    /// 完整性检查通过的私有内容。
    /// # 错误
    /// 归属、有效期、存储、密文或内容格式无效时拒绝。
    pub(super) async fn read_private<T: DeserializeOwned>(&self, id: &str, owner: &str) -> Result<T> {
        let asset = self.private_asset(id, owner, &mut ReadExecutor::default()).await?;
        self.decrypt_private(&asset).await
    }

    async fn private_asset(&self, id: &str, owner: &str, executor: &mut dyn Executor) -> Result<FileAsset> {
        let asset = self
            .db
            .file_assets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("任务文件不存在".into()))?;
        if asset.created_by != owner
            || asset.destroyed_at.is_some()
            || asset.expires_at.is_none_or(|at| at <= Instant::now())
        {
            return Err(Error::Forbidden("任务文件已过期或无权读取，请重新提交".into()));
        }
        Ok(asset)
    }

    async fn decrypt_private<T: DeserializeOwned>(&self, asset: &FileAsset) -> Result<T> {
        let bytes = timeout(Duration::from_secs(60), self.storage.read(&asset.storage_object_key))
            .await
            .map_err(|_| Error::Internal("任务文件读取超时，请重试".into()))?
            .map_err(|_| Error::Internal("任务文件读取失败，请重试".into()))?;
        if asset.expires_at.is_none_or(|at| at <= Instant::now()) {
            return Err(Error::Forbidden("任务文件已过期或无权读取，请重新提交".into()));
        }

        let text = std::str::from_utf8(&bytes).map_err(|_| Error::Internal("任务文件损坏".into()))?;
        let json = self.codec.decrypt(text)?;
        if content_fingerprint(&json, self.codec.fingerprint_key()) != asset.content_hmac.as_str() {
            return Err(Error::Internal("任务文件校验失败，请重新提交".into()));
        }
        serde_json::from_str(&json).map_err(|_| Error::Internal("任务文件格式无效".into()))
    }

    /// 文件元数据与当前任务资格共用快照；外部下载解密在提交后执行。
    /// # 参数
    /// job 为原任务，id 为其私有文件，actor 为本次读取身份。
    /// # 返回
    /// 已解密内容及同快照文件元数据。
    /// # 错误
    /// 当前资格失效、任务变化、过期或私有文件不可用时拒绝。
    pub(super) async fn reader_private<T: DeserializeOwned>(
        &self,
        job: &BackgroundJob,
        id: &str,
        actor: &AuditActor,
    ) -> Result<(T, FileAsset)> {
        let process = self.clone();
        let job = job.clone();
        let id = id.to_owned();
        let actor = actor.clone();
        let asset = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let current = process.reader(&job.base.id, &actor, true, executor).await?;
                    same_job(&job, &current.job)?;
                    process.private_asset(&id, &job.requested_by, executor).await
                })
            })
            .await?;

        Ok((self.decrypt_private(&asset).await?, asset))
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
        let reader = self.reader_snapshot(id, actor, true).await?;
        let job = &reader.job;
        if !matches!(job.status, JobStatus::Succeeded | JobStatus::Failed | JobStatus::PartiallySucceeded)
            || job.finished_at.is_none()
            || job.result_expires_at.is_none_or(|at| at <= Instant::now())
        {
            return Err(Error::BusinessLogicError("任务尚未完成或结果已过期，请查询原任务".into()));
        }

        let compact = self.reader_context(&reader, actor).await?;
        let source_id =
            job.input_file_asset_id.as_ref().ok_or_else(|| Error::NotFound("任务输入缺失".into()))?;
        let (source, _): (Envelope, _) = self.reader_private(job, source_id.as_ref(), actor).await?;
        if source.kind != compact.kind {
            return Err(Error::Internal("任务私有上下文与原输入类型不符".into()));
        }

        let asset =
            job.result_file_asset_id.as_ref().ok_or_else(|| Error::NotFound("任务结果不存在".into()))?;
        let (mut result, _): (TransferResult, _) = self.reader_private(job, asset.as_ref(), actor).await?;
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
        let mut context = self.handler.context(compact.kind, &compact.input, decoded.as_ref(), actor).await?;
        if !reader.owned {
            context.retain_classification();
        }
        result.metadata["kind"] = json!(source.kind);
        result.metadata["context"] = context.context;

        self.validate_reader(job, actor).await?;
        Ok(result)
    }
}
