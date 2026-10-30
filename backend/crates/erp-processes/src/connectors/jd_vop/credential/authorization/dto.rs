//! 页面授权只接收单次敏感输入，仅返回不含凭据的状态投影。
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroizing;

use super::protocol::callback_uri;
use super::{AuthorizationError, Result};

/// 页面单次授权命令；密码及私钥退出作用域时清除，不实现 Debug 或 Serialize。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopAuthorizationRequest {
    pub operation_id: String,
    pub expected_version: Option<u64>,
    pub username: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub redirect_uri: String,
    pub private_key: Zeroizing<String>,
    pub evidence_reference: Option<String>,
}

/// 查询原授权操作时只接受不含凭据的操作ID。
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopAuthorizationStatusQuery {
    pub operation_id: Option<String>,
}

impl JdVopAuthorizationRequest {
    pub(super) fn validate(&self) -> Result<()> {
        if Uuid::parse_str(&self.operation_id).is_err()
            || self.expected_version.is_some_and(|version| version == 0 || version > i64::MAX.unsigned_abs())
            || !bounded(&self.username, 256)
            || self.username.trim() != self.username.as_str()
            || !bounded(&self.password, 4096)
            || self.private_key.is_empty()
            || self.private_key.len() > 32768
            || self
                .evidence_reference
                .as_ref()
                .is_some_and(|value| !bounded(value, 1024) || value.trim().is_empty())
        {
            return Err(AuthorizationError::InvalidInput);
        }
        callback_uri(&self.redirect_uri)?;
        Ok(())
    }
}

impl JdVopAuthorizationStatusQuery {
    /// 校验可选操作ID，不解析或回显任何授权正文。
    /// # 参数
    /// 无。
    /// # 返回
    /// 未指定操作或符合UUID格式时成功。
    /// # 错误
    /// 操作ID非法时返回固定错误。
    pub fn validate(&self) -> Result<()> {
        if self.operation_id.as_ref().is_some_and(|id| Uuid::parse_str(id).is_err()) {
            return Err(AuthorizationError::InvalidInput);
        }
        Ok(())
    }
}

fn bounded(value: &str, maximum: usize) -> bool {
    !value.is_empty() && value.len() <= maximum && !value.chars().any(char::is_control)
}

/// 页面可缓存的脱敏状态；authorized只表达本地凭据可用，健康检查另行执行。
#[derive(Debug, Clone, Serialize)]
pub struct JdVopAuthorizationStatus {
    pub state: &'static str,
    pub version: Option<u64>,
    pub expires_at: Option<i64>,
    pub operation_id: Option<String>,
    pub operation_state: Option<&'static str>,
    pub can_authorize: bool,
    pub can_configure_tokens: bool,
    pub recovery_after: Option<i64>,
    pub message: &'static str,
}

impl JdVopAuthorizationStatus {
    pub(super) fn empty(state: &'static str, can_authorize: bool, message: &'static str) -> Self {
        Self {
            state,
            version: None,
            expires_at: None,
            operation_id: None,
            operation_state: None,
            can_authorize,
            can_configure_tokens: false,
            recovery_after: None,
            message,
        }
    }
}
