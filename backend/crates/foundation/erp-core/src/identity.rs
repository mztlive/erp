//! 稳定操作人类别。
//!
//! 仅包含跨域共用的账号种类；完整 AccountCore 仍留在身份领域。

use std::str::FromStr;

use serde::{Deserialize, Serialize};

/// 无法从字符串解析账号类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidAccountKind;

impl std::fmt::Display for InvalidAccountKind {
    // 输出不含原值的账号类型错误，避免把未知输入写进日志。
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

    /// 为跨领域记录和比较提供稳定的账号类型代码，而非展示标签。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => Self::ADMIN_CODE,
            Self::Supplier => Self::SUPPLIER_CODE,
        }
    }

    /// 从持久化或入口代码识别后台管理员与供应商门户账号。大小写和空白均须精确匹配；
    /// 未知值返回 [`InvalidAccountKind`]。
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

    // 为接收 `&str` 的转换入口复用 `AccountKind::parse`；未知代码仍返回 `InvalidAccountKind`。
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::parse(value)
    }
}

impl FromStr for AccountKind {
    type Err = InvalidAccountKind;

    // 让标准字符串解析入口沿用稳定账号代码的精确匹配；不裁剪、不忽略大小写，未知代码返回 `InvalidAccountKind`。
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
