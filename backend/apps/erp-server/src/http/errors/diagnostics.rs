use std::sync::OnceLock;

use jwt_sha2::{Digest, Sha256};
use uuid::Uuid;

use super::Error;

/// 错误诊断仅携带固定分类与单向关联标识，不复制错误正文或输入值。
pub(super) struct Diagnostic {
    pub(super) kind: &'static str,
    pub(super) category: &'static str,
    pub(super) fingerprint: String,
}

impl Diagnostic {
    /// 从边界错误提取安全诊断。
    ///
    /// # 参数
    /// `error` 为当前请求错误；原始内容只用于内存内分类和摘要计算。
    /// # 返回
    /// 固定类别与单向摘要，供关联同类服务故障使用。
    /// # 错误
    /// 无。
    pub(super) fn from_error(error: &Error) -> Self {
        let kind = match error {
            Error::Internal(_) => "internal",
            Error::BadRequest(_) => "bad_request",
            Error::NotFound(_) => "not_found",
            Error::Conflict(_) => "conflict",
            Error::Unprocessable(_) => "business_rule",
            Error::Forbidden(_) => "forbidden",
            Error::Unauthorized(_) => "unauthorized",
            Error::OutcomeUnknown(_) => "outcome_unknown",
            Error::RateLimited(_) => "rate_limit",
            Error::Logic(_) => "domain_rule",
            Error::Validation(_) => "validation",
            Error::InvalidInput(_) => "invalid_input",
            Error::Coded(_) => "coded",
        };
        let (category, fingerprint) = match error {
            Error::Internal(message) => (internal_category(message), digest(message)),
            Error::OutcomeUnknown(source) => ("commit_outcome_unknown", digest(&source.to_string())),
            // 校验和业务错误不计算输入摘要，避免为账号、密码等低熵值留下可枚举标识。
            _ => (kind, String::new()),
        };
        Self { kind, category, fingerprint }
    }
}

/// 分类只用于诊断，不改变 HTTP 错误码、业务判断或重试政策。
fn internal_category(message: &str) -> &'static str {
    let normalized = message.to_ascii_lowercase();
    let categories: &[(&[&str], &str)] = &[
        (&["timed out", "timeout", "超时"], "timeout"),
        (&["connection refused", "connection reset", "broken pipe"], "connection"),
        (&["deserialize", "decode", "bson", "反序列化"], "decode"),
        (&["serialize", "encode", "序列化"], "encode"),
        (&["database", "mongodb", "数据库"], "database"),
        (&["storage", "s3", "存储"], "storage"),
        (&["jwt", "signing", "签名"], "authentication"),
    ];
    categories
        .iter()
        .find(|(terms, _)| terms.iter().any(|term| normalized.contains(term)))
        .map_or("unclassified", |(_, category)| *category)
}

/// 使用不落日志的进程随机盐，避免通过枚举错误文本反查敏感值。
fn digest(message: &str) -> String {
    static SALT: OnceLock<Uuid> = OnceLock::new();
    let mut digest = Sha256::new();
    digest.update(SALT.get_or_init(Uuid::new_v4).as_bytes());
    digest.update(message.as_bytes());
    format!("{:x}", digest.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_diagnostics_keep_category_without_sensitive_text() {
        let error =
            Error::Internal("database timeout mongodb://admin:secret@host/user phone=13800138000".into());
        let diagnostic = Diagnostic::from_error(&error);
        assert_eq!(diagnostic.kind, "internal");
        assert_eq!(diagnostic.category, "timeout");
        assert_eq!(diagnostic.fingerprint.len(), 64);
        assert!(diagnostic.fingerprint.chars().all(|character| character.is_ascii_hexdigit()));
        assert_eq!(diagnostic.fingerprint, Diagnostic::from_error(&error).fingerprint);
        assert_ne!(
            diagnostic.fingerprint,
            Diagnostic::from_error(&Error::Internal("other".into())).fingerprint
        );
    }

    #[test]
    fn validation_and_credentials_do_not_leave_value_fingerprints() {
        for error in [Error::BadRequest("password=secret".into()), Error::Unauthorized("user=admin".into())] {
            assert!(Diagnostic::from_error(&error).fingerprint.is_empty());
        }
        let diagnostic = Diagnostic::from_error(&Error::Internal("private detail".into()));
        assert_eq!(diagnostic.category, "unclassified");
    }
}
