//! 服务端技术凭据；账号身份固定，令牌仅以密文保存，未知刷新不得按租约自动重发。

mod authorization;
mod authorization_types;
mod configuration;
mod refresh;

use std::fmt;

pub use authorization_types::{SupplierCredentialAuthorizationEvent, SupplierCredentialAuthorizationRequest};
pub use configuration::{SupplierCredentialConfiguration, SupplierCredentialConfigurationEvent};
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierCredentialProvider {
    JdVop,
}

/// 账号及应用绑定必须从服务端部署配置取得，不能由请求正文切换。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierCredentialBinding {
    pub provider: SupplierCredentialProvider,
    pub account_scope: String,
    pub app_key: String,
}

/// 密文由受控加密器生成，拥有领域不解析 access_token 或 refresh_token。
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierCredentialPayload {
    pub encrypted_payload: String,
    /// 受控加密器以专用密钥计算的身份、明文令牌及有效期HMAC；不是裸令牌摘要。
    pub payload_fingerprint: String,
    pub expires_at: Instant,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierCredentialRefreshState {
    Ready,
    Claimed {
        owner_id: String,
        operation_id: String,
        lease_until: Instant,
    },
    SentUnknown {
        owner_id: String,
        operation_id: String,
        lease_until: Instant,
        sent_at: Instant,
    },
    ReauthorizationRequired {
        owner_id: String,
        operation_id: String,
        lease_until: Instant,
        sent_at: Instant,
        invalidated_at: Instant,
    },
    AuthorizationClaimed {
        owner_id: String,
        operation_id: String,
        lease_until: Instant,
    },
    AuthorizationSentUnknown {
        owner_id: String,
        operation_id: String,
        lease_until: Instant,
        sent_at: Instant,
        exchange_sent_at: Option<Instant>,
    },
    AuthorizationRejected {
        operation_id: String,
        rejected_at: Instant,
    },
}

/// 仅未发送的领取允许到期后重新竞争；发送后保留原操作身份。
#[derive(Clone)]
pub struct SupplierCredentialRefreshClaim {
    pub owner_id: String,
    pub operation_id: String,
    pub now: Instant,
    pub lease_seconds: u64,
}

/// 重新授权是显式恢复命令，不能由普通令牌导入隐式替代。
#[derive(Clone)]
pub struct SupplierCredentialReauthorization {
    pub expected_version: u64,
    pub new_authorization: bool,
    pub operation_id: String,
    pub actor_id: String,
    pub evidence_reference: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplierCredentialReauthorizationEvent {
    pub operation_id: String,
    pub actor_id: String,
    pub evidence_reference: String,
    pub previous_refresh_operation_id: Option<String>,
    pub previous_version: u64,
    pub occurred_at: Instant,
    pub payload_fingerprint: String,
}

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SupplierCredential {
    #[serde(flatten)]
    pub base: BaseModel,
    pub provider: SupplierCredentialProvider,
    pub account_scope: String,
    pub app_key: String,
    pub encrypted_payload: String,
    pub payload_fingerprint: String,
    pub expires_at: Instant,
    pub refresh: SupplierCredentialRefreshState,
    #[serde(default)]
    pub last_completed_refresh_operation_id: Option<String>,
    #[serde(default)]
    pub reauthorization_history: Vec<SupplierCredentialReauthorizationEvent>,
    #[serde(default)]
    pub account_identity_fingerprint: Option<String>,
    #[serde(default)]
    pub authorization_candidate_identity_fingerprint: Option<String>,
    #[serde(default)]
    pub authorized_uid_fingerprint: Option<String>,
    #[serde(default)]
    pub last_completed_authorization_operation_id: Option<String>,
    #[serde(default)]
    pub authorization_history: Vec<SupplierCredentialAuthorizationEvent>,
    #[serde(default)]
    pub manually_configured: bool,
    #[serde(default)]
    pub configuration_history: Vec<SupplierCredentialConfigurationEvent>,
}

impl fmt::Debug for SupplierCredential {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SupplierCredential { [REDACTED] }")
    }
}

impl fmt::Debug for SupplierCredentialBinding {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SupplierCredentialBinding { [REDACTED] }")
    }
}

impl fmt::Debug for SupplierCredentialPayload {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("SupplierCredentialPayload { [REDACTED] }")
    }
}

impl SupplierCredentialBinding {
    /// 校验固定供应商账号和应用身份，拒绝空白及不可见字符。
    ///
    /// 身份有界且完整时成功。账号或应用为空、包含控制符及首尾空白时拒绝。
    pub fn validate(&self) -> Result<()> {
        identity(&self.account_scope, 256)?;
        identity(&self.app_key, 256)
    }
}

impl SupplierCredentialPayload {
    /// 校验密文和明确的有效期，不从远端余额或缺省值推断有效期。
    ///
    /// 有界非空密文且有效期晚于当前时刻时成功。密文为空、过长或授权已到期时拒绝。
    ///
    /// `now` 为服务器当前时间。
    pub fn validate(&self, now: Instant) -> Result<()> {
        self.validate_shape()?;
        if self.expires_at <= now {
            return Err(Error::ValidationError("技术凭据密文或授权有效期无效".into()));
        }
        Ok(())
    }

    fn validate_shape(&self) -> Result<()> {
        if self.encrypted_payload.trim().is_empty()
            || self.encrypted_payload.len() > 1_048_576
            || self.payload_fingerprint.len() != 64
            || !self.payload_fingerprint.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(Error::ValidationError("技术凭据密文或授权有效期无效".into()));
        }
        Ok(())
    }
}

impl SupplierCredential {
    /// 创建稳定凭据身份；同账号及应用的唯一性由仓储唯一索引仲裁。
    ///
    /// 初始版本为1且未开始刷新的凭据。身份、密文或时间无效时拒绝。
    ///
    /// 稳定引用、服务端绑定、受控密文及当前时间。
    pub fn new(
        id: String,
        binding: SupplierCredentialBinding,
        payload: SupplierCredentialPayload,
        now: Instant,
    ) -> Result<Self> {
        identity(&id, 128)?;
        binding.validate()?;
        payload.validate(now)?;

        let timestamp =
            u64::try_from(now.unix_secs()).map_err(|_| Error::ValidationError("技术凭据时间无效".into()))?;
        let mut base = BaseModel::new(id);
        base.created_at = timestamp;
        base.updated_at = timestamp;
        Ok(Self {
            base,
            provider: binding.provider,
            account_scope: binding.account_scope,
            app_key: binding.app_key,
            encrypted_payload: payload.encrypted_payload,
            payload_fingerprint: payload.payload_fingerprint,
            expires_at: payload.expires_at,
            refresh: SupplierCredentialRefreshState::Ready,
            last_completed_refresh_operation_id: None,
            reauthorization_history: vec![],
            account_identity_fingerprint: None,
            authorization_candidate_identity_fingerprint: None,
            authorized_uid_fingerprint: None,
            last_completed_authorization_operation_id: None,
            authorization_history: vec![],
            manually_configured: false,
            configuration_history: vec![],
        })
    }

    /// 返回原绑定身份，调用方不得将其替换为外部请求中的身份。当前账号和应用的固定绑定。
    pub fn binding(&self) -> SupplierCredentialBinding {
        SupplierCredentialBinding {
            provider: self.provider,
            account_scope: self.account_scope.clone(),
            app_key: self.app_key.clone(),
        }
    }

    /// 核对读取或更新没有跨账号、应用或已删除身份。
    ///
    /// 身份一致时成功。身份变化或原记录已删除时拒绝。
    pub fn require_binding(&self, binding: &SupplierCredentialBinding) -> Result<()> {
        binding.validate()?;
        if self.base.is_deleted()
            || self.provider != binding.provider
            || self.account_scope != binding.account_scope
            || self.app_key != binding.app_key
        {
            return Err(Error::ConflictError("技术凭据与原账号或应用绑定不一致".into()));
        }
        Ok(())
    }

    /// 普通导入仅更新同身份及所见版本，不能覆盖已发送刷新或失效授权。
    ///
    /// 成功时推进版本并替换令牌。版本不符、刷新仍在领取、发送未知或需要重新授权时拒绝。
    ///
    /// 新密文、原记录版本和当前服务器时间。
    pub fn import(&mut self, payload: SupplierCredentialPayload, expected: u64, now: Instant) -> Result<()> {
        self.require_unbound_import()?;
        self.require_version(expected)?;
        payload.validate(now)?;
        if !matches!(self.refresh, SupplierCredentialRefreshState::Ready)
            && !matches!(self.refresh, SupplierCredentialRefreshState::Claimed { lease_until, .. } if lease_until <= now)
        {
            return Err(Error::ConflictError(
                "原技术凭据正在刷新或需要明确重新授权，不能普通导入覆盖".into(),
            ));
        }

        let base = self.next_base(now)?;
        self.encrypted_payload = payload.encrypted_payload;
        self.payload_fingerprint = payload.payload_fingerprint;
        self.expires_at = payload.expires_at;
        self.refresh = SupplierCredentialRefreshState::Ready;
        self.base = base;
        Ok(())
    }

    pub(super) fn require_version(&self, expected: u64) -> Result<()> {
        if expected == 0 || self.base.version != expected || self.base.is_deleted() {
            return Err(Error::ConflictError("技术凭据版本已变化，请查询原结果后处理".into()));
        }
        Ok(())
    }

    pub(super) fn next_base(&self, now: Instant) -> Result<BaseModel> {
        if self.base.is_deleted() || self.base.version == 0 {
            return Err(Error::ConflictError("原技术凭据元数据无效或已停用".into()));
        }

        let mut base = self.base.clone();
        base.version = base
            .version
            .checked_add(1)
            .filter(|value| i64::try_from(*value).is_ok())
            .ok_or_else(|| Error::ConflictError("技术凭据版本已达到存储边界".into()))?;
        base.updated_at =
            u64::try_from(now.unix_secs()).map_err(|_| Error::ValidationError("技术凭据时间无效".into()))?;
        if base.updated_at < self.base.updated_at {
            return Err(Error::ConflictError("服务器时间早于原技术凭据操作，请先核实时间".into()));
        }
        Ok(base)
    }

    fn require_unbound_import(&self) -> Result<()> {
        if self.account_identity_fingerprint.is_some()
            || self.authorization_candidate_identity_fingerprint.is_some()
            || !self.authorization_history.is_empty()
        {
            return Err(Error::ConflictError(
                "页面授权凭据必须通过原账号核验授权，不能以无身份依据的导入覆盖".into(),
            ));
        }
        Ok(())
    }
}

pub(crate) fn identity(value: &str, maximum: usize) -> Result<()> {
    if value.is_empty()
        || value != value.trim()
        || value.len() > maximum
        || value.chars().any(char::is_control)
    {
        return Err(Error::ValidationError("技术凭据身份或操作依据无效".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests;
