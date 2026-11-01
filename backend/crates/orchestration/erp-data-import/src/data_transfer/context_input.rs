//! 新任务的轻量私有上下文与任务同事务保存，不读取上传文件即可识别类型。
use erp_core::common::time::Instant;
use erp_core::ids::BackgroundJobId;
use erp_support::repository::bulk_job::BackgroundJobInput;
use erp_support::{BackgroundJob, BulkJobExt, FileAssetExt};
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::types::Envelope;
use super::{DataTransferProcess, TransferContext, TransferKind};
use crate::{Error, Result};

pub(super) const CONTEXT_FORMAT: &str = "data_transfer_context_v1";

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ContextInput {
    pub kind: TransferKind,
    pub input: Value,
    pub expires_at: i64,
}

impl ContextInput {
    pub(super) fn new(source: &Envelope, expires_at: Instant) -> Self {
        let mut input = TransferContext::from_input(source.kind, &source.input).context;
        if let Some(scope) = source.input.get("scope").filter(|value| value.is_string()) {
            input["scope"] = scope.clone();
        }
        // 门户绑定版本只用于当前认证重验，TransferContext 的公开白名单不包含 session。
        if source.kind == TransferKind::PortalBatch {
            input["session"] = source.input["session"].clone();
        }
        Self { kind: source.kind, input, expires_at: expires_at.unix_secs() }
    }

    pub(super) fn record(&self) -> Result<BackgroundJobInput> {
        let bytes = serde_json::to_vec(self).map_err(|_| Error::Internal("任务上下文无法编码".into()))?;
        Ok(BackgroundJobInput::new(CONTEXT_FORMAT, bytes)?)
    }
}

impl DataTransferProcess {
    pub(super) async fn context_input(&self, job: &BackgroundJob) -> Result<ContextInput> {
        if let Some(bytes) = self
            .db
            .bulk_job()
            .strict_job_input(&BackgroundJobId::new(&job.base.id), CONTEXT_FORMAT, &mut NoTransaction)
            .await?
        {
            return decode_context(&bytes);
        }
        let source = self.source(job).await?;
        Ok(ContextInput::new(&source, self.source_expiry(job).await?))
    }

    pub(super) async fn source_expiry(&self, job: &BackgroundJob) -> Result<Instant> {
        let id = job.input_file_asset_id.as_ref().ok_or_else(|| Error::NotFound("任务输入缺失".into()))?;
        self.db
            .file_assets()
            .find_by_id(id.as_ref(), &mut NoTransaction)
            .await?
            .filter(|asset| asset.created_by == job.requested_by && asset.destroyed_at.is_none())
            .and_then(|asset| asset.expires_at)
            .ok_or_else(|| Error::NotFound("任务输入不存在或不可读取".into()))
    }
}

fn decode_context(bytes: &[u8]) -> Result<ContextInput> {
    let input: ContextInput =
        serde_json::from_slice(bytes).map_err(|_| Error::Internal("任务私有上下文内容损坏".into()))?;
    if !input.input.is_object() || input.expires_at <= 0 {
        return Err(Error::Internal("任务私有上下文内容损坏".into()));
    }
    Ok(input)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use serde_json::json;

    use super::*;

    #[test]
    fn compact_context_retains_private_portal_binding_without_file_or_command_content() {
        let source = Envelope {
            kind: TransferKind::PortalBatch,
            account: "portal".into(),
            account_kind: AccountKind::Supplier,
            input: json!({"session":{"account":"portal","account_version":2,"binding_version":3},
                "request":{"rows":[{"input":{"secret":"SECRET"}}]},"content_base64":"SECRET"}),
        };
        let snapshot = ContextInput::new(&source, Instant::from_unix_secs(200));
        assert_eq!(snapshot.input["session"]["binding_version"], 3);
        assert_eq!(snapshot.expires_at, 200);
        assert!(!serde_json::to_string(&snapshot).unwrap().contains("SECRET"));
        assert!(TransferContext::from_input(snapshot.kind, &snapshot.input).context.get("session").is_none());
    }

    #[test]
    fn malformed_compact_records_fail_without_mutating_non_object_json() {
        for value in [
            json!({"kind":"sales_orders","input":"invalid","expires_at":200}),
            json!({"kind":"sales_orders","input":{},"expires_at":0}),
            json!({"kind":"unknown","input":{},"expires_at":200}),
        ] {
            assert!(decode_context(&serde_json::to_vec(&value).unwrap()).is_err());
        }
    }
}
