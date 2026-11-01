//! 密码验证与绑定个人会话的公开访问凭证。

use argon2::Argon2;
use argon2::password_hash::rand_core::OsRng;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use erp_core::common::time::Instant;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{LinkTokenCrypto, SalesSelectionBooklet, SalesSelectionSession, token_hash};

const ACCESS_SECONDS: i64 = 12 * 60 * 60;

/// 密文内的授权事实；不持久化，不记录到日志。
#[derive(Clone, Serialize, Deserialize)]
pub struct SelectionGrant {
    booklet_id: String,
    token_version: u32,
    password_fingerprint: String,
    /// 个人会话身份。
    pub session_id: String,
    expires_at: i64,
}

impl SelectionGrant {
    /// 在已通过密码验证后签发个人访问凭证。
    ///
    /// # 参数
    /// * `book` - 当前有效册
    /// * `session` - 经提货码确认的个人会话或共享会话
    /// * `now` - 服务端时间
    /// * `crypto` - 应用密钥派生的编解码器
    ///
    /// # 返回
    /// 返回不可篡改的密文访问凭证。
    ///
    /// # 错误
    /// 未设置密码、会话不属于本册或加密失败时拒绝。
    pub fn issue(
        book: &SalesSelectionBooklet,
        session: &SalesSelectionSession,
        now: Instant,
        crypto: &LinkTokenCrypto,
    ) -> Result<String> {
        if session.booklet_id.as_ref() != book.base.id {
            return Err(Error::from("选品授权无效"));
        }
        let hash =
            book.access_password_hash.as_deref().ok_or_else(|| Error::from("选品册尚未设置访问密码"))?;
        let expires_at =
            now.unix_secs().checked_add(ACCESS_SECONDS).ok_or_else(|| Error::from("授权时间无效"))?;
        let grant = Self {
            booklet_id: book.base.id.clone(),
            token_version: book.link_token_version,
            password_fingerprint: token_hash(hash),
            session_id: session.base.id.clone(),
            expires_at: book.link_expires_at.map_or(expires_at, |at| at.unix_secs().min(expires_at)),
        };
        let encoded = serde_json::to_string(&grant).map_err(|_| Error::from("选品授权签发失败"))?;
        crypto.encrypt(&encoded)
    }

    /// 解码访问凭证；随后必须按当前册重验。
    ///
    /// # 参数
    /// * `encoded` - 密文凭证
    /// * `crypto` - 应用编解码器
    ///
    /// # 返回
    /// 返回授权事实。
    ///
    /// # 错误
    /// 凭证损坏、过长或密钥不匹配时拒绝。
    pub fn decode(encoded: &str, crypto: &LinkTokenCrypto) -> Result<Self> {
        if encoded.len() > 4096 {
            return Err(Error::from("选品授权无效"));
        }
        let json = crypto.decrypt(encoded).map_err(|_| Error::from("选品授权无效"))?;
        serde_json::from_str(&json).map_err(|_| Error::from("选品授权无效"))
    }

    /// 以数据库当前事实重验凭证。
    ///
    /// # 参数
    /// * `book` - 当前册
    /// * `session` - 当前会话
    /// * `now` - 服务端时间
    ///
    /// # 返回
    /// 所有绑定仍有效时成功。
    ///
    /// # 错误
    /// 换链、改密、跨册、跨会话或到期时拒绝。
    pub fn ensure_current(
        &self,
        book: &SalesSelectionBooklet,
        session: &SalesSelectionSession,
        now: Instant,
    ) -> Result<()> {
        let hash = book.access_password_hash.as_deref().ok_or_else(|| Error::from("请先输入选品密码"))?;
        if self.booklet_id != book.base.id
            || self.token_version != book.link_token_version
            || self.password_fingerprint != token_hash(hash)
            || self.session_id != session.base.id
            || session.booklet_id.as_ref() != book.base.id
            || now.unix_secs() >= self.expires_at
        {
            return Err(Error::from("选品授权已失效，请重新输入密码"));
        }
        Ok(())
    }
}

/// 使用 Argon2id 为访问密码生成慢哈希。
///
/// # 参数
/// * `password` - 8 至 64 字符的原始密码
///
/// # 返回
/// 返回带盐哈希；调用方必须放在阻塞任务中执行。
///
/// # 错误
/// 长度非法或哈希失败时拒绝。
pub fn hash_selection_password(password: &str) -> Result<String> {
    if !(8..=64).contains(&password.chars().count()) {
        return Err(Error::from("访问密码须为8至64个字符"));
    }
    Argon2::default()
        .hash_password(password.as_bytes(), &SaltString::generate(&mut OsRng))
        .map(|value| value.to_string())
        .map_err(|_| Error::from("访问密码设置失败"))
}

/// 验证访问密码，损坏哈希失败关闭。
///
/// # 参数
/// * `password` - 输入密码
/// * `hash` - 数据库密码哈希
///
/// # 返回
/// 匹配返回真；调用方必须放在阻塞任务中执行。
///
/// # 错误
/// 无，非法输入和损坏哈希返回假。
pub fn verify_selection_password(password: &str, hash: &str) -> bool {
    if !(8..=64).contains(&password.chars().count()) {
        return false;
    }
    PasswordHash::new(hash)
        .is_ok_and(|value| Argon2::default().verify_password(password.as_bytes(), &value).is_ok())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SalesSelectionBookletId;

    use super::*;

    #[test]
    fn password_hash_is_salted_and_fails_closed() {
        let hash = hash_selection_password("selection-password").unwrap();
        assert!(verify_selection_password("selection-password", &hash));
        assert!(!verify_selection_password("incorrect-password", &hash));
        assert!(!verify_selection_password("selection-password", "damaged"));
        assert_ne!(hash, hash_selection_password("selection-password").unwrap());
        assert!(hash_selection_password("short").is_err());
        assert!(hash_selection_password(&"a".repeat(65)).is_err());
    }
    fn fixtures() -> (SalesSelectionBooklet, SalesSelectionSession, LinkTokenCrypto, Instant) {
        use erp_core::ids::{CustomerAccountId, SalesSelectionBookletId, SalesSelectionSessionId};

        use crate::entity::sales_selection::{
            PoolFilterSnapshot, PoolSource, PoolSourceKind, SalesSelectionBookletData, SelectionForm,
            SubmitMode,
        };

        let book = SalesSelectionBooklet::new(
            SalesSelectionBookletId::new("book-1"),
            SalesSelectionBookletData {
                customer_id: CustomerAccountId::new("customer-1"),
                customer_no: "C1".into(),
                customer_name: "客户甲".into(),
                sales_owner_user_id: "sales-1".into(),
                business_org_unit_id: "org-1".into(),
                form: SelectionForm::SingleSku,
                submit_mode: SubmitMode::ByQuantity,
                access_password_hash: Some("salted-password-hash".into()),
                per_person_budget: None,
                voucher_count: None,
                pool_source: PoolSource::new(
                    PoolSourceKind::Filter,
                    Some(PoolFilterSnapshot::default()),
                    None,
                )
                .unwrap(),
                tiers: Vec::new(),
                created_by: "user-1".into(),
            },
        )
        .unwrap();
        let session = SalesSelectionSession::new(
            SalesSelectionSessionId::new("session-1"),
            SalesSelectionBookletId::new("book-1"),
        );
        (book, session, LinkTokenCrypto::from_secret(b"selection-test-key"), Instant::from_unix_secs(100))
    }

    #[test]
    fn grant_roundtrip_binds_current_booklet_and_session() {
        let (book, session, crypto, now) = fixtures();
        let encoded = SelectionGrant::issue(&book, &session, now, &crypto).unwrap();
        assert!(!encoded.contains("session-1"));
        assert!(!encoded.contains("salted-password-hash"));
        let grant = SelectionGrant::decode(&encoded, &crypto).unwrap();
        assert_eq!(grant.session_id, "session-1");
        assert!(grant.ensure_current(&book, &session, now).is_ok());
        assert!(SelectionGrant::decode(&encoded, &LinkTokenCrypto::from_secret(b"other-key")).is_err());
        assert!(SelectionGrant::decode("v1.invalid.invalid", &crypto).is_err());
        assert!(SelectionGrant::decode(&"a".repeat(4097), &crypto).is_err());
    }

    #[test]
    fn grant_rejects_changed_password_link_booklet_and_session() {
        let (book, session, crypto, now) = fixtures();
        let encoded = SelectionGrant::issue(&book, &session, now, &crypto).unwrap();
        let grant = SelectionGrant::decode(&encoded, &crypto).unwrap();
        let mut changed = book.clone();
        changed.access_password_hash = Some("new-salted-password-hash".into());
        assert!(grant.ensure_current(&changed, &session, now).is_err());
        changed.access_password_hash = None;
        assert!(grant.ensure_current(&changed, &session, now).is_err());
        changed = book.clone();
        changed.link_token_version += 1;
        assert!(grant.ensure_current(&changed, &session, now).is_err());
        changed = book.clone();
        changed.base.id = "book-2".into();
        assert!(grant.ensure_current(&changed, &session, now).is_err());
        let mut other_session = session.clone();
        other_session.base.id = "session-2".into();
        assert!(grant.ensure_current(&book, &other_session, now).is_err());
        other_session = session.clone();
        other_session.booklet_id = SalesSelectionBookletId::new("book-2");
        assert!(grant.ensure_current(&book, &other_session, now).is_err());
        assert!(SelectionGrant::issue(&book, &other_session, now, &crypto).is_err());
    }

    #[test]
    fn grant_expiry_is_shortened_to_link_deadline_and_rejects_boundary() {
        let (mut book, session, crypto, now) = fixtures();
        book.link_expires_at = Some(Instant::from_unix_secs(150));
        let encoded = SelectionGrant::issue(&book, &session, now, &crypto).unwrap();
        let grant = SelectionGrant::decode(&encoded, &crypto).unwrap();
        assert_eq!(grant.expires_at, 150);
        assert!(grant.ensure_current(&book, &session, Instant::from_unix_secs(149)).is_ok());
        assert!(grant.ensure_current(&book, &session, Instant::from_unix_secs(150)).is_err());
        book.link_expires_at = None;
        let encoded = SelectionGrant::issue(&book, &session, now, &crypto).unwrap();
        let grant = SelectionGrant::decode(&encoded, &crypto).unwrap();
        assert_eq!(grant.expires_at, now.unix_secs() + ACCESS_SECONDS);
        assert!(grant.ensure_current(&book, &session, Instant::from_unix_secs(grant.expires_at)).is_err());
    }

    #[test]
    fn grant_missing_password_and_invalid_json_fail_closed() {
        let (mut book, session, crypto, now) = fixtures();
        book.access_password_hash = None;
        assert!(SelectionGrant::issue(&book, &session, now, &crypto).is_err());
        let encrypted = crypto.encrypt("{invalid json}").unwrap();
        assert!(SelectionGrant::decode(&encrypted, &crypto).is_err());
        let encrypted = crypto.encrypt(r#"{"session_id":"s1","expires_at":9223372036854775808}"#).unwrap();
        assert!(SelectionGrant::decode(&encrypted, &crypto).is_err());
    }
}
