use bson::{Bson, Document};
use serde_json::Value;

use crate::entity::fingerprint::Fingerprint;

/// 持久化值到稳定指纹的适配，仅在仓储层依赖 BSON。
pub(crate) trait BsonFingerprint {
    /// 编码任意 BSON 值，保留值类型与有序结构。
    fn bson(&mut self, value: &Bson);
    /// 编码 BSON 文档，保留字段顺序。
    fn document(&mut self, document: &Document);
}

impl BsonFingerprint for Fingerprint {
    /// BSON 容器使用独立标记；文档保持字段顺序，数组保持元素顺序。
    /// 标量采用 BSON Canonical Extended JSON 的类型表示，再按稳定 JSON 规则编码。
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

    /// 文档按原顺序编码键值，避免改变 MongoDB 中有序文档的查询含义。
    fn document(&mut self, document: &Document) {
        self.text("document");
        self.sequence(document, |hash, (key, value)| {
            hash.text(key);
            hash.bson(value);
        });
    }
}

impl Fingerprint {
    /// JSON 固定类型标记；对象键排序，数字采用 JSON 十进制表示。
    fn json(&mut self, value: &Value) {
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

    /// BSON 类型及有序结构不可被规范化为同一摘要。
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
