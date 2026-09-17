//! 纯内存查询条件测试工具；只支持显式登记的查询操作，不连接数据库。
use mongodb::bson::{Bson, Document, serialize_to_document};
use serde_json::Value;

/// 以固定 JSON 样本验证仓储生成的查询条件。
///
/// # 参数
/// * `filter` - 仓储编译的条件
/// * `object` - 测试样本，不含登录态或授权政策
/// # 返回
/// 样本是否满足条件；仅构成内存逻辑证据，不证明 MongoDB 实际执行计划或事务。
/// # Panics
/// 样本无法转换或条件含未支持的操作时失败，禁止静默忽略未知查询操作。
pub fn matches_filter(filter: &Document, object: &Value) -> bool {
    let object = serialize_to_document(object).expect("测试样本必须是可序列化对象");
    matches(filter, &object)
}

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
            let values = operator.get_array("$in").expect("未知查询操作必须使测试失败");
            object.get(key).is_some_and(|value| values.contains(value))
        },
        (key, _) if key.starts_with('$') => unsupported_operator(key),
        (_, value) => object.get(key) == Some(value),
    })
}

/// 将查询数组元素转为文档；非文档一律走统一的未知操作失败路径。
///
/// # 参数
/// * `part` - `$and`/`$or` 数组中的单个元素
/// * `operator` - 所属的组合操作名，仅用于失败信息
///
/// # 返回值
/// 返回元素对应的文档引用。
///
/// # Panics
/// 元素不是文档时 panic，信息与其他未知查询操作保持一致。
fn unsupported_as_document<'a>(part: &'a Bson, operator: &str) -> &'a Document {
    part.as_document().unwrap_or_else(|| unsupported_operator(operator))
}

/// 以统一信息使测试失败；所有不支持的查询操作都走本函数。
///
/// # 参数
/// * `operation` - 不支持的操作名
///
/// # 返回值
/// 永不返回。
///
/// # Panics
/// 总是 panic，信息前缀固定为 `未知查询操作`。
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
    #[should_panic(expected = "未知查询操作")]
    fn unsupported_operators_never_silently_match() {
        matches_filter(&doc! {"id":{"$regex":"a"}}, &json!({"id":"a"}));
    }
}
