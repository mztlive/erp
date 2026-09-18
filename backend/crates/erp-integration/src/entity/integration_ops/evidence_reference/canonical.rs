//! 规范化证据引用：身份加上可选版本与状态。

use std::fmt;

use erp_core::{Error, Result};

use super::parse::{ensure_encoded_len, parse_id, parse_kind, parse_status, parse_version_token};

/// 规范化证据引用：身份加上可选版本与状态。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalEvidenceReference {
    kind: String,
    id: String,
    version: Option<u64>,
    status: Option<String>,
}

impl CanonicalEvidenceReference {
    /// 构造已验证的 canonical 引用。
    ///
    /// # 参数
    /// * `kind` - 对象类型
    /// * `id` - 记录身份 ID
    /// * `version` - 正式版本；无版本形态传 `None`
    /// * `status` - 终态或核验状态段
    ///
    /// # 返回
    /// 返回可稳定编码的 canonical 引用。
    ///
    /// # 错误
    /// 任一段为空、超长或含分隔符时返回领域校验错误。
    ///
    /// # 约束
    /// 编码为 `type:id:status` 或 `type:id:vN:status`，不写历史 `://` 形态。
    pub fn verified(kind: &str, id: &str, version: Option<u64>, status: &str) -> Result<Self> {
        let reference = Self {
            kind: parse_kind(kind.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
            id: parse_id(id.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
            version,
            status: Some(parse_status(status.trim())?),
        };
        ensure_encoded_len(&reference.encode())?;
        Ok(reference)
    }

    /// 解析已持久化或历史证据引用。
    ///
    /// # 参数
    /// * `value` - 存量 `type://id`、`type:id`、`type:id:status` 或 `type:id:vN:status`
    ///
    /// # 返回
    /// 返回按精确 grammar 解析的引用；身份 ID 不含版本或状态。
    ///
    /// # 错误
    /// 空值、超长、分隔符注入、无法识别的段数或非法版本段时返回领域校验错误。
    ///
    /// # 约束
    /// 不把 `://` 之后的内容再按冒号切开；嵌套 ID 失败关闭。
    pub fn parse_stored(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() {
            return Err(Error::from("证据记录 ID 必须使用 type:id 格式"));
        }
        ensure_encoded_len(value)?;
        if let Some((kind, id)) = value.split_once("://") {
            return Ok(Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: None,
                status: None,
            });
        }
        let parts = value.split(':').collect::<Vec<_>>();
        let reference = match parts.as_slice() {
            [kind, id] => Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: None,
                status: None,
            },
            [kind, id, status] => Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: None,
                status: Some(parse_status(status)?),
            },
            [kind, id, version, status] => Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: Some(parse_version_token(version)?),
                status: Some(parse_status(status)?),
            },
            _ => return Err(Error::from("证据记录 ID 必须使用唯一的 type:id 格式")),
        };
        Ok(reference)
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
    /// 不包含 `://` 或版本前缀。
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// 返回身份 ID。
    ///
    /// # 返回
    /// 返回与主体关联比较使用的精确 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不含类型名、`vN` 或状态段，因此不会被错误 substring/token 命中。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 返回可选正式版本。
    ///
    /// # 返回
    /// 有版本段时返回版本号。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 历史 `://` 身份引用没有版本。
    pub fn version(&self) -> Option<u64> {
        self.version
    }

    /// 返回可选状态段。
    ///
    /// # 返回
    /// 有状态段时返回状态文本。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 状态不参与主体关联比较。
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// 消费引用并返回 canonical 编码。
    ///
    /// # 返回
    /// 返回可写入终态证据字段的稳定字符串。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不输出历史 `://` 形态；与 [`fmt::Display`] 编码规则相同。
    pub fn into_wire(self) -> String {
        self.encode()
    }

    /// 按冻结段序编码 canonical 文本。
    ///
    /// # 返回
    /// 返回 `type:id`、`type:id:status` 或 `type:id:vN:status`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 版本使用十进制无前导零（零本身为 `v0`）。
    fn encode(&self) -> String {
        match (self.version, self.status.as_deref()) {
            (Some(version), Some(status)) => {
                format!("{}:{}:v{}:{}", self.kind, self.id, version, status)
            },
            (None, Some(status)) => format!("{}:{}:{}", self.kind, self.id, status),
            (Some(version), None) => format!("{}:{}:v{}", self.kind, self.id, version),
            (None, None) => format!("{}:{}", self.kind, self.id),
        }
    }
}

impl fmt::Display for CanonicalEvidenceReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.encode())
    }
}

#[cfg(test)]
mod tests {
    use super::CanonicalEvidenceReference;

    #[test]
    fn stored_reference_parses_historical_scheme_and_canonical_forms() {
        let scheme = CanonicalEvidenceReference::parse_stored(" mall_order_fact://f-1001 ").unwrap();
        assert_eq!(scheme.kind(), "mall_order_fact");
        assert_eq!(scheme.id(), "f-1001");
        assert_eq!(scheme.version(), None);
        assert_eq!(scheme.status(), None);
        let hyphenated = CanonicalEvidenceReference::parse_stored("mall-snapshot:7").unwrap();
        assert_eq!(hyphenated.kind(), "mall-snapshot");
        assert_eq!(hyphenated.id(), "7");

        let verified =
            CanonicalEvidenceReference::parse_stored("inbox_message:message-1:v2:processed").unwrap();
        assert_eq!(verified.id(), "message-1");
        assert_eq!(verified.version(), Some(2));
        assert_eq!(verified.status(), Some("processed"));
        assert_eq!(verified.to_string(), "inbox_message:message-1:v2:processed");

        let reviewed =
            CanonicalEvidenceReference::parse_stored("reconciliation_difference_resolution:res-1:reviewed")
                .unwrap();
        assert_eq!(reviewed.id(), "res-1");
        assert_eq!(reviewed.status(), Some("reviewed"));
        assert_eq!(reviewed.to_string(), "reconciliation_difference_resolution:res-1:reviewed");
    }

    #[test]
    fn stored_reference_rejects_nested_identity_and_leading_zero_version() {
        assert!(
            CanonicalEvidenceReference::parse_stored("inbox_message:message-1:v1:processed:extra").is_err()
        );
        assert!(CanonicalEvidenceReference::parse_stored("mall_order_fact://f-1001:v1:attributed").is_err());
        assert!(CanonicalEvidenceReference::parse_stored("inbox_message:message-1:v01:processed").is_err());
        assert!(CanonicalEvidenceReference::parse_stored("").is_err());
    }
}
