//! 加密封套同时绑定凭据身份、账号、应用及有效期，阻止跨文档调换密文。
use config::JdVopConfig;
use erp_core::common::time::Instant;
use erp_integration::entity::supplier_credential::{
    SupplierCredential, SupplierCredentialBinding, SupplierCredentialPayload, SupplierCredentialProvider,
};
use erp_party::SensitiveDataCodec;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::jd_vop::token::{JdVopAccessToken, token_error};
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;
use subtle::ConstantTimeEq;

use super::JdVopCredentialImport;

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CredentialPlaintext {
    schema_version: u32,
    credential_id: String,
    account_scope: String,
    app_key: String,
    pub(super) access_token: String,
    pub(super) refresh_token: String,
    pub(super) expires_at: i64,
    pub(super) refresh_after: i64,
}

#[derive(Clone)]
pub(super) struct CredentialCrypto {
    pub(super) id: String,
    pub(super) binding: SupplierCredentialBinding,
    codec: SensitiveDataCodec,
}

impl CredentialCrypto {
    pub(super) fn identity_fingerprint(&self, kind: &str, value: &str) -> ConnectorResult<String> {
        let body =
            serde_json::to_vec(&(&self.id, &self.binding.account_scope, &self.binding.app_key, kind, value))
                .map_err(|_| token_error())?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.codec.fingerprint_key()).map_err(|_| token_error())?;
        mac.update(b"erp-jd-vop-authorized-identity-v1\0");
        mac.update(&body);
        Ok(hex::encode(mac.finalize().into_bytes()))
    }
    pub(super) fn new(settings: &JdVopConfig) -> ConnectorResult<Self> {
        settings.validate_credential_settings().map_err(|_| token_error())?;
        Ok(Self {
            id: settings.credential_id.clone(),
            binding: SupplierCredentialBinding {
                provider: SupplierCredentialProvider::JdVop,
                account_scope: settings.account_scope.clone(),
                app_key: settings.app_key.clone(),
            },
            codec: SensitiveDataCodec::from_secret(settings.credential_encryption_key.as_bytes()),
        })
    }

    pub(super) fn seal(
        &self,
        input: JdVopCredentialImport,
        now: Instant,
    ) -> ConnectorResult<SupplierCredentialPayload> {
        if input.credential_id != self.id
            || input.account_scope != self.binding.account_scope
            || input.app_key != self.binding.app_key
            || !valid_token(&input.refresh_token)
        {
            return Err(token_error());
        }
        let expires_at = Instant::try_from_unix_secs(input.expires_at).map_err(|_| token_error())?;
        JdVopAccessToken::new(
            input.access_token.clone(),
            "validation".into(),
            input.account_scope.clone(),
            expires_at,
        )?;
        if expires_at <= now {
            return Err(token_error());
        }
        let plain = CredentialPlaintext {
            schema_version: 1,
            credential_id: input.credential_id,
            account_scope: input.account_scope,
            app_key: input.app_key,
            access_token: input.access_token,
            refresh_token: input.refresh_token,
            expires_at: input.expires_at,
            refresh_after: refresh_after(now, expires_at)?,
        };
        let body = serde_json::to_string(&plain).map_err(|_| token_error())?;
        let payload_fingerprint = self.fingerprint(&plain)?;
        let encrypted_payload = self.codec.encrypt(&body).map_err(|_| token_error())?;
        Ok(SupplierCredentialPayload { encrypted_payload, payload_fingerprint, expires_at })
    }

    pub(super) fn open(&self, record: &SupplierCredential) -> ConnectorResult<CredentialPlaintext> {
        record.require_binding(&self.binding).map_err(|_| token_error())?;
        if record.base.id != self.id {
            return Err(token_error());
        }
        let body = self.codec.decrypt(&record.encrypted_payload).map_err(|_| token_error())?;
        let plain: CredentialPlaintext = serde_json::from_str(&body).map_err(|_| token_error())?;
        if plain.schema_version != 1
            || plain.credential_id != self.id
            || plain.account_scope != record.account_scope
            || plain.app_key != record.app_key
            || plain.expires_at != record.expires_at.unix_secs()
            || plain.refresh_after >= plain.expires_at
            || !valid_token(&plain.access_token)
            || !valid_token(&plain.refresh_token)
            || !bool::from(self.fingerprint(&plain)?.as_bytes().ct_eq(record.payload_fingerprint.as_bytes()))
        {
            return Err(token_error());
        }
        Ok(plain)
    }

    fn fingerprint(&self, plain: &CredentialPlaintext) -> ConnectorResult<String> {
        let body = serde_json::to_vec(&(
            1,
            &plain.credential_id,
            &plain.account_scope,
            &plain.app_key,
            &plain.access_token,
            &plain.refresh_token,
            plain.expires_at,
        ))
        .map_err(|_| token_error())?;
        let mut mac =
            Hmac::<Sha256>::new_from_slice(self.codec.fingerprint_key()).map_err(|_| token_error())?;
        mac.update(b"erp-jd-vop-credential-payload-v1\0");
        mac.update(&body);
        Ok(hex::encode(mac.finalize().into_bytes()))
    }

    pub(super) fn replacement(
        &self,
        access_token: String,
        refresh_token: String,
        expires_at: Instant,
        now: Instant,
    ) -> ConnectorResult<SupplierCredentialPayload> {
        self.seal(
            JdVopCredentialImport {
                credential_id: self.id.clone(),
                account_scope: self.binding.account_scope.clone(),
                app_key: self.binding.app_key.clone(),
                access_token,
                refresh_token,
                expires_at: expires_at.unix_secs(),
            },
            now,
        )
    }
}

fn valid_token(value: &str) -> bool {
    !value.is_empty() && value.len() <= 8192 && value.trim() == value && !value.chars().any(char::is_control)
}

fn refresh_after(now: Instant, expires_at: Instant) -> ConnectorResult<i64> {
    let remaining = expires_at.unix_secs().checked_sub(now.unix_secs()).ok_or_else(token_error)?;
    let interval = remaining.saturating_sub(300).max(remaining / 2).clamp(0, 8 * 3600).min(remaining - 1);
    now.unix_secs().checked_add(interval).ok_or_else(token_error)
}
