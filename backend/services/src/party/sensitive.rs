//! D07 敏感值加密、查询指纹与短时揭示令牌。
//!
//! 运行时从启动密钥派生相互隔离的 AES-256-GCM 加密密钥、查询 HMAC 密钥和
//! 揭示令牌签名密钥。密钥只驻留在内存中，不进入实体、DTO、日志或 Debug。

use aes_gcm::{
    aead::{Aead, Generate, Nonce},
    Aes256Gcm, KeyInit,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::errors::{Error, Result};

const CIPHERTEXT_VERSION: &str = "v1";
const NONCE_LEN: usize = 12;
type HmacSha256 = Hmac<Sha256>;

/// 可揭示的敏感字段类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SensitiveFieldKind {
    /// 联系人手机号。
    ContactMobile,
    /// 履约地址。
    Address,
    /// 银行账号。
    BankAccountNumber,
}

/// 已验签的短时揭示范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SensitiveRevealScope {
    /// 字段类型。
    pub kind: SensitiveFieldKind,
    /// 敏感事实行 ID。
    pub record_id: String,
    /// 所属供应商 ID。
    pub supplier_id: String,
    /// 令牌过期时间（秒级 Unix 时间戳）。
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct RevealClaims {
    kind: SensitiveFieldKind,
    record_id: String,
    supplier_id: String,
    expires_at: u64,
}

/// 启动期构造、进程内共享的敏感数据编解码器。
#[derive(Clone)]
pub struct SensitiveDataCodec {
    encryption_key: [u8; 32],
    fingerprint_key: [u8; 32],
    token_key: [u8; 32],
}

impl SensitiveDataCodec {
    /// 从应用启动密钥派生用途隔离的运行时密钥。
    ///
    /// 调用方必须在启动期创建一次并注入所有 Service；进程运行期间不得切换，
    /// 否则既有密文与查询指纹会变得不可用。
    pub fn from_secret(secret: &[u8]) -> Self {
        Self {
            encryption_key: derive_key(secret, b"erp-sensitive-encryption-v1"),
            fingerprint_key: derive_key(secret, b"erp-sensitive-fingerprint-v1"),
            token_key: derive_key(secret, b"erp-sensitive-reveal-token-v1"),
        }
    }

    /// 返回只读查询指纹密钥。
    ///
    /// 该密钥仅传给实体构造函数计算 HMAC，不得持久化或输出。
    pub fn fingerprint_key(&self) -> &[u8] {
        &self.fingerprint_key
    }

    /// 使用 AES-256-GCM 加密明文并返回带版本的 URL-safe 编码。
    ///
    /// # Errors
    /// 随机数源或 AEAD 加密失败时返回内部错误。
    pub fn encrypt(&self, plaintext: &str) -> Result<String> {
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|_| Error::Internal("敏感数据加密初始化失败".to_string()))?;
        let nonce = Nonce::<Aes256Gcm>::try_generate()
            .map_err(|_| Error::Internal("敏感数据随机数生成失败".to_string()))?;
        let ciphertext = cipher
            .encrypt(&nonce, plaintext.as_bytes())
            .map_err(|_| Error::Internal("敏感数据加密失败".to_string()))?;
        Ok(format!(
            "{CIPHERTEXT_VERSION}.{}.{}",
            URL_SAFE_NO_PAD.encode(nonce.as_slice()),
            URL_SAFE_NO_PAD.encode(ciphertext)
        ))
    }

    /// 解密由当前版本编码器生成的密文。
    ///
    /// # Errors
    /// 密文为空、格式非法、版本未知、认证失败或密钥不匹配时返回校验错误。
    pub fn decrypt(&self, encoded: &str) -> Result<String> {
        let (nonce, ciphertext) = parse_ciphertext(encoded)?;
        let cipher = Aes256Gcm::new_from_slice(&self.encryption_key)
            .map_err(|_| Error::Internal("敏感数据解密初始化失败".to_string()))?;
        let nonce = Nonce::<Aes256Gcm>::try_from(nonce.as_slice())
            .map_err(|_| Error::ValidationError("敏感数据格式非法".to_string()))?;
        let plaintext = cipher
            .decrypt(&nonce, ciphertext.as_ref())
            .map_err(|_| Error::ValidationError("敏感数据不可用或密钥不匹配".to_string()))?;
        String::from_utf8(plaintext).map_err(|_| Error::ValidationError("敏感数据编码非法".to_string()))
    }

    /// 签发限定字段、事实行和供应商的短时揭示令牌。
    ///
    /// `expires_at` 由调用方统一控制；令牌只表达访问范围，最终仍必须经过 HTTP
    /// RBAC 权限校验并记录揭示审计。
    pub fn issue_reveal_token(
        &self,
        kind: SensitiveFieldKind,
        record_id: impl Into<String>,
        supplier_id: impl Into<String>,
        expires_at: u64,
    ) -> Result<String> {
        let claims = RevealClaims {
            kind,
            record_id: record_id.into(),
            supplier_id: supplier_id.into(),
            expires_at,
        };
        let payload =
            serde_json::to_vec(&claims).map_err(|_| Error::Internal("敏感字段令牌序列化失败".to_string()))?;
        let signature = sign(&self.token_key, &payload)?;
        Ok(format!(
            "{}.{}",
            URL_SAFE_NO_PAD.encode(payload),
            URL_SAFE_NO_PAD.encode(signature)
        ))
    }

    /// 验证短时揭示令牌并返回受限访问范围。
    ///
    /// # Errors
    /// 令牌格式、签名或过期时间非法时返回校验错误。
    pub fn verify_reveal_token(&self, token: &str, now: u64) -> Result<SensitiveRevealScope> {
        let (payload, signature) = token
            .split_once('.')
            .ok_or_else(|| Error::ValidationError("敏感字段令牌格式非法".to_string()))?;
        let payload = URL_SAFE_NO_PAD
            .decode(payload)
            .map_err(|_| Error::ValidationError("敏感字段令牌格式非法".to_string()))?;
        let signature = URL_SAFE_NO_PAD
            .decode(signature)
            .map_err(|_| Error::ValidationError("敏感字段令牌格式非法".to_string()))?;
        verify_signature(&self.token_key, &payload, &signature)?;
        let claims: RevealClaims = serde_json::from_slice(&payload)
            .map_err(|_| Error::ValidationError("敏感字段令牌格式非法".to_string()))?;
        if claims.expires_at < now {
            return Err(Error::ValidationError("敏感字段查看权限已失效".to_string()));
        }
        Ok(SensitiveRevealScope {
            kind: claims.kind,
            record_id: claims.record_id,
            supplier_id: claims.supplier_id,
            expires_at: claims.expires_at,
        })
    }
}

/// 使用用途标签从启动密钥派生固定长度子密钥。
fn derive_key(secret: &[u8], context: &[u8]) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(context);
    digest.update([0]);
    digest.update(secret);
    digest.finalize().into()
}

/// 解析版本化密文并校验 nonce 长度。
fn parse_ciphertext(encoded: &str) -> Result<([u8; NONCE_LEN], Vec<u8>)> {
    let mut segments = encoded.split('.');
    if segments.next() != Some(CIPHERTEXT_VERSION) {
        return Err(Error::ValidationError("敏感数据版本不受支持".to_string()));
    }
    let nonce = segments
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .ok_or_else(|| Error::ValidationError("敏感数据格式非法".to_string()))?;
    let ciphertext = segments
        .next()
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok())
        .ok_or_else(|| Error::ValidationError("敏感数据格式非法".to_string()))?;
    if segments.next().is_some() || nonce.len() != NONCE_LEN {
        return Err(Error::ValidationError("敏感数据格式非法".to_string()));
    }
    let mut nonce_bytes = [0_u8; NONCE_LEN];
    nonce_bytes.copy_from_slice(&nonce);
    Ok((nonce_bytes, ciphertext))
}

/// 为令牌正文计算 HMAC-SHA256。
fn sign(key: &[u8], payload: &[u8]) -> Result<Vec<u8>> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key)
        .map_err(|_| Error::Internal("敏感字段令牌签名初始化失败".to_string()))?;
    mac.update(payload);
    Ok(mac.finalize().into_bytes().to_vec())
}

/// 常量时间校验令牌签名。
fn verify_signature(key: &[u8], payload: &[u8], signature: &[u8]) -> Result<()> {
    let mut mac = <HmacSha256 as KeyInit>::new_from_slice(key)
        .map_err(|_| Error::Internal("敏感字段令牌验签初始化失败".to_string()))?;
    mac.update(payload);
    mac.verify_slice(signature)
        .map_err(|_| Error::ValidationError("敏感字段令牌签名非法".to_string()))
}

#[cfg(test)]
mod tests {
    use super::{SensitiveDataCodec, SensitiveFieldKind};

    #[test]
    fn ciphertext_roundtrip_uses_random_nonce() {
        let codec = SensitiveDataCodec::from_secret(b"test-secret-at-least-32-bytes-long");
        let first = codec.encrypt("13800138000").unwrap();
        let second = codec.encrypt("13800138000").unwrap();
        assert_ne!(first, second);
        assert_eq!(codec.decrypt(&first).unwrap(), "13800138000");
        assert_eq!(codec.decrypt(&second).unwrap(), "13800138000");
    }

    #[test]
    fn reveal_token_rejects_expiry_and_tampering() {
        let codec = SensitiveDataCodec::from_secret(b"test-secret-at-least-32-bytes-long");
        let token = codec
            .issue_reveal_token(SensitiveFieldKind::ContactMobile, "contact-1", "supplier-1", 200)
            .unwrap();
        let scope = codec.verify_reveal_token(&token, 100).unwrap();
        assert_eq!(scope.record_id, "contact-1");
        assert!(codec.verify_reveal_token(&token, 201).is_err());
        assert!(codec.verify_reveal_token(&format!("{token}x"), 100).is_err());
    }
}
