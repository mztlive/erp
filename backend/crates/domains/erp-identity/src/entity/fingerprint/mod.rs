//! 身份版本编码合同 v1：首字段为用途及格式版本，输出为 MD5 小写十六进制。
//! 文本编码为 `s + UTF-8 字节数(u64 大端) + UTF-8`；无符号整数为 `u + u64 大端`；
//! 布尔为 t/f，Option 为 n 或 o + 值，序列为 [ + 元素 + ]。
//! 无序集合按元素的完整编码排序，保留重复项；有序序列保持输入顺序。
//! 字段顺序由各调用点声明；修改标记、字段或顺序必须提升对应用途的格式版本。
//! 此格式替代旧 Debug 编码，旧分页版本按原 DATA_SCOPE_CHANGED 合同失效。

mod authorization;

use crate::entity::access_control::{ResolvedScope, ScopeClause};

/// 身份域内部的版本指纹编码器，以类型标记和边界形成稳定字节串。
#[derive(Default)]
pub(crate) struct Fingerprint(
    // 待计算摘要的完整编码；临时元素编码也使用此缓冲区。
    Vec<u8>,
);

impl Fingerprint {
    /// 将用途和格式版本作为首个文本字段，区分不同版本合同。
    pub(crate) fn new(format: &str) -> Self {
        let mut value = Self::default();
        value.text(format);
        value
    }

    /// 写入原始文本，依次编码类型标记、UTF-8 字节数和原始字节。
    pub(crate) fn text(&mut self, value: &str) {
        self.0.push(b's');
        self.0.extend_from_slice(&u64::try_from(value.len()).expect("字符串长度不超过 u64").to_be_bytes());
        self.0.extend_from_slice(value.as_bytes());
    }

    /// 以独立数字标记和固定 64 位大端字节写入无符号整数。
    pub(crate) fn number(&mut self, value: u64) {
        self.0.push(b'u');
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    /// 分别以单字节 `t` 和 `f` 编码真和假。
    pub(crate) fn boolean(&mut self, value: bool) {
        self.0.push(if value { b't' } else { b'f' });
    }

    /// 先写缺失或存在标记；仅在有值时编码内容，使存在的空值与缺失有不同编码。
    pub(crate) fn optional<T>(&mut self, value: Option<T>, encode: impl FnOnce(&mut Self, T)) {
        match value {
            None => self.0.push(b'n'),
            Some(value) => {
                self.0.push(b'o');
                encode(self, value);
            },
        }
    }

    /// 按迭代顺序编码元素，以成对的起止标记保留空序列和嵌套边界。
    pub(crate) fn sequence<T>(&mut self, values: impl IntoIterator<Item = T>, encode: impl Fn(&mut Self, T)) {
        self.0.push(b'[');
        for value in values {
            encode(self, value);
        }
        self.0.push(b']');
    }

    /// 对每个成员的完整编码排序后写入序列，消除输入顺序差异并保留重复项。
    pub(crate) fn unordered<T>(
        &mut self,
        values: impl IntoIterator<Item = T>,
        encode: impl Fn(&mut Self, T),
    ) {
        let mut parts: Vec<_> = values
            .into_iter()
            .map(|value| {
                let mut part = Self::default();
                encode(&mut part, value);
                part.0
            })
            .collect();
        parts.sort();

        self.0.push(b'[');
        for part in parts {
            self.0.extend(part);
        }
        self.0.push(b']');
    }

    /// 消费编码缓冲区，生成版本字段使用的 32 字符小写十六进制 MD5 摘要。
    pub(crate) fn finish(self) -> String {
        format!("{:x}", md5::compute(self.0))
    }

    /// 为版本绑定范围：先编码无序角色条款，再编码可选个人上限，区分未设上限与空条款。
    pub(crate) fn scope(&mut self, scope: &ResolvedScope) {
        let ResolvedScope { role_clauses, user_limit } = scope;
        self.unordered(role_clauses, Self::clause);
        self.optional(user_limit.as_ref(), Self::clause);
    }

    // 依次编码公司、本人、协作及组织、结算主体、仓库，三组 ID 均按无序集合处理。
    fn clause(&mut self, clause: &ScopeClause) {
        let ScopeClause {
            company,
            self_owned,
            collaborative,
            org_unit_ids,
            settlement_party_ids,
            warehouse_ids,
        } = clause;
        self.boolean(*company);
        self.boolean(*self_owned);
        self.boolean(*collaborative);
        for ids in [org_unit_ids, settlement_party_ids, warehouse_ids] {
            self.unordered(ids, |hash, id| hash.text(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 用固定摘要锁定文本、整数、布尔、可选值和序列的字节合同。
    #[test]
    fn encoding_has_a_fixed_contract() {
        let mut value = Fingerprint::new("test-v1");
        value.text("甲");
        value.number(42);
        value.boolean(true);
        value.optional(None::<&str>, Fingerprint::text);
        value.optional(Some(""), Fingerprint::text);
        value.sequence(["a", "b"], Fingerprint::text);
        assert_eq!(value.finish(), "e1a3f72da0a3e03d21a01ce172cfbf8e");
    }

    // 验证字段切分、空序列、空值标记与格式版本均能区分摘要。
    #[test]
    fn boundaries_and_empty_values_are_distinct() {
        let hash = |parts: &[&str]| {
            let mut value = Fingerprint::new("test-v1");
            value.sequence(parts.iter().copied(), Fingerprint::text);
            value.finish()
        };
        assert_ne!(hash(&["a:b", "c"]), hash(&["a", "b:c"]));
        assert_ne!(hash(&[]), hash(&[""]));
        let mut missing = Fingerprint::new("test-v1");
        missing.optional(None::<&str>, Fingerprint::text);
        let mut empty = Fingerprint::new("test-v1");
        empty.optional(Some(""), Fingerprint::text);
        assert_ne!(missing.finish(), empty.finish());
        assert_ne!(Fingerprint::new("test-v1").finish(), Fingerprint::new("test-v2").finish());
    }

    // 验证无序编码对重排保持不变，重复成员仍改变摘要。
    #[test]
    fn unordered_inputs_are_sorted_without_losing_duplicates() {
        let hash = |parts: &[&str]| {
            let mut value = Fingerprint::new("test-v1");
            value.unordered(parts.iter().copied(), Fingerprint::text);
            value.finish()
        };
        assert_eq!(hash(&["a", "b"]), hash(&["b", "a"]));
        assert_ne!(hash(&["a"]), hash(&["a", "a"]));
    }
}
