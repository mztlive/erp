//! 京东凭据刷新请求和结果合同。
use async_trait::async_trait;
use erp_core::common::time::Instant;

use super::token::{JdVopAccessToken, token_error};
use crate::ports::connector::common::ConnectorResult;
/// 原刷新身份及敏感凭据；不提供 Debug，不得进入日志或业务响应。
pub struct JdVopRefreshRequest {
    app_key: String,
    app_secret: String,
    refresh_token: String,
}

impl JdVopRefreshRequest {
    /// 保存受控存储提供的原刷新身份；不编码或发送网络。
    /// 校验通过的短生命周期请求。
    /// 字段缺失、超长或含控制字符时拒绝。
    /// 应用标识、应用密钥与原刷新令牌。
    pub fn new(app_key: &str, app_secret: &str, refresh_token: &str) -> ConnectorResult<Self> {
        if [app_key, app_secret, refresh_token]
            .iter()
            .any(|v| v.trim().is_empty() || v.len() > 8192 || v.chars().any(char::is_control))
        {
            return Err(token_error());
        }
        Ok(Self {
            app_key: app_key.into(),
            app_secret: app_secret.into(),
            refresh_token: refresh_token.into(),
        })
    }
    /// 返回应用标识，只用于协议编码。
    /// 应用标识借用。
    pub fn app_key(&self) -> &str {
        &self.app_key
    }
    /// 返回受控应用密钥，只用于协议编码。
    /// 应用密钥借用；调用方不得记录。
    pub fn app_secret(&self) -> &str {
        &self.app_secret
    }
    /// 返回原刷新令牌，只用于协议编码。
    /// 原刷新令牌借用；调用方不得记录。
    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }
}
/// 尚未写回凭据存储的新令牌；持久化必须使用原版本进行 CAS，并确保账号不变。
pub struct JdVopRefreshResult {
    access_token: String,
    refresh_token: String,
    expires_in: u64,
}

impl JdVopRefreshResult {
    /// 接收适配器已解释的响应载荷；不会持久化或发送请求。
    /// 待按原版本保存的新凭据。
    /// 令牌缺失、格式非法或有效期为零时拒绝。
    pub fn new(access_token: String, refresh_token: String, expires_in: u64) -> ConnectorResult<Self> {
        if expires_in == 0
            || [&access_token, &refresh_token].iter().any(|value| {
                value.trim().is_empty()
                    || value.trim() != value.as_str()
                    || value.len() > 8192
                    || value.chars().any(char::is_control)
            })
        {
            return Err(token_error());
        }
        Ok(Self { access_token, refresh_token, expires_in })
    }
    /// 取得待保存的完整凭据并在观察时刻计算到期时间。
    /// 访问令牌、刷新令牌与到期时间。
    /// 有效期换算或相加溢出时拒绝。
    pub fn into_credential_parts(self, observed_at: Instant) -> ConnectorResult<(String, String, Instant)> {
        let seconds = i64::try_from(self.expires_in).map_err(|_| token_error())?;
        let expires = observed_at.unix_secs().checked_add(seconds).ok_or_else(token_error)?;
        let expires = Instant::try_from_unix_secs(expires).map_err(|_| token_error())?;
        Ok((self.access_token, self.refresh_token, expires))
    }
    /// 取得待受控持久化的刷新令牌，不得写入日志。
    /// 新刷新令牌原文。
    pub fn refresh_token(&self) -> &str {
        &self.refresh_token
    }
    /// 将明确账号及存储版本绑定到新访问令牌，不自行写回存储。
    /// 有界计算有效期的令牌快照，供同一受控 CAS 保存。
    /// 时间溢出、账号或版本无效时返回脱敏错误。
    /// `account_scope`、`new_version` 来自凭据拥有服务；`observed_at` 为响应取得时刻。
    pub fn access_token(
        &self,
        account_scope: String,
        new_version: String,
        observed_at: Instant,
    ) -> ConnectorResult<JdVopAccessToken> {
        let seconds = i64::try_from(self.expires_in).map_err(|_| token_error())?;
        let expires = observed_at.unix_secs().checked_add(seconds).ok_or_else(token_error)?;
        let expires = Instant::try_from_unix_secs(expires).map_err(|_| token_error())?;
        JdVopAccessToken::new(self.access_token.clone(), new_version, account_scope, expires)
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum RefreshFailure {
    Unknown,
    AuthorizationInvalid,
}

/// 凭据编排消费的单次刷新通信；实现必须禁止隐藏重试。
#[async_trait]
pub trait OAuthRefresh: Send + Sync {
    /// 使用原刷新身份发送一次，返回明确撤销或未知分类。
    /// 已解释的新凭据。
    /// 明确撤销与未知结果保持独立分类。
    async fn refresh(&self, request: JdVopRefreshRequest) -> Result<JdVopRefreshResult, RefreshFailure>;
}
