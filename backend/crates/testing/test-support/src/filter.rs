//! 纯内存查询条件测试工具；只支持显式登记的查询操作，不连接数据库。
use mongodb::bson::{Bson, Document, serialize_to_document};
use serde_json::Value;

/// 以固定 JSON 样本验证仓储生成的查询条件。
///
/// # 参数
/// * `filter` - 仓储编译的条件。
/// * `object` - 测试样本，不含登录态或授权政策。
///
/// # 返回
/// 样本是否满足条件；仅构成内存逻辑证据，不证明 MongoDB 实际执行计划或事务。
///
/// # 错误
/// 不返回错误。
///
/// # Panics
/// 样本无法转换成 BSON 文档，或条件含未支持的操作时 panic，禁止静默忽略未知查询操作。
pub fn matches_filter(filter: &Document, object: &Value) -> bool {
    let object = serialize_to_document(object).expect("测试样本必须是可序列化对象");
    matches(filter, &object)
}

/// 支持 `$and`、`$or`、布尔 `$expr`、单键 `$exists` 或 `$in` 及字段等值；未知查询操作会 panic。
fn matches(filter: &Document, object: &Document) -> bool {
    filter.iter().all(|(key, condition)| match (key.as_str(), condition) {
        ("$and", Bson::Array(parts)) => {
            parts.iter().all(|part| matches(unsupported_as_document(part, "$and"), object))
        },
        ("$or", Bson::Array(parts)) => {
            parts.iter().any(|part| matches(unsupported_as_document(part, "$or"), object))
        },
        ("$expr", Bson::Boolean(value)) => *value,
        (_, Bson::Document(operator)) => {
            assert_eq!(operator.len(), 1, "未知查询操作必须使测试失败");
            if let Some(Bson::Boolean(expected)) = operator.get("$exists") {
                path_value(object, key).is_some() == *expected
            } else {
                let values = operator.get_array("$in").expect("未知查询操作必须使测试失败");
                path_value(object, key).is_some_and(|value| values.contains(value))
            }
        },
        (key, _) if key.starts_with('$') => unsupported_operator(key),
        (_, value) => path_value(object, key) == Some(value),
    })
}

/// 按 Mongo 字段路径读取文档或明确数组位置；缺路径不制造值。
fn path_value<'a>(object: &'a Document, path: &str) -> Option<&'a Bson> {
    let mut parts = path.split('.');
    let mut value = object.get(parts.next()?)?;
    for part in parts {
        value = match value {
            Bson::Document(doc) => doc.get(part)?,
            Bson::Array(items) => items.get(part.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(value)
}

fn unsupported_as_document<'a>(part: &'a Bson, operator: &str) -> &'a Document {
    part.as_document().unwrap_or_else(|| unsupported_operator(operator))
}

fn unsupported_operator(operation: &str) -> ! {
    panic!("未知查询操作: {operation}")
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;
    use serde_json::json;

    use super::*;

    #[test]
    fn conjunction_and_disjunction_keep_empty_sets_distinct() {
        assert!(matches_filter(&doc! {}, &json!({"id":"a"})));
        assert!(!matches_filter(&doc! {"$expr":false}, &json!({"id":"a"})));
        let query = doc! {"$and":[{"id":{"$in":["a","b"]}}, {"$or":[{"org":"one"},{"owner":"me"}]}]};
        assert!(matches_filter(&query, &json!({"id":"a","owner":"me"})));
        assert!(!matches_filter(&query, &json!({"id":"c","owner":"me"})));
        assert!(!matches_filter(&query, &json!({"id":"a","org":"two"})));
    }

    #[test]
    fn exists_distinguishes_null_from_missing() {
        let present = doc! {"item.code": {"$exists": true}};
        let absent = doc! {"item.code": {"$exists": false}};
        let with_null = json!({"item": {"code": null}});
        let without_code = json!({"item": {}});

        assert!(matches_filter(&present, &with_null));
        assert!(!matches_filter(&present, &without_code));
        assert!(!matches_filter(&absent, &with_null));
        assert!(matches_filter(&absent, &without_code));
    }

    #[test]
    #[should_panic(expected = "未知查询操作必须使测试失败")]
    fn exists_requires_a_boolean() {
        matches_filter(&doc! {"id": {"$exists": 1}}, &json!({"id": "a"}));
    }

    #[test]
    #[should_panic(expected = "未知查询操作")]
    fn unsupported_operators_never_silently_match() {
        matches_filter(&doc! {"id":{"$regex":"a"}}, &json!({"id":"a"}));
    }
}
