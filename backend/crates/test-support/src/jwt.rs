//! `mint_jwt`：签发与 web-api 完全同构的后台 JWT。

use std::collections::BTreeMap;

use chrono::Utc;
use jwt::{Claims, RegisteredClaims, SignWithKey};
use jwt_hmac::{Hmac, Mac};
use jwt_sha2::Sha256;
use serde_json::Value;

use crate::seed::{seed_login, ACCOUNT_VERSION};
use crate::{Error, Result};

/// JWT 密钥最小字节数（与 web-api `JwtEngine::new` 一致）。
const MIN_SECRET_LEN: usize = 32;

/// 为种子账号签发后台 JWT。
///
/// 复制 `apps/web-api/src/core/auth/jwt.rs` 的 token 结构与 HMAC-SHA256
/// 签名算法（标准声明 `sub`/`exp` + 私有声明 `id`/`account`/
/// `subject_kind`/`account_kind`/`account_version`）。本 crate 不依赖
/// web-api，只能复制格式；密钥必须与构造 AppState 时使用的 `app.secret`
/// 完全一致，否则 `authenticate` 验签失败。
///
/// # 参数
/// * `account_id` - 种子账号 ID（`seed_admin_account` 的返回值）
/// * `secret` - 与 web-api 配置一致的 JWT 密钥（至少 32 字节）
/// * `exp_seconds` - token 有效期（秒）
///
/// # 返回值
/// 返回可直接放入 `Authorization: Bearer <token>` 的 JWT。
///
/// # 错误
/// 密钥过短、HMAC 初始化失败或签名失败时返回错误。
pub fn mint_jwt(account_id: &str, secret: &str, exp_seconds: u64) -> Result<String> {
    if secret.len() < MIN_SECRET_LEN {
        return Err(Error::JwtSecretTooShort);
    }
    let key = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;
    let expiration = u64::try_from(Utc::now().timestamp()).unwrap_or_default() + exp_seconds;
    let mut claims = Claims::new(RegisteredClaims {
        subject: Some(account_id.to_string()),
        expiration: Some(expiration),
        ..Default::default()
    });
    claims.private = private_claims(account_id);
    Ok(claims.sign_with_key(&key)?)
}

/// 构造与 `TokenPayload::backoffice` 序列化结果一致的私有声明。
///
/// # 参数
/// * `account_id` - 种子账号 ID
///
/// # 返回值
/// 返回按稳定顺序排布的 JWT 私有声明映射。
fn private_claims(account_id: &str) -> BTreeMap<String, Value> {
    let mut claims = BTreeMap::new();
    claims.insert("id".to_string(), Value::String(account_id.to_string()));
    claims.insert("account".to_string(), Value::String(seed_login(account_id)));
    claims.insert(
        "subject_kind".to_string(),
        Value::String("backoffice".to_string()),
    );
    claims.insert("account_kind".to_string(), Value::String("admin".to_string()));
    claims.insert("account_version".to_string(), Value::from(ACCOUNT_VERSION));
    claims
}

#[cfg(test)]
mod tests {
    use super::mint_jwt;

    const TEST_SECRET: &str = "test-secret-that-is-at-least-32-bytes-long";

    #[test]
    fn minted_jwt_should_have_three_parts() {
        let token = mint_jwt("acc-1234567890ab", TEST_SECRET, 3600).unwrap();
        assert_eq!(token.split('.').count(), 3);
    }

    #[test]
    fn mint_jwt_should_reject_short_secret() {
        assert!(matches!(
            mint_jwt("acc-1234567890ab", "too-short", 3600),
            Err(super::Error::JwtSecretTooShort)
        ));
    }
}
