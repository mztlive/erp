//! 运维导入与只读状态合同；正文不实现 Debug/Serialize，状态不包含令牌或密文。
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_credential::{SupplierCredential, SupplierCredentialRefreshState};
use serde::{Deserialize, Serialize};

/// 仅从受控文件或标准输入读取；有效期是授权响应换算后的 Unix 秒。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopCredentialImport {
    pub credential_id: String,
    pub account_scope: String,
    pub app_key: String,
    pub access_token: String,
    pub refresh_token: String,
    pub expires_at: i64,
}

/// 可输出到运维终端的脱敏状态；账号及应用身份采用固定遮罩。
#[derive(Debug, Serialize)]
pub struct JdVopCredentialStatus {
    pub credential_id: String,
    pub account_scope: String,
    pub app_key: String,
    pub version: u64,
    pub expires_at: i64,
    pub refresh_stage: String,
    pub refresh_deadline: Option<i64>,
    pub reauthorization_required: bool,
}

impl JdVopCredentialStatus {
    pub(super) fn from_record(record: &SupplierCredential, now: Instant) -> Self {
        Self {
            credential_id: record.base.id.clone(),
            account_scope: "[REDACTED]".into(),
            app_key: "[REDACTED]".into(),
            version: record.base.version,
            expires_at: record.expires_at.unix_secs(),
            refresh_stage: record.refresh.code().into(),
            refresh_deadline: record.refresh.deadline().map(Instant::unix_secs),
            reauthorization_required: record.expires_at <= now
                || matches!(
                    record.refresh,
                    SupplierCredentialRefreshState::ReauthorizationRequired { .. }
                        | SupplierCredentialRefreshState::SentUnknown { .. }
                ),
        }
    }
}
