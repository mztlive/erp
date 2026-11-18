//! `mint_jwt`：签发与 web-api 完全同构的后台 JWT。

use std::collections::BTreeMap;

use chrono::Utc;
use jwt::{Claims, RegisteredClaims, SignWithKey};
use jwt_hmac::{Hmac, Mac};
use jwt_sha2::Sha256;
use serde_json::Value;

use crate::seed::{ACCOUNT_VERSION, seed_login};
use crate::{Error, Result};

/// JWT 密钥最小字节数（与 web-api `JwtEngine::new` 一致）。
const MIN_SECRET_LEN: usize = 32;

/// 用种子管理员的账号 ID 和以秒计的有效期签发后台 JWT，供路由测试作为 Bearer 凭据使用。
/// 不依赖 Web API，但 `sub`、`exp` 及私有声明 `id`、`account`、`subject_kind`、`account_kind`、
/// `account_version` 必须与 `apps/erp-server/src/http/auth/jwt.rs` 同步。
/// HMAC-SHA256 密钥须与测试应用的 `app.secret` 一致，且至少 32 字节；密钥过短、HMAC 初始化失败
/// 或签名失败时返回错误。
pub fn mint_jwt(account_id: &str, secret: &str, exp_seconds: u64) -> Result<String> {
    if secret.len() < MIN_SECRET_LEN {
        return Err(Error::JwtSecretTooShort);
    }
    let key = Hmac::<Sha256>::new_from_slice(secret.as_bytes())?;

    // 1970 年前的时间戳没有合法的 `exp` 语义，显式饱和到 0 再加有效期，
    // 使边界行为可读（原 `unwrap_or_default` 隐含了同一语义）。
    let now_secs = Utc::now().timestamp().max(0) as u64;
    let expiration = now_secs + exp_seconds;
    let mut claims = Claims::new(RegisteredClaims {
        subject: Some(account_id.to_string()),
        expiration: Some(expiration),
        ..Default::default()
    });
    // 私有声明须与 `TokenPayload::backoffice` 的序列化结果一致。
    claims.private = BTreeMap::from([
        ("id".to_string(), Value::String(account_id.to_string())),
        ("account".to_string(), Value::String(seed_login(account_id))),
        ("subject_kind".to_string(), Value::String("backoffice".to_string())),
        ("account_kind".to_string(), Value::String("admin".to_string())),
        ("account_version".to_string(), Value::from(ACCOUNT_VERSION)),
    ]);

    Ok(claims.sign_with_key(&key)?)
}

#[cfg(test)]
mod tests {
    use jwt::VerifyWithKey;
    use serde_json::json;

    use super::{Claims, Hmac, Mac, Sha256, mint_jwt};

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
