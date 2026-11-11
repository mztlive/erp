use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_audit::execution::persist_log;
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialConfiguration, SupplierCredentialPayload,
};
use erp_integration::indexes::supplier_credential::ensure;
use erp_integration::repository::SupplierCredentialExt;
use persistence_core::{NoTransaction, Transactional};
use serde::Deserialize;
use uuid::Uuid;
use zeroize::Zeroizing;

use super::super::crypto::CredentialCrypto;
use super::{AuthorizationError, JdVopAuthorizationProcess, JdVopAuthorizationStatus, Result, status};
use crate::{Error as ProcessError, Result as ProcessResult};

/// 页面令牌仅在当前请求存活；部署身份不可由请求切换，不实现Debug或Serialize。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct JdVopTokenConfiguration {
    pub operation_id: String,
    pub expected_version: Option<u64>,
    pub access_token: Zeroizing<String>,
    pub refresh_token: Zeroizing<String>,
    pub expires_at: i64,
}

impl JdVopTokenConfiguration {
    fn validate(&self, now: Instant) -> Result<Instant> {
        let expiry =
            Instant::try_from_unix_secs(self.expires_at).map_err(|_| AuthorizationError::InvalidTokens)?;
        if Uuid::parse_str(&self.operation_id).is_err()
            || self.expected_version.is_some_and(|value| value == 0 || i64::try_from(value).is_err())
            || expiry <= now
            || !valid_token(&self.access_token)
            || !valid_token(&self.refresh_token)
        {
            return Err(AuthorizationError::InvalidTokens);
        }
        Ok(expiry)
    }
}

fn valid_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 8192 && !value.chars().any(|ch| ch.is_whitespace() || ch.is_control())
}

impl JdVopAuthorizationProcess {
    /// 将外部授权令牌保存到自动刷新使用的同一加密记录。
    /// # 参数
    /// 原连接ID、同次授权的两项令牌和实际有效期、当前操作人。
    /// # 返回
    /// 脱敏配置状态及操作回执，不返回令牌或自动开启下单。
    /// # 错误
    /// 格式、权限、绑定、所见版本或活跃操作不符时拒绝；提交结果不明时须查询原操作。
    pub async fn configure_tokens(
        &self,
        id: &str,
        request: JdVopTokenConfiguration,
        actor: &AuditActor,
    ) -> Result<JdVopAuthorizationStatus> {
        let now = Instant::now();
        let expiry = request.validate(now)?;
        let connection = self.access.require(id, actor, "authorize", &mut NoTransaction).await?;
        self.access.binding(&connection)?;
        let settings = self.access.settings.as_ref().ok_or(AuthorizationError::Unavailable)?;
        let crypto = CredentialCrypto::new(settings).map_err(|_| AuthorizationError::Unavailable)?;
        let payload = crypto
            .replacement(request.access_token.to_string(), request.refresh_token.to_string(), expiry, now)
            .map_err(|_| AuthorizationError::InvalidTokens)?;
        let command = SupplierCredentialConfiguration {
            operation_id: request.operation_id.clone(),
            actor_id: actor.id().to_string(),
            expected_version: request.expected_version,
        };
        drop(request);
        ensure(&self.access.db).await.map_err(|_| AuthorizationError::Unavailable)?;
        let record = self.save_tokens(id, actor, &crypto, payload, command.clone()).await?;
        let mut result =
            status::operation_projection(&record, &crypto, Instant::now(), Some(&command.operation_id));
        result.can_configure_tokens =
            result.recovery_after.is_none_or(|deadline| deadline <= Instant::now().unix_secs());
        Ok(result)
    }

    async fn save_tokens(
        &self,
        id: &str,
        actor: &AuditActor,
        crypto: &CredentialCrypto,
        payload: SupplierCredentialPayload,
        command: SupplierCredentialConfiguration,
    ) -> Result<SupplierCredential> {
        let access = self.access.clone();
        let id = id.to_string();
        let actor = actor.clone();
        let crypto = crypto.clone();
        let result: ProcessResult<_> = self
            .access
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let connection = access.require(&id, &actor, "authorize", executor).await?;
                    access
                        .binding(&connection)
                        .map_err(|_| ProcessError::ConflictError("京东连接配置已变化".into()))?;
                    let (record, changed) = access
                        .db
                        .supplier_credentials()
                        .configure_tokens(
                            &crypto.id,
                            &crypto.binding,
                            payload,
                            &command,
                            Instant::now(),
                            executor,
                        )
                        .await?;
                    if changed {
                        let log = actor.resource_log(
                            "supplier_credential.configure_tokens",
                            "supplier_credential",
                            crypto.id,
                        )?;
                        persist_log(&access.db, &log, executor).await?;
                    }
                    Ok(record)
                })
            })
            .await;
        result.map_err(configuration_error)
    }
}

fn configuration_error(error: ProcessError) -> AuthorizationError {
    match error {
        ProcessError::Forbidden(_) => AuthorizationError::Forbidden,
        ProcessError::ConflictError(_) => AuthorizationError::Conflict,
        ProcessError::ReceiptDuplicate(_) | ProcessError::TransientTransaction(_) => {
            AuthorizationError::Conflict
        },
        ProcessError::ValidationError(_) => AuthorizationError::InvalidTokens,
        other => AuthorizationError::from(other),
    }
}

#[cfg(test)]
mod tests {
    use erp_integration::entity::supplier_credential::SupplierCredentialRefreshClaim;

    use super::*;

    fn request() -> JdVopTokenConfiguration {
        JdVopTokenConfiguration {
            operation_id: "26c4dfba-fda5-490d-9d46-d88da680f51d".into(),
            expected_version: None,
            access_token: Zeroizing::new("access-sentinel".into()),
            refresh_token: Zeroizing::new("refresh-sentinel".into()),
            expires_at: 87_400,
        }
    }
    fn crypto() -> CredentialCrypto {
        let settings = serde_json::from_value(serde_json::json!({
            "app_key":"app", "app_secret":"secret", "account_scope":"account", "credential_id":"credential",
            "credential_encryption_key":"unit-only-independent-credential-key-32"
        }))
        .unwrap();
        CredentialCrypto::new(&settings).unwrap()
    }

    #[test]
    fn tokens_require_explicit_future_expiry_and_do_not_accept_deployment_overrides() {
        let now = Instant::from_unix_secs(1000);
        assert!(request().validate(now).is_ok());
        for value in ["", "with space", "with\nnewline"] {
            let mut input = request();
            input.refresh_token = Zeroizing::new(value.into());
            assert_eq!(input.validate(now).unwrap_err(), AuthorizationError::InvalidTokens);
        }
        let mut input = request();
        input.expires_at = 1000;
        assert_eq!(input.validate(now).unwrap_err(), AuthorizationError::InvalidTokens);
        let body = serde_json::json!({"operation_id":"26c4dfba-fda5-490d-9d46-d88da680f51d", "access_token":"a", "refresh_token":"b", "expires_at":87400, "app_key":"another-app"});
        assert!(serde_json::from_value::<JdVopTokenConfiguration>(body).is_err());
    }

    #[test]
    fn manual_status_is_sanitized_reports_expiry_and_does_not_claim_account_verification() {
        let crypto = crypto();
        let input = request();
        let now = Instant::from_unix_secs(1000);
        let payload = crypto
            .replacement(
                input.access_token.to_string(),
                input.refresh_token.to_string(),
                input.validate(now).unwrap(),
                now,
            )
            .unwrap();
        let record = SupplierCredential::configured(
            crypto.id.clone(),
            crypto.binding.clone(),
            payload,
            &SupplierCredentialConfiguration {
                operation_id: input.operation_id.clone(),
                actor_id: "operator".into(),
                expected_version: None,
            },
            now,
        )
        .unwrap();
        let status = status::operation_projection(&record, &crypto, now, Some(&input.operation_id));
        assert_eq!(status.state, "configured");
        assert!(!status.can_authorize);
        assert_eq!(status.expires_at, Some(87_400));
        assert_eq!(status.operation_state, Some("completed"));
        let json = serde_json::to_string(&status).unwrap();
        for secret in
            ["access-sentinel", "refresh-sentinel", &record.encrypted_payload, &record.payload_fingerprint]
        {
            assert!(!json.contains(secret));
        }
        assert_eq!(status::projection(&record, &crypto, Instant::from_unix_secs(87_400)).state, "expired");
        let mut refreshing = record;
        refreshing
            .claim_refresh(
                1,
                &SupplierCredentialRefreshClaim {
                    owner_id: "worker".into(),
                    operation_id: "refresh".into(),
                    now,
                    lease_seconds: 30,
                },
            )
            .unwrap();
        assert_eq!(status::projection(&refreshing, &crypto, now).state, "refreshing");
    }
}
