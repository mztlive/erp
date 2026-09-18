//! 客户端提交的证据记录引用：精确 `type:id`。

use std::fmt;

use erp_core::{Error, Result};

use super::parse::{parse_id, parse_kind};

/// 客户端提交的证据记录引用：精确 `type:id`。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvidenceRecordRef {
    kind: String,
    id: String,
}

impl EvidenceRecordRef {
    /// 由已校验的对象类型与记录 ID 构造客户端引用。
    ///
    /// # 参数
    /// * `kind` - 对象类型（如 `inbox_message`）
    /// * `id` - 记录身份 ID
    ///
    /// # 返回
    /// 返回可编码为 `type:id` 的引用。
    ///
    /// # 错误
    /// 类型或 ID 为空、超长、含分隔符或嵌套冒号时返回领域校验错误。
    ///
    /// # 约束
    /// 不接受 `type://id` 或带版本/状态的 canonical 形态。
    pub fn new(kind: &str, id: &str) -> Result<Self> {
        Ok(Self {
            kind: parse_kind(kind.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
            id: parse_id(id.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
        })
    }

    /// 解析客户端证据记录 ID。
    ///
    /// # 参数
    /// * `value` - 原始记录 ID 字符串
    ///
    /// # 返回
    /// 返回去除首尾空白后的 `type:id` 引用。
    ///
    /// # 错误
    /// 缺少冒号、空段、嵌套 ID 或分隔符注入时返回领域校验错误。
    ///
    /// # 约束
    /// 只允许恰好一段类型和一段 ID；额外 `:` 一律拒绝。
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let (kind, id) =
            value.split_once(':').ok_or_else(|| Error::from("证据记录 ID 必须使用 type:id 格式"))?;
        if id.contains(':') {
            return Err(Error::from("证据记录 ID 必须使用唯一的 type:id 格式"));
        }
        Self::new(kind, id)
    }

    /// 返回对象类型。
    ///
    /// # 返回
    /// 返回已规范化的类型代码。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 只读访问，不重新解析。
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// 返回记录身份 ID。
    ///
    /// # 返回
    /// 返回已规范化的身份 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不包含类型、版本或状态段。
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for EvidenceRecordRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.kind, self.id)
    }
}

#[cfg(test)]
mod tests {
    use super::EvidenceRecordRef;

    #[test]
    fn record_ref_accepts_type_id_and_rejects_nested_or_empty() {
        let parsed = EvidenceRecordRef::parse(" inbox_message:message-1 ").unwrap();
        assert_eq!(parsed.kind(), "inbox_message");
        assert_eq!(parsed.id(), "message-1");
        assert_eq!(parsed.to_string(), "inbox_message:message-1");
        assert!(EvidenceRecordRef::parse("message-1").is_err());
        assert!(EvidenceRecordRef::parse("inbox_message:").is_err());
        assert!(EvidenceRecordRef::parse(":message-1").is_err());
        assert!(EvidenceRecordRef::parse("   ").is_err());
        assert!(EvidenceRecordRef::parse("inbox_message:message-1:forged").is_err());
    }

    #[test]
    fn record_ref_rejects_delimiter_injection() {
        for injected in [
            "inbox_message:message-1;forged",
            "inbox_message:message-1,forged",
            "inbox_message:message-1|forged",
            "inbox_message:message-1/forged",
            "inbox_message:message-1=forged",
            "inbox_message:message 1",
        ] {
            assert!(EvidenceRecordRef::parse(injected).is_err(), "{injected} 应拒绝分隔符注入");
        }
    }
}
