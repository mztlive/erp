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
    // 以明确的格式版本和用途初始化摘要器，避免不同授权版本合同使用相同编码域。
    pub(crate) fn new(format: &str) -> Self {
        let mut value = Self::default();
        value.text(format);
        value
    }

    // 按 UTF-8 字节长度编码指纹中的文本，调用方提供原值而不是不稳定的 `Debug` 格式。
    pub(crate) fn text(&mut self, value: &str) {
        self.0.push(b's');
        self.0.extend_from_slice(&u64::try_from(value.len()).expect("字符串长度不超过 u64").to_be_bytes());
        self.0.extend_from_slice(value.as_bytes());
    }

    // 向摘要写入固定 64 位大端数字，使结果不依赖机器字宽或显示格式。
    pub(crate) fn number(&mut self, value: u64) {
        self.0.push(b'u');
        self.0.extend_from_slice(&value.to_be_bytes());
    }

    // 将布尔值写成独立的固定标记，不与数字或字符串的编码混用。
    pub(crate) fn boolean(&mut self, value: bool) {
        self.0.push(if value { b't' } else { b'f' });
    }

    // 为可选事实分别编码缺失与存在；即使存在值的编码为空，也不得与缺失产生相同摘要。
    pub(crate) fn optional<T>(&mut self, value: Option<T>, encode: impl FnOnce(&mut Self, T)) {
        match value {
            None => self.0.push(b'n'),
            Some(value) => {
                self.0.push(b'o');
                encode(self, value);
            },
        }
    }

    // 按原顺序写入有序序列。用起止标记隔开嵌套边界，避免不同形状的输入合并。
    pub(crate) fn sequence<T>(&mut self, values: impl IntoIterator<Item = T>, encode: impl Fn(&mut Self, T)) {
        self.0.push(b'[');
        for value in values {
            encode(self, value);
        }
        self.0.push(b']');
    }

    // 计算无序集合摘要时按成员完整编码排序，保留重复项，不把集合清洗混入摘要逻辑。
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

    // 完成摘要并返回对外版本字段沿用的 32 位小写十六进制文本。
    pub(crate) fn finish(self) -> String {
        format!("{:x}", md5::compute(self.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // 验证：固定字节合同，防止内部重构悄悄改变版本编码。
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

    // 验证：字段边界、可选值和集合边界均参与摘要。
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

    // 验证：只对明确声明无序的集合忽略输入顺序。
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
