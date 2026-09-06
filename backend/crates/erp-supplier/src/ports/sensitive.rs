//! Consumer port for party-owned sensitive-field reveal tokens.

use crate::error::{Error, Result};

/// 敏感字段种类快照；与主体 `SensitiveFieldKind` 稳定代码对齐。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SensitiveFieldKindFact {
    /// 联系人手机号。
    ContactMobile,
    /// 履约地址。
    Address,
    /// 银行账号。
    BankAccountNumber,
}

/// 签发短时揭示令牌的消费方端口。
///
/// 编解码与密钥留在 `erp-party`；供应商只提交字段种类、事实行与供应商 ID。
pub trait SensitiveTokenPort: Send + Sync {
    /// 签发受字段、事实行和供应商约束的短时令牌。
    ///
    /// # Parameters
    /// * `kind` - 敏感字段种类
    /// * `record_id` - 事实行 ID
    /// * `supplier_id` - 供应商角色 ID
    /// * `expires_at` - 过期 Unix 秒
    ///
    /// # Errors
    /// 令牌序列化或签名失败时返回内部错误。
    fn issue_reveal_token(
        &self,
        kind: SensitiveFieldKindFact,
        record_id: &str,
        supplier_id: &str,
        expires_at: u64,
    ) -> Result<String>;
}

/// Empty token issuer used when详情不签发揭示令牌。
#[derive(Debug, Default, Clone, Copy)]
pub struct EmptySensitiveTokens;

impl SensitiveTokenPort for EmptySensitiveTokens {
    fn issue_reveal_token(
        &self,
        _kind: SensitiveFieldKindFact,
        _record_id: &str,
        _supplier_id: &str,
        _expires_at: u64,
    ) -> Result<String> {
        Err(Error::Internal("未配置敏感字段令牌能力".to_string()))
    }
}
