use std::borrow::Cow;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// 权限文本解析错误；不依赖身份设施或业务错误类型。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ParsePermissionError {
    #[error("权限必须使用 resource:action 格式")]
    InvalidFormat,
    #[error("权限只能包含一个冒号分隔符")]
    ExtraSeparator,
    #[error("{0}不能为空")]
    Empty(&'static str),
    #[error("{0}长度不能超过128个字符")]
    TooLong(&'static str),
    #[error("{0}不能包含斜杠")]
    Slash(&'static str),
    #[error("{0}包含非法字符")]
    InvalidCharacters(&'static str),
}

/// 已规范化的 `resource:action` 权限。常量借用静态文本，动态输入拥有规范化文本。
#[derive(Clone, Debug, Eq, PartialEq, Hash, Ord, PartialOrd)]
pub struct Permission {
    resource: Cow<'static, str>,
    action: Cow<'static, str>,
}

impl Permission {
    /// 使用已经规范化的静态文本定义内置权限。
    ///
    /// # 参数
    /// resource 为资源标识，action 为动作；均须小写且无首尾空白。
    /// # 返回
    /// 借用静态文本的权限，可用于 const 初始化。
    /// # 错误
    /// 不返回错误；非法常量在编译期失败。
    /// # Panics
    /// 在运行时调用时，非法静态文本触发 panic；动态输入必须使用 parse。
    pub const fn new(resource: &'static str, action: &'static str) -> Self {
        assert!(validate_part(resource, "权限资源", true).is_ok(), "非法的内置权限资源");
        assert!(validate_part(action, "权限动作", false).is_ok(), "非法的内置权限动作");
        Self { resource: Cow::Borrowed(resource), action: Cow::Borrowed(action) }
    }

    /// 解析权限文本，去除整体首尾空白并转换为 ASCII 小写。
    ///
    /// # 参数
    /// value 为外部 resource:action 文本。
    /// # 返回
    /// 已规范化且通过校验的权限。
    /// # 错误
    /// 格式错误、空值、部分超过 128 字节或含非法字符时返回解析错误。
    pub fn parse(value: impl AsRef<str>) -> Result<Self, ParsePermissionError> {
        let normalized = value.as_ref().trim().to_ascii_lowercase();
        let (resource, action) = normalized.split_once(':').ok_or(ParsePermissionError::InvalidFormat)?;
        if action.contains(':') {
            return Err(ParsePermissionError::ExtraSeparator);
        }
        validate_part(resource, "权限资源", true)?;
        validate_part(action, "权限动作", false)?;
        Ok(Self { resource: Cow::Owned(resource.into()), action: Cow::Owned(action.into()) })
    }

    /// 取得资源标识。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已规范化的资源文本。
    /// # 错误
    /// 不返回错误。
    pub fn resource(&self) -> &str {
        &self.resource
    }

    /// 取得动作标识。
    /// # 参数
    /// 无。
    /// # 返回
    /// 已规范化的动作文本。
    /// # 错误
    /// 不返回错误。
    pub fn action(&self) -> &str {
        &self.action
    }

    /// 判断当前权限是否覆盖目标权限。
    /// # 参数
    /// required 为需要检查的目标权限。
    /// # 返回
    /// 资源与动作分别相等或当前部分为完整通配符时为 true。
    /// # 错误
    /// 不返回错误。
    pub fn covers(&self, required: &Self) -> bool {
        (self.resource == "*" || self.resource == required.resource)
            && (self.action == "*" || self.action == required.action)
    }
}

/// 常量与动态输入共用的格式约束；斜杠只分隔非空资源路径段。
const fn validate_part(
    value: &str,
    label: &'static str,
    allow_slash: bool,
) -> Result<(), ParsePermissionError> {
    let bytes = value.as_bytes();
    if bytes.is_empty() {
        return Err(ParsePermissionError::Empty(label));
    }
    if bytes.len() > 128 {
        return Err(ParsePermissionError::TooLong(label));
    }
    if bytes.len() == 1 && bytes[0] == b'*' {
        return Ok(());
    }
    let mut index = 0;
    while index < bytes.len() {
        let ch = bytes[index];
        if ch == b'/' {
            if !allow_slash {
                return Err(ParsePermissionError::Slash(label));
            }
            if index == 0 || index + 1 == bytes.len() || bytes[index - 1] == b'/' {
                return Err(ParsePermissionError::InvalidCharacters(label));
            }
        } else if !ch.is_ascii_lowercase() && !ch.is_ascii_digit() && ch != b'_' && ch != b'-' {
            return Err(ParsePermissionError::InvalidCharacters(label));
        }
        index += 1;
    }
    Ok(())
}

impl fmt::Display for Permission {
    /// 输出稳定的 resource:action 编码。
    /// # 参数
    /// formatter 为目标格式化器。
    /// # 返回
    /// 文本写入结果。
    /// # 错误
    /// 目标写入失败时返回格式化错误。
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.resource, self.action)
    }
}

impl Serialize for Permission {
    /// 将权限编码为字符串。
    /// # 参数
    /// serializer 为目标序列化器。
    /// # 返回
    /// resource:action 文本的编码结果。
    /// # 错误
    /// 序列化器拒绝写入时返回其错误。
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for Permission {
    /// 在解码边界解析并校验权限。
    /// # 参数
    /// deserializer 为输入反序列化器。
    /// # 返回
    /// 已规范化的权限值。
    /// # 错误
    /// 非字符串或非法权限文本返回解码错误。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Self::parse(String::deserialize(deserializer)?).map_err(serde::de::Error::custom)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeSet, HashSet};

    use super::*;

    const UPDATE: Permission = Permission::new("sales_inquiry", "update");

    #[test]
    fn static_and_dynamic_permissions_have_the_same_identity_and_serialization() {
        let dynamic = Permission::parse(" Sales_Inquiry:UPDATE ").unwrap();
        assert_eq!(UPDATE, dynamic);
        assert_eq!(UPDATE.cmp(&dynamic), std::cmp::Ordering::Equal);
        assert_eq!(HashSet::from([UPDATE, dynamic.clone()]).len(), 1);
        assert_eq!(BTreeSet::from([UPDATE, dynamic.clone()]).len(), 1);
        assert_eq!(serde_json::to_string(&UPDATE).unwrap(), "\"sales_inquiry:update\"");
        assert_eq!(serde_json::to_string(&dynamic).unwrap(), serde_json::to_string(&UPDATE).unwrap());
        assert_eq!(serde_json::from_str::<Permission>("\" Sales_Inquiry:UPDATE \"").unwrap(), UPDATE);
        assert!(matches!(UPDATE.resource, Cow::Borrowed(_)));
        assert!(matches!(dynamic.resource, Cow::Owned(_)));
    }

    #[test]
    fn parsing_preserves_boundaries_and_reports_specific_errors() {
        for (raw, expected) in [
            ("a", ParsePermissionError::InvalidFormat),
            ("a:b:c", ParsePermissionError::ExtraSeparator),
            (":b", ParsePermissionError::Empty("权限资源")),
            ("a:", ParsePermissionError::Empty("权限动作")),
            ("a:b/c", ParsePermissionError::Slash("权限动作")),
            ("a//b:c", ParsePermissionError::InvalidCharacters("权限资源")),
            ("/a:b", ParsePermissionError::InvalidCharacters("权限资源")),
            ("a/:b", ParsePermissionError::InvalidCharacters("权限资源")),
            ("a :b", ParsePermissionError::InvalidCharacters("权限资源")),
            ("a:b*", ParsePermissionError::InvalidCharacters("权限动作")),
            ("中文:b", ParsePermissionError::InvalidCharacters("权限资源")),
        ] {
            assert_eq!(Permission::parse(raw), Err(expected), "{raw}");
            assert!(serde_json::from_value::<Permission>(raw.into()).is_err());
        }
        let limit = "a".repeat(128);
        assert_eq!(Permission::parse(format!("{limit}:{limit}")).unwrap().action(), limit);
        for raw in [format!("{limit}a:b"), format!("a:{limit}b")] {
            assert!(matches!(Permission::parse(raw), Err(ParsePermissionError::TooLong(_))));
        }
        assert!(serde_json::from_str::<Permission>("{}").is_err());
    }

    #[test]
    fn wildcard_coverage_is_exact_and_does_not_expand_path_prefixes() {
        for (grant, required, expected) in [
            (Permission::new("*", "*"), UPDATE, true),
            (Permission::new("sales_inquiry", "*"), UPDATE, true),
            (Permission::new("*", "update"), UPDATE, true),
            (UPDATE, Permission::new("sales_inquiry", "*"), false),
            (UPDATE, Permission::new("customer", "update"), false),
            (Permission::new("a", "read"), Permission::new("a/b", "read"), false),
        ] {
            assert_eq!(grant.covers(&required), expected);
            assert_eq!(Permission::parse(grant.to_string()).unwrap().covers(&required), expected);
        }
        assert_eq!(Permission::new("a-b/c_1", "read").to_string(), "a-b/c_1:read");
    }

    #[test]
    fn static_constructor_rejects_unnormalized_or_invalid_literals() {
        for (resource, action) in [("A", "read"), (" a", "read"), ("a", "READ"), ("a", "b/c"), ("", "read")] {
            assert!(std::panic::catch_unwind(|| Permission::new(resource, action)).is_err());
        }
    }
}
