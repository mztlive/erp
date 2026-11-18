//! 身份版本编码合同 v1：首字段为用途及格式版本，输出为 MD5 小写十六进制。
//! 文本编码为 `s + UTF-8 字节数(u64 大端) + UTF-8`；无符号整数为 `u + u64 大端`；
//! 布尔为 t/f，Option 为 n 或 o + 值，序列为 [ + 元素 + ]。
//! 无序集合按元素的完整编码排序，保留重复项；有序序列保持输入顺序。
//! 字段顺序由各调用点声明；修改标记、字段或顺序必须提升对应用途的格式版本。
//! 此格式替代旧 Debug 编码，旧分页版本按原 DATA_SCOPE_CHANGED 合同失效。

mod authorization;
mod scope;

/// 仅供身份域内部使用的有边界、带类型的版本编码器。
#[derive(Default)]
pub(crate) struct Fingerprint(Vec<u8>);

impl Fingerprint {
    /// 用明确的用途和格式版本开始编码。
    pub(crate) fn new(format: &str) -> Self {
        let mut value = Self::default();
        value.text(format);
        value
    }

    /// 文本采用 UTF-8 字节长度，禁止调用方传入 Debug 表示。
    pub(crate) fn text(&mut self, value: &str) {
        self.0.push(b's');
        self.0.extend_from_slice(&u64::try_from(value.len()).expect("字符串长度不超过 u64").to_be_bytes());
        self.0.extend_from_slice(value.as_bytes());
    }

    /// 数字固定为 64 位大端，不依赖平台字宽和显示格式。
    pub(crate) fn number(&mut self, value: u64) {
        self.0.push(b'u');
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    /// 布尔使用独立的固定标记。
    pub(crate) fn boolean(&mut self, value: bool) {
        self.0.push(if value { b't' } else { b'f' });
    }

    /// 缺失值与存在的空值必须区分。
    pub(crate) fn optional<T>(&mut self, value: Option<T>, encode: impl FnOnce(&mut Self, T)) {
        match value {
            None => self.0.push(b'n'),
            Some(value) => {
                self.0.push(b'o');
                encode(self, value);
            },
        }
    }

    /// 保持有序输入；嵌套序列通过起止标记划分边界。
    pub(crate) fn sequence<T>(&mut self, values: impl IntoIterator<Item = T>, encode: impl Fn(&mut Self, T)) {
        self.0.push(b'[');
        for value in values {
            encode(self, value);
        }
        self.0.push(b']');
    }

    /// 无序输入按完整元素编码排序；不丢弃重复元素。
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

    /// 保持现有对外版本字段的 32 位小写十六进制形式。
    pub(crate) fn finish(self) -> String {
        format!("{:x}", md5::compute(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 固定字节合同，防止内部重构悄悄改变版本编码。
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

    /// 字段边界、可选值和集合边界均参与摘要。
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

    /// 只对明确声明无序的集合忽略输入顺序。
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
