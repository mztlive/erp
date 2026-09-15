//! 公开选品链接令牌：哈希查找、密文复制、禁止明文落库。

use aes_gcm::aead::{Aead, Generate, Nonce};
use aes_gcm::{Aes256Gcm, KeyInit};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use erp_core::{Error, Result};
use sha2::{Digest, Sha256};

use super::limits::LINK_TOKEN_BYTES;

const CIPHERTEXT_VERSION: &str = "v1";

/// 由启动密钥派生、仅驻留内存的令牌编解码器。
#[derive(Clone)]
pub struct LinkTokenCrypto {
    key: [u8; 32],
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
        Self { key: derive_key(secret, b"erp-sales-selection-link-token-v1") }
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
    fn encrypt(&self, token: &str) -> Result<String> {
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
    use super::{LinkTokenCrypto, token_hash};

    #[test]
    fn issue_round_trip_and_hash_lookup() {
        let crypto = LinkTokenCrypto::from_secret(b"test-secret-at-least-32-bytes!!");
        let (token, hash, ciphertext) = crypto.issue().unwrap();
        assert_eq!(token.len(), 64);
        assert_eq!(hash, token_hash(&token));
        assert_eq!(crypto.decrypt(&ciphertext).unwrap(), token);
        assert!(!ciphertext.contains(&token));
    }
}
