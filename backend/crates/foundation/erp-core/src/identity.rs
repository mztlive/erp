//! 稳定操作人类别。
//!
//! 仅包含跨域共用的账号种类；完整 AccountCore 仍留在身份领域。

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// 无法从字符串解析账号类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAccountKind;

impl std::fmt::Display for InvalidAccountKind {
    /// 输出稳定的解析失败说明。
    ///
    /// # 参数
    /// * `f` - 格式化器
    ///
    /// # 返回
    /// 格式化成功返回 `Ok`，写入失败返回格式化错误。
    ///
    /// # 错误
    /// 仅在底层 `fmt` 写入失败时返回错误。
    ///
    /// # 约束
    /// 文案不得携带输入原值，避免把未知枚举写进日志。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("未知的账号类型")
    }
}

impl std::error::Error for InvalidAccountKind {}

/// 后台账号种类。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum AccountKind {
    /// 系统管理员账号。
    Admin,
    /// 独立供应商门户账号，不具备后台身份。
    Supplier,
}

impl AccountKind {
    /// 管理员账号的稳定代码（唯一真相源，`as_str`/`parse` 均由此驱动）。
    pub const ADMIN_CODE: &'static str = "admin";
    /// 供应商门户账号的稳定代码。
    pub const SUPPLIER_CODE: &'static str = "supplier";

    /// 返回账号类型字符串表示。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回稳定的账号类型字符串。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => Self::ADMIN_CODE,
            Self::Supplier => Self::SUPPLIER_CODE,
        }
    }

    /// 从字符串解析账号类型。
    ///
    /// # 参数
    /// * `value` - 账号类型字符串
    ///
    /// # 返回
    /// 成功返回账号类型。
    ///
    /// # 错误
    /// 未知取值返回 [`InvalidAccountKind`]。
    pub fn parse(value: &str) -> std::result::Result<Self, InvalidAccountKind> {
        if value == Self::ADMIN_CODE {
            return Ok(Self::Admin);
        }
        if value == Self::SUPPLIER_CODE {
            return Ok(Self::Supplier);
        }
        Err(InvalidAccountKind)
    }
}

impl TryFrom<&str> for AccountKind {
    type Error = InvalidAccountKind;

    /// 从字符串解析账号类型。
    ///
    /// # 参数
    /// * `value` - 账号类型字符串
    ///
    /// # 返回
    /// 成功返回账号类型。
    ///
    /// # 错误
    /// 未知取值返回 [`InvalidAccountKind`]。
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl FromStr for AccountKind {
    type Err = InvalidAccountKind;

    /// 按账号稳定代码解析，保持既有转换的大小写及错误语义。
    ///
    /// # 参数
    /// * `value` - 账号类型的稳定代码。
    /// # 返回
    /// 返回对应账号类型。
    /// # 错误
    /// 未知代码返回 [`InvalidAccountKind`]。
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::parse(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{AccountKind, InvalidAccountKind};

    #[test]
    fn standard_string_parsing_preserves_exact_account_codes() {
        assert_eq!("admin".parse::<AccountKind>(), Ok(AccountKind::Admin));
        assert_eq!("supplier".parse::<AccountKind>(), Ok(AccountKind::Supplier));
        for invalid in ["", "Admin", " admin", "supplier ", "customer"] {
            assert_eq!(invalid.parse::<AccountKind>(), Err(InvalidAccountKind));
        }
        assert_eq!(AccountKind::try_from("admin"), Ok(AccountKind::Admin));
    }
}
