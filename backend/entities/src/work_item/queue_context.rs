//! 队列上下文的版本化规范身份。

use sha2::{Digest, Sha256};

/// 队列上下文字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueContextField {
    name: String,
    values: Vec<String>,
}

impl QueueContextField {
    /// 构造单值字段。
    pub fn scalar(name: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            values: vec![value.into()],
        }
    }

    /// 构造可选单值字段，显式区分缺失和值。
    pub fn optional(name: impl Into<String>, value: Option<&str>) -> Self {
        Self::scalar(
            name,
            match value {
                Some(value) => Self::tuple(["some".to_string(), value.to_string()]),
                None => Self::tuple(["none".to_string()]),
            },
        )
    }

    /// 构造语义无序的集合字段，值将排序并去重。
    pub fn set(name: impl Into<String>, values: impl IntoIterator<Item = String>) -> Self {
        let mut values = values.into_iter().collect::<Vec<_>>();
        values.sort();
        values.dedup();
        Self {
            name: name.into(),
            values,
        }
    }

    /// 把多个分量编码为无拼接碰撞的单个集合值。
    pub fn tuple(parts: impl IntoIterator<Item = String>) -> String {
        let mut encoded = Vec::new();
        for part in parts {
            push_component(&mut encoded, part.as_bytes());
        }
        hex::encode(encoded)
    }
}

/// 版本化队列上下文身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QueueContextIdentity(String);

impl QueueContextIdentity {
    /// 按字段名稳定排序、长度前缀编码后形成 v1 SHA-256 身份。
    ///
    /// 字段值不依赖 `Debug` 或 JSON Map 顺序。旧版无前缀令牌不会匹配，调用方
    /// 应返回刷新提示。
    pub fn new(namespace: &str, fields: impl IntoIterator<Item = QueueContextField>) -> Self {
        let mut fields = fields.into_iter().collect::<Vec<_>>();
        fields.sort_by(|left, right| left.name.cmp(&right.name));
        let mut bytes = Vec::new();
        push_component(&mut bytes, b"queue-context-v1");
        push_component(&mut bytes, namespace.as_bytes());
        for field in fields {
            push_component(&mut bytes, field.name.as_bytes());
            bytes.extend_from_slice(&(field.values.len() as u64).to_be_bytes());
            for value in field.values {
                push_component(&mut bytes, value.as_bytes());
            }
        }
        Self(format!("qctx-v1-{}", hex::encode(Sha256::digest(bytes))))
    }

    /// 返回稳定上下文 ID。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 消费值对象并返回稳定上下文 ID。
    pub fn into_string(self) -> String {
        self.0
    }
}

fn push_component(output: &mut Vec<u8>, component: &[u8]) {
    output.extend_from_slice(&(component.len() as u64).to_be_bytes());
    output.extend_from_slice(component);
}

#[cfg(test)]
mod tests {
    use super::{QueueContextField, QueueContextIdentity};

    #[test]
    fn unordered_semantic_sets_have_one_identity() {
        let left = QueueContextIdentity::new(
            "work-items",
            [QueueContextField::set(
                "types",
                ["B".to_string(), "A".to_string()],
            )],
        );
        let right = QueueContextIdentity::new(
            "work-items",
            [QueueContextField::set(
                "types",
                ["A".to_string(), "B".to_string()],
            )],
        );
        assert_eq!(left, right);
        assert_eq!(left.as_str().len(), 72);
    }

    #[test]
    fn field_changes_and_tuple_boundaries_change_identity() {
        let left = QueueContextIdentity::new(
            "work-items",
            [QueueContextField::scalar(
                "scope",
                QueueContextField::tuple(["ab".into(), "c".into()]),
            )],
        );
        let right = QueueContextIdentity::new(
            "work-items",
            [QueueContextField::scalar(
                "scope",
                QueueContextField::tuple(["a".into(), "bc".into()]),
            )],
        );
        assert_ne!(left, right);
    }
}
