//! 读取真实凭据并在提前刷新窗口执行单次刷新；已发送未知不按租约重试。
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use config::JdVopConfig;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialRefreshClaim, SupplierCredentialRefreshState,
};
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::jd_vop::oauth::{
    JdVopRefreshRequest, JdVopRefreshResult, OAuthRefresh, RefreshFailure,
};
use erp_supply::ports::jd_vop::token::{JdVopAccessToken, JdVopTokenProvider, token_error};
use id_generator::next_id;
use mongodb::Database;
use tokio::sync::Semaphore;
use tokio::time::timeout;

use super::crypto::CredentialCrypto;
use super::store::{CredentialStore, MongoCredentialStore};
use super::{Clock, SystemClock};

/// 多进程以 Mongo 版本竞争刷新的固定凭据提供器；不提供 Debug 或令牌序列化。
#[derive(Clone)]
pub struct MongoJdVopTokenProvider {
    crypto: CredentialCrypto,
    app_secret: String,
    timeout_seconds: u64,
    store: Arc<dyn CredentialStore>,
    oauth: Arc<dyn OAuthRefresh>,
    clock: Arc<dyn Clock>,
    owner_id: String,
    refresh_slot: Arc<Semaphore>,
}

impl MongoJdVopTokenProvider {
    /// 创建固定 Mongo 身份及受控 OAuth 刷新客户端，不发起读取或网络。
    /// # 参数
    /// `db` 是应用数据库；`settings` 提供独立加密密钥、应用凭据和超时；`oauth` 是应用注入的单次刷新通信。
    /// # 返回
    /// 可显式注入京东 HTTP 客户端的令牌提供器。
    /// # 错误
    /// 配置或 TLS 客户端无法构造时返回脱敏鉴权错误。
    pub fn new(db: Database, settings: &JdVopConfig, oauth: Arc<dyn OAuthRefresh>) -> ConnectorResult<Self> {
        let crypto = CredentialCrypto::new(settings)?;
        let store = MongoCredentialStore::new(db, crypto.id.clone(), crypto.binding.clone());
        Ok(Self {
            crypto,
            app_secret: settings.app_secret.clone(),
            timeout_seconds: settings.timeout_seconds,
            store: Arc::new(store),
            oauth,
            clock: Arc::new(SystemClock),
            owner_id: next_id(),
            refresh_slot: Arc::new(Semaphore::new(1)),
        })
    }

    async fn current(&self) -> ConnectorResult<JdVopAccessToken> {
        let record = self.store.load().await?.ok_or_else(token_error)?;
        self.snapshot(&record).await
    }

    async fn start_refresh(
        &self,
        record: SupplierCredential,
        request: JdVopRefreshRequest,
    ) -> ConnectorResult<JdVopAccessToken> {
        let Ok(permit) = self.refresh_slot.clone().try_acquire_owned() else {
            return self.current().await;
        };
        let context = self.clone();
        // JoinHandle被读取超时丢弃只分离等待者；刷新仍负责保存响应或保留发送未知。
        let task = tokio::spawn(async move {
            let _permit = permit;
            timeout(Duration::from_secs(600), context.refresh(record, request))
                .await
                .map_err(|_| token_error())?
        });
        task.await.map_err(|_| token_error())?
    }

    async fn snapshot(&self, record: &SupplierCredential) -> ConnectorResult<JdVopAccessToken> {
        // 官方指南：新token产生后旧token仅保留10秒。已发送未知不能沿用本地原有效期。
        if matches!(
            record.refresh,
            SupplierCredentialRefreshState::ReauthorizationRequired { .. }
                | SupplierCredentialRefreshState::SentUnknown { .. }
                | SupplierCredentialRefreshState::AuthorizationClaimed { .. }
                | SupplierCredentialRefreshState::AuthorizationSentUnknown { .. }
                | SupplierCredentialRefreshState::AuthorizationRejected { .. }
        ) {
            return Err(token_error());
        }
        let plain = self.crypto.open(record)?;
        let token = JdVopAccessToken::new(
            plain.access_token,
            record.base.version.to_string(),
            self.crypto.binding.account_scope.clone(),
            record.expires_at,
        )?;
        token.value_for(&self.crypto.binding.account_scope, self.clock.now())?;
        Ok(token)
    }

    async fn refresh(
        &self,
        record: SupplierCredential,
        request: JdVopRefreshRequest,
    ) -> ConnectorResult<JdVopAccessToken> {
        let operation_id = next_id();
        let claim = SupplierCredentialRefreshClaim {
            owner_id: self.owner_id.clone(),
            operation_id,
            now: self.clock.now(),
            lease_seconds: self.timeout_seconds + 5,
        };
        let claimed = match self.store.claim(record.base.version, claim).await {
            Ok(Some(claimed)) => claimed,
            _ => return self.current().await,
        };
        let sent = match self.store.sent(claimed, self.clock.now()).await {
            Ok(Some(sent)) => sent,
            _ => return self.current().await,
        };
        let latest_finish = self
            .clock
            .now()
            .unix_secs()
            .checked_add(i64::try_from(self.timeout_seconds).map_err(|_| token_error())?)
            .and_then(|deadline| deadline.checked_add(1))
            .ok_or_else(token_error)?;
        if sent.refresh.deadline().is_none_or(|deadline| deadline.unix_secs() < latest_finish) {
            return self.current().await;
        }
        let result = timeout(Duration::from_secs(self.timeout_seconds), self.oauth.refresh(request))
            .await
            .unwrap_or(Err(RefreshFailure::Unknown));
        self.finish(sent, result).await
    }

    async fn finish(
        &self,
        sent: SupplierCredential,
        result: Result<JdVopRefreshResult, RefreshFailure>,
    ) -> ConnectorResult<JdVopAccessToken> {
        match result {
            Err(RefreshFailure::AuthorizationInvalid) => {
                let _ = self.store.invalidate(sent, self.clock.now()).await;
                Err(token_error())
            },
            Err(RefreshFailure::Unknown) => self.current().await,
            Ok(result) => {
                let now = self.clock.now();
                let (access, refresh, expires) = result.into_credential_parts(now)?;
                let payload = self.crypto.replacement(access, refresh, expires, now)?;
                // 保存失败或提交未知时只读查询原回执/当前版本，绝不重发 refresh。
                let _ = self.store.complete(sent, payload, now).await;
                self.current().await
            },
        }
    }
}

#[async_trait]
impl JdVopTokenProvider for MongoJdVopTokenProvider {
    async fn access_token(&self, reference: &str) -> ConnectorResult<JdVopAccessToken> {
        if reference != self.crypto.id {
            return Err(token_error());
        }
        let record = self.store.load().await?.ok_or_else(token_error)?;
        let now = self.clock.now();
        if record.expires_at <= now {
            return Err(token_error());
        }
        if matches!(
            record.refresh,
            SupplierCredentialRefreshState::ReauthorizationRequired { .. }
                | SupplierCredentialRefreshState::SentUnknown { .. }
                | SupplierCredentialRefreshState::AuthorizationClaimed { .. }
                | SupplierCredentialRefreshState::AuthorizationSentUnknown { .. }
                | SupplierCredentialRefreshState::AuthorizationRejected { .. }
        ) {
            return Err(token_error());
        }
        let plain = self.crypto.open(&record)?;
        let due = plain.refresh_after <= now.unix_secs();
        let claimable = matches!(record.refresh, SupplierCredentialRefreshState::Ready)
            || matches!(record.refresh, SupplierCredentialRefreshState::Claimed { lease_until, .. } if lease_until <= now);
        if due && claimable {
            let request = JdVopRefreshRequest::new(
                &self.crypto.binding.app_key,
                &self.app_secret,
                &plain.refresh_token,
            )?;
            return self.start_refresh(record, request).await;
        }
        self.snapshot(&record).await
    }

    async fn settle_refresh(&self) -> ConnectorResult<()> {
        let permit = self.refresh_slot.acquire().await.map_err(|_| token_error())?;
        drop(permit);
        Ok(())
    }
}

#[cfg(test)]
#[path = "provider_tests.rs"]
mod tests;
