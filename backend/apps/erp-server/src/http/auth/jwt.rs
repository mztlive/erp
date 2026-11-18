use std::collections::BTreeMap;
use std::str::FromStr;

use chrono::{Duration, Utc};
use erp_core::AccountKind;
use jwt::{Claims, RegisteredClaims, SignWithKey, VerifyWithKey};
use jwt_hmac::digest::InvalidLength;
use jwt_hmac::{Hmac, Mac};
use jwt_sha2::Sha256;
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("jwt error: {0}")]
    Jwt(#[from] jwt::Error),

    #[error("jwt hmac error: {0}")]
    Hmac(#[from] InvalidLength),

    #[error("jwt token creation failed")]
    TokenCreationFailed,

    #[error("jwt secret is invalid")]
    InvalidSecret,

    #[error("jwt token claims are invalid")]
    InvalidClaims,

    #[error("jwt token has expired")]
    TokenExpired,
}

#[derive(Debug, Clone)]
pub struct Engine {
    key: Hmac<Sha256>,
}

/// JWT 主体类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubjectKind {
    Backoffice,
    SupplierPortal,
}

impl SubjectKind {
    /// 把已知主体类型编码成稳定的 JWT 声明值，后台与供应商门户使用不同取值。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Backoffice => "backoffice",
            Self::SupplierPortal => "supplier_portal",
        }
    }
}

impl TryFrom<&str> for SubjectKind {
    type Error = ();

    // 从字符串解析主体类型。
    // 成功返回主体类型，失败返回空错误。
    // 字符串不是精确的已知主体类型时返回空错误。
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::from_str(value)
    }
}

impl FromStr for SubjectKind {
    type Err = ();

    // 按精确字符串解析 JWT 主体类型。
    // 返回已知的后台或供应商门户主体类型。
    // 未知值、大小写变化或首尾空白均返回空错误。
    fn from_str(value: &str) -> std::result::Result<Self, Self::Err> {
        match value {
            "backoffice" => Ok(Self::Backoffice),
            "supplier_portal" => Ok(Self::SupplierPortal),
            _ => Err(()),
        }
    }
}

pub struct TokenPayload {
    pub id: String,
    pub account: String,
    pub subject_kind: SubjectKind,
    pub account_kind: Option<AccountKind>,
    pub account_version: Option<u64>,
    pub binding_version: Option<u64>,
}

impl TokenPayload {
    /// 为认证成功的后台账号构造签发载荷，将账号类型和当前持久化版本写入声明，供请求认证时复验。
    pub fn backoffice(id: String, account: String, account_kind: AccountKind, account_version: u64) -> Self {
        Self {
            id,
            account,
            subject_kind: SubjectKind::Backoffice,
            account_kind: Some(account_kind),
            account_version: Some(account_version),
            binding_version: None,
        }
    }

    /// 构造独立门户 JWT 载荷，带账号及绑定版本；请求使用时仍由当前绑定解析供应商归属。
    pub fn supplier_portal(id: String, account: String, account_version: u64, binding_version: u64) -> Self {
        Self {
            id,
            account,
            subject_kind: SubjectKind::SupplierPortal,
            account_kind: Some(AccountKind::Supplier),
            account_version: Some(account_version),
            binding_version: Some(binding_version),
        }
    }
}

impl From<TokenPayload> for BTreeMap<String, Value> {
    // 从 TokenPayload 生成 JWT 载荷映射。
    // 返回载荷映射。
    fn from(payload: TokenPayload) -> Self {
        let mut out: BTreeMap<String, Value> = BTreeMap::new();
        out.insert("id".to_string(), payload.id.into());
        out.insert("account".to_string(), payload.account.into());
        out.insert("subject_kind".to_string(), payload.subject_kind.as_str().into());
        if let Some(account_kind) = payload.account_kind {
            out.insert("account_kind".to_string(), account_kind.as_str().into());
        }
        if let Some(account_version) = payload.account_version {
            out.insert("account_version".to_string(), account_version.into());
        }
        if let Some(binding_version) = payload.binding_version {
            out.insert("binding_version".to_string(), binding_version.into());
        }

        out
    }
}

impl TryFrom<BTreeMap<String, Value>> for TokenPayload {
    type Error = Error;

    // 把验签后的声明解码为后台或门户身份；两类主体要求不同的账号类型与绑定版本，缺少账号版本的旧凭证不再有效。
    fn try_from(mut payload_map: BTreeMap<String, Value>) -> Result<Self, Self::Error> {
        let id = required_string(&mut payload_map, "id")?;
        let account = required_string(&mut payload_map, "account")?;
        let account_kind = match payload_map.remove("account_kind") {
            Some(Value::String(s)) => AccountKind::try_from(s.as_str()).ok(),
            _ => None,
        };
        let account_version = match payload_map.remove("account_version") {
            Some(Value::Number(value)) => value.as_u64(),
            _ => None,
        };

        let subject_kind = SubjectKind::try_from(required_string(&mut payload_map, "subject_kind")?.as_str())
            .map_err(|_| Error::InvalidClaims)?;
        let binding_version = payload_map.remove("binding_version").and_then(|value| value.as_u64());
        match (subject_kind, account_kind, account_version, binding_version) {
            (SubjectKind::Backoffice, Some(AccountKind::Admin), Some(_), None) => {},
            (SubjectKind::SupplierPortal, Some(AccountKind::Supplier), Some(_), Some(_)) => {},
            _ => return Err(Error::InvalidClaims),
        }
        Ok(TokenPayload { id, account, subject_kind, account_kind, account_version, binding_version })
    }
}

impl Engine {
    /// 为当前 JWT 密钥初始化签发与验签共用的 HMAC 引擎；密钥少于 32 字节或 HMAC 初始化失败时返回错误。
    pub fn new(secret: String) -> Result<Self, Error> {
        if secret.len() < 32 {
            return Err(Error::InvalidSecret);
        }
        let out = Self { key: Hmac::new_from_slice(secret.as_bytes())? };

        Ok(out)
    }

    /// 认证后用服务端身份载荷签发含标准主体和时效声明的 JWT；签名失败时不交付令牌。
    pub fn create_token<T: Into<TokenPayload>>(&self, payload: T) -> Result<String, Error> {
        let expiration =
            Utc::now().checked_add_signed(Duration::days(30)).ok_or(Error::TokenCreationFailed)?.timestamp();
        let expiration = u64::try_from(expiration).map_err(|_| Error::TokenCreationFailed)?;
        let infomation = payload.into();

        let mut claims = Claims::new(RegisteredClaims {
            subject: Some(infomation.id.clone()),
            expiration: Some(expiration),
            ..Default::default()
        });

        claims.private = infomation.into();

        let token = claims.sign_with_key(&self.key)?;

        Ok(token)
    }

    /// 在接收 JWT 时验证签名、时效和标准主体与私有载荷的一致性；必要声明缺失、过期或尚未生效时拒绝。
    pub fn verify_token(&self, token: &str) -> Result<TokenPayload, Error> {
        let now = u64::try_from(Utc::now().timestamp()).map_err(|_| Error::InvalidClaims)?;
        self.verify_token_at(token, now)
    }

    // 签名通过后按给定时间检查有效期、生效时间和主体一致性；显式时钟也用于覆盖边界测试。
    fn verify_token_at(&self, token: &str, now: u64) -> Result<TokenPayload, Error> {
        let claims: Claims = token.verify_with_key(&self.key)?;
        let subject =
            claims.registered.subject.filter(|value| !value.trim().is_empty()).ok_or(Error::InvalidClaims)?;
        let expiration = claims.registered.expiration.ok_or(Error::InvalidClaims)?;
        if expiration <= now {
            return Err(Error::TokenExpired);
        }
        if claims.registered.not_before.is_some_and(|not_before| not_before > now) {
            return Err(Error::InvalidClaims);
        }

        let payload = TokenPayload::try_from(claims.private)?;
        if payload.id != subject {
            return Err(Error::InvalidClaims);
        }

        Ok(payload)
    }
}

fn required_string(payload_map: &mut BTreeMap<String, Value>, field: &str) -> Result<String, Error> {
    match payload_map.remove(field) {
        Some(Value::String(value)) if !value.trim().is_empty() => Ok(value),
        _ => Err(Error::InvalidClaims),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_SECRET: &str = "test-secret-that-is-at-least-32-bytes";

    #[test]
    fn standard_subject_parsing_matches_try_from_and_keeps_exact_values() {
        for (raw, expected) in [
            ("backoffice", Ok(SubjectKind::Backoffice)),
            ("supplier_portal", Ok(SubjectKind::SupplierPortal)),
            ("", Err(())),
            ("Backoffice", Err(())),
            (" backoffice ", Err(())),
            ("supplier", Err(())),
        ] {
            assert_eq!(raw.parse::<SubjectKind>(), expected);
            assert_eq!(SubjectKind::try_from(raw), expected);
        }
    }

    #[test]
    fn supplier_token_round_trip_requires_external_kind_and_binding_version() {
        let engine = Engine::new(TEST_SECRET.into()).unwrap();
        let token = engine
            .create_token(TokenPayload::supplier_portal("supplier-user".into(), "supplier01".into(), 3, 5))
            .unwrap();
        let payload = engine.verify_token(&token).unwrap();
        assert_eq!(payload.subject_kind, SubjectKind::SupplierPortal);
        assert_eq!(payload.account_kind, Some(AccountKind::Supplier));
        assert_eq!(payload.binding_version, Some(5));
        let mut map: BTreeMap<String, Value> = payload.into();
        map.remove("binding_version");
        assert!(TokenPayload::try_from(map).is_err());
        let mut map: BTreeMap<String, Value> =
            TokenPayload::supplier_portal("s".into(), "supplier01".into(), 1, 1).into();
        map.insert("account_kind".into(), Value::String("admin".into()));
        assert!(TokenPayload::try_from(map).is_err());
    }

    #[test]
    fn supplier_account_cannot_be_encoded_as_backoffice_subject() {
        let map: BTreeMap<String, Value> =
            TokenPayload::backoffice("supplier-user".into(), "supplier01".into(), AccountKind::Supplier, 1)
                .into();
        assert!(TokenPayload::try_from(map).is_err());
    }

    // 用单测引擎签发自定义声明，让声明校验测试走真实验签路径。
    fn signed_token(engine: &Engine, registered: RegisteredClaims, payload: TokenPayload) -> String {
        let mut claims = Claims::new(registered);
        claims.private = payload.into();
        claims.sign_with_key(&engine.key).expect("token should be signed")
    }

    // 验证后台载荷会写入后台主体类型和账号类型字段。
    #[test]
    fn backoffice_payload_contains_subject_kind_and_account_kind() {
        let payload =
            TokenPayload::backoffice("admin_1".to_string(), "demo_admin".to_string(), AccountKind::Admin, 7);
        let payload_map: BTreeMap<String, Value> = payload.into();

        assert_eq!(payload_map.get("subject_kind"), Some(&Value::String("backoffice".to_string())));
        assert_eq!(payload_map.get("account_kind"), Some(&Value::String("admin".to_string())));
        assert_eq!(payload_map.get("account_version"), Some(&Value::from(7)));
    }

    // 覆盖认证后台令牌所需的账号类型声明，确认合法后台身份能解码。
    #[test]
    fn backoffice_payload_with_account_kind_should_be_accepted() {
        let mut payload_map = BTreeMap::new();
        payload_map.insert("id".to_string(), Value::String("admin_1".to_string()));
        payload_map.insert("account".to_string(), Value::String("demo_admin".to_string()));
        payload_map.insert("subject_kind".to_string(), Value::String("backoffice".to_string()));
        payload_map.insert("account_kind".to_string(), Value::String("admin".to_string()));
        payload_map.insert("account_version".to_string(), Value::from(1));

        let payload = TokenPayload::try_from(payload_map).expect("payload");

        assert_eq!(payload.account_kind, Some(AccountKind::Admin));
    }

    // 签名有效但缺少账号版本的旧后台令牌应失效，迫使客户端重新登录获取新版凭证。
    #[test]
    fn backoffice_payload_without_account_version_should_be_rejected() {
        let mut payload_map = BTreeMap::new();
        payload_map.insert("id".to_string(), Value::String("admin_1".to_string()));
        payload_map.insert("account".to_string(), Value::String("demo_admin".to_string()));
        payload_map.insert("subject_kind".to_string(), Value::String("backoffice".to_string()));
        payload_map.insert("account_kind".to_string(), Value::String("admin".to_string()));

        assert!(matches!(TokenPayload::try_from(payload_map), Err(Error::InvalidClaims)));
    }

    // 检查空白密钥不能初始化 JWT 引擎，避免在无有效密钥时签发或验签。
    #[test]
    fn blank_secret_should_be_rejected() {
        assert!(matches!(Engine::new("  ".to_string()), Err(Error::InvalidSecret)));
    }

    // 用真实引擎完成签发再验签，确认正常令牌能通过全部声明检查。
    #[test]
    fn created_token_should_be_verified() {
        let engine = Engine::new(TEST_SECRET.to_string()).expect("engine");
        let token = engine
            .create_token(TokenPayload::backoffice(
                "admin_1".to_string(),
                "demo_admin".to_string(),
                AccountKind::Admin,
                1,
            ))
            .expect("token");

        let payload = engine.verify_token(&token).expect("valid token");

        assert_eq!(payload.id, "admin_1");
        assert_eq!(payload.subject_kind, SubjectKind::Backoffice);
    }

    // 把校验时间推进到到期边界，确认签名正确的 JWT 也不能继续使用。
    #[test]
    fn expired_token_should_be_rejected() {
        let engine = Engine::new(TEST_SECRET.to_string()).expect("engine");
        let token = signed_token(
            &engine,
            RegisteredClaims {
                subject: Some("admin_1".to_string()),
                expiration: Some(100),
                ..Default::default()
            },
            TokenPayload::backoffice("admin_1".to_string(), "demo_admin".to_string(), AccountKind::Admin, 1),
        );

        assert!(matches!(engine.verify_token_at(&token, 100), Err(Error::TokenExpired)));
    }

    // 通过有效签名构造缺少 `exp` 的 JWT，确保签名通过不能替代时效校验。
    #[test]
    fn token_without_expiration_should_be_rejected() {
        let engine = Engine::new(TEST_SECRET.to_string()).expect("engine");
        let token = signed_token(
            &engine,
            RegisteredClaims { subject: Some("admin_1".to_string()), ..Default::default() },
            TokenPayload::backoffice("admin_1".to_string(), "demo_admin".to_string(), AccountKind::Admin, 1),
        );

        assert!(matches!(engine.verify_token_at(&token, 100), Err(Error::InvalidClaims)));
    }

    // 在生效时间之前验证已签发令牌，锁定 `nbf` 边界的拒绝行为。
    #[test]
    fn token_before_not_before_should_be_rejected() {
        let engine = Engine::new(TEST_SECRET.to_string()).expect("engine");
        let token = signed_token(
            &engine,
            RegisteredClaims {
                subject: Some("admin_1".to_string()),
                expiration: Some(200),
                not_before: Some(101),
                ..Default::default()
            },
            TokenPayload::backoffice("admin_1".to_string(), "demo_admin".to_string(), AccountKind::Admin, 1),
        );

        assert!(matches!(engine.verify_token_at(&token, 100), Err(Error::InvalidClaims)));
    }

    // 给已正确签名的令牌设置不同的标准主体和私有载荷身份，确认验签后仍拒绝错配。
    #[test]
    fn token_with_mismatched_subject_should_be_rejected() {
        let engine = Engine::new(TEST_SECRET.to_string()).expect("engine");
        let token = signed_token(
            &engine,
            RegisteredClaims {
                subject: Some("admin_2".to_string()),
                expiration: Some(200),
                ..Default::default()
            },
            TokenPayload::backoffice("admin_1".to_string(), "demo_admin".to_string(), AccountKind::Admin, 1),
        );

        assert!(matches!(engine.verify_token_at(&token, 100), Err(Error::InvalidClaims)));
    }
}
