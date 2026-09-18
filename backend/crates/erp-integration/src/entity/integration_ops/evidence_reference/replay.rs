//! 原动作重放结果：canonical inbox 引用加上不透明业务事实键。

use erp_core::{Error, Result};

use super::CanonicalEvidenceReference;

/// 原动作重放结果：canonical inbox 引用加上不透明业务事实键。
///
/// 事实键可含 `|` 等 inbox 约定分隔符，因此不得作为证据 ID 解析，也不得
/// 作为 [`super::EvidenceReferenceSet`] 成员编码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayOriginalReference {
    inbox: CanonicalEvidenceReference,
    business_fact_key: String,
}

impl ReplayOriginalReference {
    /// 由入站消息身份与已规范化业务事实键构造重放引用。
    ///
    /// # 参数
    /// * `inbox_id` - 入站消息 ID
    /// * `version` - 入站消息正式版本
    /// * `business_fact_key` - 不透明业务事实键，允许含 `|`
    ///
    /// # 返回
    /// 返回 inbox canonical 与事实键分离持有的重放引用。
    ///
    /// # 错误
    /// inbox 身份非法或事实键为空时返回领域校验错误。
    ///
    /// # 约束
    /// 事实键只去首尾空白，不按证据 ID 解析；inbox 段由
    /// [`CanonicalEvidenceReference::verified`] 编码为 `inbox_message:id:vN:requeued`。
    pub fn new(inbox_id: &str, version: u64, business_fact_key: &str) -> Result<Self> {
        let inbox =
            CanonicalEvidenceReference::verified("inbox_message", inbox_id, Some(version), "requeued")?;
        let business_fact_key = business_fact_key.trim();
        if business_fact_key.is_empty() {
            return Err(Error::from("业务事实键不能为空"));
        }
        Ok(Self { inbox, business_fact_key: business_fact_key.to_string() })
    }

    /// 解析重放引用的持久化形态。
    ///
    /// # 参数
    /// * `value` - `{inbox_canonical};business_fact_key:{opaque_key}`
    ///
    /// # 返回
    /// 返回 inbox canonical 与完整事实键。
    ///
    /// # 错误
    /// 缺少固定分隔、inbox grammar 非法或事实键为空时返回领域校验错误。
    ///
    /// # 约束
    /// 使用 `split_once(";business_fact_key:")`，事实键可含 `|`、额外 `;` 或 `:`。
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let (inbox, business_fact_key) = value
            .split_once(";business_fact_key:")
            .ok_or_else(|| Error::from("重放引用必须包含 business_fact_key 字段"))?;
        Self::from_parts(CanonicalEvidenceReference::parse_stored(inbox)?, business_fact_key)
    }

    /// 返回 inbox canonical 引用。
    ///
    /// # 返回
    /// 返回 `inbox_message:id:vN:requeued` 对应的值对象。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 只读；不重新编码事实键。
    pub fn inbox(&self) -> &CanonicalEvidenceReference {
        &self.inbox
    }

    /// 返回不透明业务事实键。
    ///
    /// # 返回
    /// 返回构造时冻结的事实键，含 `|` 等原字符。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不得把返回值当作证据 ID 再解析。
    pub fn business_fact_key(&self) -> &str {
        &self.business_fact_key
    }

    /// 消费并返回可写入业务结果引用的编码。
    ///
    /// # 返回
    /// 返回 `{inbox};business_fact_key:{key}`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不把整串交给 [`super::EvidenceReferenceSet`]；事实键保持原文。
    pub fn into_wire(self) -> String {
        format!("{};business_fact_key:{}", self.inbox, self.business_fact_key)
    }

    /// 由已解析 inbox 与原文事实键组装重放引用。
    ///
    /// # 参数
    /// * `inbox` - 已校验的 inbox canonical
    /// * `business_fact_key` - 分隔符之后的原文
    ///
    /// # 返回
    /// 返回重放引用。
    ///
    /// # 错误
    /// 事实键为空时返回领域校验错误。
    ///
    /// # 约束
    /// 不把事实键按证据 ID 解析。
    fn from_parts(inbox: CanonicalEvidenceReference, business_fact_key: &str) -> Result<Self> {
        let business_fact_key = business_fact_key.trim();
        if business_fact_key.is_empty() {
            return Err(Error::from("业务事实键不能为空"));
        }
        Ok(Self { inbox, business_fact_key: business_fact_key.to_string() })
    }
}

#[cfg(test)]
mod tests {
    use super::{CanonicalEvidenceReference, ReplayOriginalReference};

    #[test]
    fn replay_original_preserves_pipe_bearing_fact_key_without_set_encoding() {
        const KEY: &str = "mall-1|PAYMENT_SUCCEEDED|SO-2026-001|v3";
        assert!(
            CanonicalEvidenceReference::parse_stored(&format!("business_fact_key:{KEY}")).is_err(),
            "含 | 的事实键不得当作证据 ID"
        );
        assert!(
            CanonicalEvidenceReference::verified("inbox_message", KEY, Some(1), "requeued").is_err(),
            "含 | 的事实键不得走 parse_id"
        );

        let replay = ReplayOriginalReference::new("msg-1", 2, &format!(" {KEY} ")).unwrap();
        assert_eq!(replay.inbox().to_string(), "inbox_message:msg-1:v2:requeued");
        assert_eq!(replay.business_fact_key(), KEY);
        let wire = replay.into_wire();
        assert_eq!(wire, format!("inbox_message:msg-1:v2:requeued;business_fact_key:{KEY}"));
        let parsed = ReplayOriginalReference::parse(&wire).unwrap();
        assert_eq!(parsed.business_fact_key(), KEY);
        assert_eq!(parsed.inbox().id(), "msg-1");
        assert_eq!(parsed.inbox().version(), Some(2));
        assert!(ReplayOriginalReference::parse("inbox_message:msg-1:v2:requeued").is_err());
        assert!(ReplayOriginalReference::new("msg-1", 2, "  ").is_err());
    }
}
