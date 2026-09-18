//! 当前业务项可参与授权关联的稳定身份。

use super::CanonicalEvidenceReference;

/// 当前业务项可参与授权关联的稳定身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceSubjectBindings {
    message_id: Option<String>,
    business_object_id: Option<String>,
    fact_identities: Vec<CanonicalEvidenceReference>,
    unparsed_fact_references: Vec<String>,
}

impl EvidenceSubjectBindings {
    /// 由主体身份字段构造关联绑定。
    ///
    /// # 参数
    /// * `message_id` - 错误任务入站消息 ID
    /// * `business_object_id` - 任务或差异业务对象 ID
    /// * `fact_references` - 差异两侧已持久化事实引用
    ///
    /// # 返回
    /// 返回只读绑定；非法存量引用不会导致构造失败，但也不会按 token 命中。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 空白身份视为缺失；可解析引用只暴露身份 ID。
    pub fn new(
        message_id: Option<&str>,
        business_object_id: Option<&str>,
        fact_references: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Self {
        let mut fact_identities = Vec::new();
        let mut unparsed_fact_references = Vec::new();
        for reference in fact_references {
            let raw = reference.as_ref().trim();
            if raw.is_empty() {
                continue;
            }
            match CanonicalEvidenceReference::parse_stored(raw) {
                Ok(parsed) => fact_identities.push(parsed),
                Err(_) => unparsed_fact_references.push(raw.to_string()),
            }
        }
        Self {
            message_id: normalize_optional_identity(message_id),
            business_object_id: normalize_optional_identity(business_object_id),
            fact_identities,
            unparsed_fact_references,
        }
    }

    /// 判断候选 ID 是否与主体存在可验证的正式关联。
    ///
    /// # 参数
    /// * `ids` - 证据记录及其正式关联对象的候选 ID
    ///
    /// # 返回
    /// 任一非空候选精确等于消息 ID、业务对象 ID 或事实引用身份 ID 时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不匹配类型名、版本段、状态段或分隔后的子串；空候选永不命中。
    pub fn associates_any<'a, I>(&self, ids: I) -> bool
    where
        I: IntoIterator<Item = &'a str>,
    {
        ids.into_iter().any(|id| self.associates_id(id))
    }

    /// 按对象类型提取第一条事实引用的身份 ID。
    ///
    /// # 参数
    /// * `kind` - 期望的对象类型
    ///
    /// # 返回
    /// 返回首次匹配类型的身份 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不返回 `vN` 或状态后缀，避免把 canonical 文本整段当作查找键。
    pub fn referenced_id(&self, kind: &str) -> Option<&str> {
        self.fact_identities
            .iter()
            .find(|reference| reference.kind() == kind)
            .map(CanonicalEvidenceReference::id)
    }

    /// 精确比较单个候选 ID。
    ///
    /// # 参数
    /// * `id` - 候选身份
    ///
    /// # 返回
    /// 精确命中时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 无法解析的存量引用只允许整串精确相等，禁止 delimiter split。
    fn associates_id(&self, id: &str) -> bool {
        let id = id.trim();
        if id.is_empty() {
            return false;
        }
        self.message_id.as_deref() == Some(id)
            || self.business_object_id.as_deref() == Some(id)
            || self.fact_identities.iter().any(|reference| reference.id() == id)
            || self.unparsed_fact_references.iter().any(|reference| reference.as_str() == id)
    }
}

/// 规范化可选身份，空白视为缺失。
///
/// # 参数
/// * `value` - 原始可选身份
///
/// # 返回
/// 非空时返回去空白后的身份。
///
/// # 错误
/// 无。
///
/// # 约束
/// 不解析 grammar，只处理主体字段。
fn normalize_optional_identity(value: Option<&str>) -> Option<String> {
    value.map(str::trim).filter(|value| !value.is_empty()).map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::EvidenceSubjectBindings;

    fn bindings(facts: &[&str]) -> EvidenceSubjectBindings {
        EvidenceSubjectBindings::new(Some("message-1"), Some("object-1"), facts)
    }

    #[test]
    fn referenced_id_returns_identity_not_version_suffix() {
        let bindings = EvidenceSubjectBindings::new(
            None,
            None,
            ["mall_order_fact:abc:v1:attributed", "invoice://inv-88"],
        );
        assert_eq!(bindings.referenced_id("mall_order_fact"), Some("abc"));
        assert_eq!(bindings.referenced_id("invoice"), Some("inv-88"));
        assert_eq!(bindings.referenced_id("missing"), None);
    }

    #[test]
    fn association_uses_exact_identity_and_ignores_false_substring_hits() {
        let subject = bindings(&["mall_order_fact:order-123:v1:attributed"]);
        assert!(subject.associates_any(["order-123"]));
        assert!(subject.associates_any(["message-1"]));
        assert!(subject.associates_any(["object-1"]));
        assert!(!subject.associates_any(["order-12"]));
        assert!(!subject.associates_any(["v1"]));
        assert!(!subject.associates_any(["attributed"]));
        assert!(!subject.associates_any(["mall_order_fact"]));
        assert!(!subject.associates_any(["processed"]));
        assert!(!subject.associates_any([""]));
        assert!(!subject.associates_any(["  "]));
        assert!(!subject.associates_any(["mall_order_fact:order-123:v1:attributed"]));
    }

    #[test]
    fn association_rejects_delimiter_injected_and_unparsed_token_splits() {
        let subject = bindings(&["mall_order_fact:abc;evil", "not-a-reference"]);
        assert!(!subject.associates_any(["abc"]));
        assert!(!subject.associates_any(["evil"]));
        assert!(subject.associates_any(["not-a-reference"]));
        assert!(!subject.associates_any(["not"]));
        assert!(!subject.associates_any(["a"]));
        assert!(!subject.associates_any(["reference"]));
    }
}
