//! 稳定操作人类别。
//!
//! 仅包含跨域共用的账号种类；完整 AccountCore 仍留在身份领域。

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
}

impl AccountKind {
    /// 返回账号类型字符串表示。
    ///
    /// # 返回值
    /// 返回稳定的账号类型字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Admin => "admin",
        }
    }

    /// 从字符串解析账号类型。
    ///
    /// # 参数
    /// * `value` - 账号类型字符串
    ///
    /// # 返回值
    /// 成功返回账号类型。
    ///
    /// # 错误
    /// 未知取值返回 [`InvalidAccountKind`]。
    pub fn parse(value: &str) -> std::result::Result<Self, InvalidAccountKind> {
        match value {
            "admin" => Ok(Self::Admin),
            _ => Err(InvalidAccountKind),
        }
    }
}

impl TryFrom<&str> for AccountKind {
    type Error = InvalidAccountKind;

    /// 从字符串解析账号类型。
    ///
    /// # 参数
    /// * `value` - 账号类型字符串
    ///
    /// # 返回值
    /// 成功返回账号类型。
    ///
    /// # 错误
    /// 未知取值返回 [`InvalidAccountKind`]。
    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
        Self::parse(value)
    }
}
