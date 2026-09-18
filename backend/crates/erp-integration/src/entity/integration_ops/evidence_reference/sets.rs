//! 终态证据引用集合与非终结动作摘要的紧凑集合。

use erp_core::Result;

use super::parse::{encode_set, parse_kind};
use super::{CanonicalEvidenceReference, EvidenceRecordRef};

/// 终态证据引用集合：分号连接、排序、去重、512 字节上限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceReferenceSet {
    encoded: String,
}

impl EvidenceReferenceSet {
    /// 由 canonical 引用构造可持久化终态集合。
    ///
    /// # 参数
    /// * `refs` - 已验证的 canonical 引用
    ///
    /// # 返回
    /// 返回排序去重后的集合。
    ///
    /// # 错误
    /// 空集合或编码超过 512 字节时返回领域校验错误。
    ///
    /// # 约束
    /// 使用 `;` 连接以兼容存量终态证据字段；成员 ID 不得含 `;`。
    pub fn try_from_canonical<I>(refs: I) -> Result<Self>
    where
        I: IntoIterator<Item = CanonicalEvidenceReference>,
    {
        let encoded = encode_set(
            refs.into_iter().map(CanonicalEvidenceReference::into_wire),
            ';',
            true,
            "终态证据引用为空或过长",
        )?;
        Ok(Self { encoded })
    }

    /// 返回可持久化编码。
    ///
    /// # 返回
    /// 返回分号连接的 canonical 文本。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 编码已在构造时通过长度门禁。
    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    /// 消费集合并返回可持久化编码。
    ///
    /// # 返回
    /// 返回所有权字符串。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 与 [`Self::as_str`] 相同。
    pub fn into_wire(self) -> String {
        self.encoded
    }
}

/// 非终结动作摘要使用的紧凑证据集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactEvidenceSet {
    encoded: String,
}

impl CompactEvidenceSet {
    /// 由证据类型代码与记录 ID 构造紧凑集合。
    ///
    /// # 参数
    /// * `members` - `(证据类型代码, 记录 ID)`；记录 ID 必须是 `type:id`
    ///
    /// # 返回
    /// 空输入返回 `Ok(None)`；否则返回排序去重后的集合。
    ///
    /// # 错误
    /// 成员 grammar 非法或编码超过 512 字节时返回领域校验错误。
    ///
    /// # 约束
    /// 使用 `,` 连接以保持既有动作摘要形态；成员不得含 `,`。
    pub fn try_from_pairs<'a, I>(members: I) -> Result<Option<Self>>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut encoded = Vec::new();
        for (kind_code, record_id) in members {
            encoded.push(encode_compact_member(kind_code, record_id)?);
        }
        if encoded.is_empty() {
            return Ok(None);
        }
        let encoded = encode_set(encoded, ',', false, "证据引用汇总过长")?;
        Ok(Some(Self { encoded }))
    }

    /// 返回紧凑编码。
    ///
    /// # 返回
    /// 返回逗号连接的摘要文本。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 编码已在构造时通过长度门禁。
    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    /// 消费集合并返回紧凑编码。
    ///
    /// # 返回
    /// 返回所有权字符串。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 与 [`Self::as_str`] 相同。
    pub fn into_wire(self) -> String {
        self.encoded
    }
}

/// 编码紧凑集合的单个成员。
///
/// # 参数
/// * `kind_code` - 证据类型稳定代码
/// * `record_id` - 客户端 `type:id`
///
/// # 返回
/// 返回 `KIND:type:id`。
///
/// # 错误
/// 类型代码或记录 ID 非法时返回领域校验错误。
///
/// # 约束
/// 类型代码不得含分隔符，避免与记录 ID 抢段。
fn encode_compact_member(kind_code: &str, record_id: &str) -> Result<String> {
    let kind_code = parse_kind(kind_code.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?;
    let record = EvidenceRecordRef::parse(record_id)?;
    Ok(format!("{kind_code}:{record}"))
}

#[cfg(test)]
mod tests {
    use super::super::parse::ENCODED_MAX_LEN;
    use super::{CanonicalEvidenceReference, CompactEvidenceSet, EvidenceReferenceSet};

    #[test]
    fn terminal_set_sorts_dedups_and_enforces_512_boundary() {
        let first = CanonicalEvidenceReference::verified("inbox_message", "b", Some(1), "processed").unwrap();
        let second =
            CanonicalEvidenceReference::verified("inbox_message", "a", Some(1), "processed").unwrap();
        let duplicate = second.clone();
        let encoded =
            EvidenceReferenceSet::try_from_canonical([first, second, duplicate]).unwrap().into_wire();
        assert_eq!(encoded, "inbox_message:a:v1:processed;inbox_message:b:v1:processed");
        assert!(EvidenceReferenceSet::try_from_canonical(Vec::new()).is_err());

        let left_ok = "a".repeat(251);
        let right = "b".repeat(256);
        let at_limit =
            EvidenceReferenceSet::try_from_canonical([identity_ref(&left_ok), identity_ref(&right)]).unwrap();
        assert_eq!(at_limit.as_str().len(), ENCODED_MAX_LEN);

        let left_over = "a".repeat(252);
        assert!(
            EvidenceReferenceSet::try_from_canonical([identity_ref(&left_over), identity_ref(&right),])
                .is_err()
        );
    }

    #[test]
    fn compact_set_sorts_dedups_empty_and_512_boundary() {
        assert!(CompactEvidenceSet::try_from_pairs(Vec::<(&str, &str)>::new()).unwrap().is_none());
        let encoded = CompactEvidenceSet::try_from_pairs([
            ("EXTERNAL_CASE_RESULT", "inbox_message:b"),
            ("BUSINESS_OBJECT_VERIFICATION", "mall_order_fact:a"),
            ("EXTERNAL_CASE_RESULT", "inbox_message:b"),
        ])
        .unwrap()
        .unwrap()
        .into_wire();
        assert_eq!(
            encoded,
            "BUSINESS_OBJECT_VERIFICATION:mall_order_fact:a,EXTERNAL_CASE_RESULT:inbox_message:b"
        );
        assert!(CompactEvidenceSet::try_from_pairs([("EXTERNAL_CASE_RESULT", "inbox_message:a:b")]).is_err());

        let left_ok = format!("t:{}", "a".repeat(247));
        let right = format!("t:{}", "b".repeat(256));
        let at_limit = CompactEvidenceSet::try_from_pairs([("K", left_ok.as_str()), ("K", right.as_str())])
            .unwrap()
            .unwrap();
        assert_eq!(at_limit.as_str().len(), ENCODED_MAX_LEN);

        let left_over = format!("t:{}", "a".repeat(248));
        assert!(
            CompactEvidenceSet::try_from_pairs([("K", left_over.as_str()), ("K", right.as_str())]).is_err()
        );
    }

    fn identity_ref(id: &str) -> CanonicalEvidenceReference {
        CanonicalEvidenceReference::parse_stored(&format!("k:{id}")).unwrap()
    }
}
