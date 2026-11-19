use bson::{Bson, Document};
use serde_json::Value;

use crate::entity::fingerprint::Fingerprint;

/// 将 BSON 持久化值写入稳定指纹，把 BSON 依赖留在仓储层。
pub(crate) trait BsonFingerprint {
    /// 写入 BSON 值时保留原始类型和有序容器结构。
    fn bson(&mut self, value: &Bson);
    /// 按字段原序写入 BSON 文档，不能按无序键集合处理。
    fn document(&mut self, document: &Document);
}

impl BsonFingerprint for Fingerprint {
    // 为容器保留专用标记和元素顺序；标量用 BSON Canonical Extended JSON 保留类型后再作稳定编码。
    fn bson(&mut self, value: &Bson) {
        match value {
            Bson::Document(document) => self.document(document),
            Bson::Array(values) => {
                self.text("array");
                self.sequence(values, Self::bson);
            },
            Bson::JavaScriptCodeWithScope(value) => {
                self.text("javascript_scope");
                self.text(&value.code);
                self.document(&value.scope);
            },
            _ => {
                self.text("scalar");
                self.json(&value.clone().into_canonical_extjson());
            },
        }
    }

    // 逐对写入原序键值，保留 MongoDB 有序文档的语义。
    fn document(&mut self, document: &Document) {
        self.text("document");
        self.sequence(document, |hash, (key, value)| {
            hash.text(key);
            hash.bson(value);
        });
    }
}

impl Fingerprint {
    /// 将动态事实的 JSON 写入摘要，使用类型标记、排序后的对象键和数字的十进制文本表示。
    pub(crate) fn json(&mut self, value: &Value) {
        match value {
            Value::Null => self.text("null"),
            Value::Bool(value) => self.boolean(*value),
            Value::String(value) => {
                self.text("string");
                self.text(value);
            },
            Value::Number(value) => {
                self.text("number");
                self.text(&value.to_string());
            },
            Value::Array(values) => {
                self.text("array");
                self.sequence(values, Self::json);
            },
            Value::Object(values) => {
                self.text("object");
                self.unordered(values, |hash, (key, value)| {
                    hash.text(key);
                    hash.json(value);
                });
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use bson::doc;

    use super::*;

    // 验证不同 BSON 类型及不同容器顺序会生成不同摘要。
    #[test]
    fn bson_types_and_document_order_remain_distinct() {
        let hash = |value: Bson| {
            let mut hash = Fingerprint::new("bson-test-v1");
            hash.bson(&value);
            hash.finish()
        };
        assert_ne!(hash(Bson::Int32(1)), hash(Bson::Int64(1)));
        assert_ne!(hash(Bson::Int64(1)), hash(Bson::String("1".into())));
        assert_ne!(hash(Bson::Int64(1)), hash(Bson::Document(doc! {"$numberLong": "1"})));
        assert_ne!(hash(Bson::Null), hash(Bson::String("null".into())));
        assert_ne!(hash(Bson::Document(doc! {"a": 1, "b": 2})), hash(Bson::Document(doc! {"b": 2, "a": 1})));
        assert_ne!(hash(Bson::Array(vec![1.into(), 2.into()])), hash(Bson::Array(vec![2.into(), 1.into()])));
    }
}
