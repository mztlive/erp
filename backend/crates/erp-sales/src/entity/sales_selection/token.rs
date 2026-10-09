//! 公开选品链接令牌：哈希查找、密文复制、禁止明文落库。

use aes_gcm::aead::{Aead, Generate, Nonce};
use aes_gcm::{Aes256Gcm, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use erp_core::{Error, Result};
use hmac::{Hmac, Mac};
use serde::Serialize;
use sha2::{Digest, Sha256};

use super::idempotency::IdempotencyOperation;
use super::limits::LINK_TOKEN_BYTES;

const CIPHERTEXT_VERSION: &str = "v1";

/// 由启动密钥派生、仅驻留内存的令牌编解码器。
#[derive(Clone)]
pub struct LinkTokenCrypto {
    key: [u8; 32],
    fingerprint_key: [u8; 32],
}

/// 含敏感字段请求的用途隔离指纹；只能由应用密钥编解码器生成。
#[derive(Clone)]
pub struct SelectionRequestFingerprint(String);

impl SelectionRequestFingerprint {
    /// 返回可持久化的 HMAC 指纹，不包含请求明文或无密钥密码摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回指纹的版本标记与十六进制 HMAC。
    ///
    /// # 错误
    /// 无。
    pub(crate) fn as_str(&self) -> &str {
        &self.0
    }
}

impl LinkTokenCrypto {
    /// 从应用启动密钥派生选品链接加密密钥。
    ///
    /// # 参数
    /// * `secret` - 配置中的应用密钥，不得与令牌密文同库保存
    ///
    /// # 返回
    /// 返回进程内编解码器。
    ///
    /// # 错误
    /// 无。
    pub fn from_secret(secret: &[u8]) -> Self {
        Self {
            key: derive_key(secret, b"erp-sales-selection-link-token-v1"),
            fingerprint_key: derive_key(secret, b"erp-sales-selection-request-fingerprint-v1"),
        }
    }

    /// 为完整敏感请求生成稳定幂等指纹。
    ///
    /// # 参数
    /// `operation` 为幂等操作域，`payload` 为包含原密码的完整请求及其资源身份。
    ///
    /// # 返回
    /// 返回应用密钥与操作域共同绑定的 HMAC-SHA256 指纹。
    ///
    /// # 错误
    /// 请求编码或 HMAC 初始化失败时拒绝，不包含敏感载荷。
    pub fn request_fingerprint<T: Serialize>(
        &self,
        operation: IdempotencyOperation,
        payload: &T,
    ) -> Result<SelectionRequestFingerprint> {
        let encoded = serde_json::to_vec(payload).map_err(|_| Error::from("选品请求指纹编码失败"))?;
        let mut mac = <Hmac<Sha256> as hmac::KeyInit>::new_from_slice(&self.fingerprint_key)
            .map_err(|_| Error::from("选品请求指纹初始化失败"))?;
        mac.update(operation.as_str().as_bytes());
        mac.update(&[0]);
        mac.update(&encoded);
        Ok(SelectionRequestFingerprint(format!(
            "hmac-sha256-v1:{}",
            hex::encode(mac.finalize().into_bytes())
        )))
    }

    /// 生成至少 128 位安全随机令牌、查找哈希与密文。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `(明文令牌, SHA-256 十六进制哈希, 密文)`。明文不得落库。
    ///
    /// # 错误
    /// 随机数或加密失败时返回内部错误。
    pub fn issue(&self) -> Result<(String, String, String)> {
        let raw = random_bytes();
        let token = hex::encode(raw);
        let hash = token_hash(&token);
        let ciphertext = self.encrypt(&token)?;
        Ok((token, hash, ciphertext))
    }

    /// 解密内部复制接口使用的令牌密文。
    ///
    /// # 参数
    /// * `ciphertext` - 本编解码器产出的密文
    ///
    /// # 返回
    /// 返回明文令牌。
    ///
    /// # 错误
    /// 密文损坏或密钥不匹配时返回内部错误，不泄漏明文。
    pub fn decrypt(&self, ciphertext: &str) -> Result<String> {
        let rest = ciphertext
            .strip_prefix(&format!("{CIPHERTEXT_VERSION}."))
            .ok_or_else(|| Error::from("选品链接密文无效"))?;
        let (nonce_part, body) = rest.split_once('.').ok_or_else(|| Error::from("选品链接密文无效"))?;
        let nonce_bytes = URL_SAFE_NO_PAD.decode(nonce_part).map_err(|_| Error::from("选品链接密文无效"))?;
        let body_bytes = URL_SAFE_NO_PAD.decode(body).map_err(|_| Error::from("选品链接密文无效"))?;
        let cipher =
            Aes256Gcm::new_from_slice(&self.key).map_err(|_| Error::from("选品链接加密初始化失败"))?;
        let nonce = Nonce::<Aes256Gcm>::try_from(nonce_bytes.as_slice())
            .map_err(|_| Error::from("选品链接密文无效"))?;
        let plain =
            cipher.decrypt(&nonce, body_bytes.as_ref()).map_err(|_| Error::from("选品链接密文无效"))?;
        String::from_utf8(plain).map_err(|_| Error::from("选品链接密文无效"))
    }

    /// 加密明文令牌。
    ///
    /// # 参数
    /// * `token` - 明文令牌
    ///
    /// # 返回
    /// 返回带版本的密文。
    ///
    /// # 错误
    /// 加密失败时返回内部错误。
    pub(super) fn encrypt(&self, token: &str) -> Result<String> {
        let cipher =
            Aes256Gcm::new_from_slice(&self.key).map_err(|_| Error::from("选品链接加密初始化失败"))?;
        let nonce = Nonce::<Aes256Gcm>::try_generate().map_err(|_| Error::from("选品链接随机数生成失败"))?;
        let ciphertext =
            cipher.encrypt(&nonce, token.as_bytes()).map_err(|_| Error::from("选品链接加密失败"))?;
        Ok(format!(
            "{CIPHERTEXT_VERSION}.{}.{}",
            URL_SAFE_NO_PAD.encode(nonce.as_slice()),
            URL_SAFE_NO_PAD.encode(ciphertext)
        ))
    }
}

/// 计算令牌查找哈希。
///
/// # 参数
/// * `token` - 明文令牌
///
/// # 返回
/// 返回 SHA-256 十六进制。
///
/// # 错误
/// 无。
pub fn token_hash(token: &str) -> String {
    hex::encode(Sha256::digest(token.as_bytes()))
}

/// 用 SHA-256 派生 32 字节密钥。
///
/// # 参数
/// * `secret` - 启动密钥
/// * `info` - 用途隔离标签
///
/// # 返回
/// 返回 32 字节密钥。
///
/// # 错误
/// 无。
fn derive_key(secret: &[u8], info: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(secret);
    hasher.update(info);
    hasher.finalize().into()
}

/// 生成 32 字节安全随机数。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回随机字节。
///
/// # 错误
/// 无。UUID v4 两次拼接提供超过 128 位熵。
fn random_bytes() -> [u8; LINK_TOKEN_BYTES] {
    let first = uuid::Uuid::new_v4();
    let second = uuid::Uuid::new_v4();
    let mut bytes = [0_u8; LINK_TOKEN_BYTES];
    bytes[..16].copy_from_slice(first.as_bytes());
    bytes[16..].copy_from_slice(second.as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{LinkTokenCrypto, token_hash};
    use crate::entity::sales_selection::IdempotencyOperation;

    #[test]
    fn issue_round_trip_and_hash_lookup() {
        let crypto = LinkTokenCrypto::from_secret(b"test-secret-at-least-32-bytes!!");
        let (token, hash, ciphertext) = crypto.issue().unwrap();
        assert_eq!(token.len(), 64);
        assert_eq!(hash, token_hash(&token));
        assert_eq!(crypto.decrypt(&ciphertext).unwrap(), token);
        assert!(!ciphertext.contains(&token));
    }

    #[test]
    fn sensitive_fingerprint_is_stable_and_separates_key_operation_and_payload() {
        let crypto = LinkTokenCrypto::from_secret(b"application-secret-one");
        let other_crypto = LinkTokenCrypto::from_secret(b"application-secret-two");
        let payload = json!({"access_password": "selection-password", "idempotency_key": "same-key"});
        let operation = IdempotencyOperation::Create;
        let first = crypto.request_fingerprint(operation, &payload).unwrap();
        let replay = crypto.request_fingerprint(operation, &payload).unwrap();
        assert_eq!(first.as_str(), replay.as_str());
        assert!(first.as_str().starts_with("hmac-sha256-v1:"));
        assert_ne!(first.as_str(), other_crypto.request_fingerprint(operation, &payload).unwrap().as_str());
        assert_ne!(
            first.as_str(),
            crypto.request_fingerprint(IdempotencyOperation::SetAccessPassword, &payload).unwrap().as_str()
        );
        let changed = json!({"access_password": "changed-password", "idempotency_key": "same-key"});
        assert_ne!(first.as_str(), crypto.request_fingerprint(operation, &changed).unwrap().as_str());
    }
}
