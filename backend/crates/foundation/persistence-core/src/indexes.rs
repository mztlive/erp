//! 命名 MongoDB 索引的通用构造；索引内容与登记顺序由拥有领域提供。

use mongodb::IndexModel;
use mongodb::bson::Document;
use mongodb::options::IndexOptions;

/// 构造命名普通索引，不设置唯一性或部分过滤条件。
///
/// # 参数
/// * `name` - 拥有领域确定的稳定索引名。
/// * `keys` - 保持字段顺序与排序方向的索引键文档。
///
/// # 返回
/// 返回仅设置名称和键的索引模型。
///
/// # 错误
/// 无；本方法不校验业务定义，也不访问数据库。
pub fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder().keys(keys).options(IndexOptions::builder().name(name.into()).build()).build()
}

/// 构造命名唯一索引，不设置部分过滤条件。
///
/// # 参数
/// * `name` - 拥有领域确定的稳定索引名。
/// * `keys` - 保持字段顺序与排序方向的唯一索引键文档。
///
/// # 返回
/// 返回设置名称、键与 `unique: true` 的索引模型。
///
/// # 错误
/// 无；已有数据是否违反约束由索引创建操作判定。
pub fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

/// 构造命名部分唯一索引，原样保留领域提供的过滤条件。
///
/// # 参数
/// * `name` - 拥有领域确定的稳定索引名。
/// * `keys` - 保持字段顺序与排序方向的唯一索引键文档。
/// * `filter` - 拥有领域确定的部分过滤表达式。
///
/// # 返回
/// 返回设置名称、键、唯一性及部分过滤表达式的索引模型。
///
/// # 错误
/// 无；过滤表达式合法性及已有数据约束由索引创建操作判定。
pub fn unique_partial_index(name: impl Into<String>, keys: Document, filter: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder().name(name.into()).unique(true).partial_filter_expression(filter).build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::{doc, serialize_to_document};

    use super::{named_index, unique_index, unique_partial_index};

    #[test]
    fn ordinary_index_keeps_key_order_and_omits_unique_option() {
        let index = named_index("idx_owner_created", doc! { "owner_id": 1, "created_at": -1, "id": -1 });

        assert_eq!(
            index.keys.keys().map(String::as_str).collect::<Vec<_>>(),
            ["owner_id", "created_at", "id"]
        );
        assert_eq!(index.keys, doc! { "owner_id": 1, "created_at": -1, "id": -1 });
        assert_eq!(
            serialize_to_document(index.options.as_ref().unwrap()).unwrap(),
            doc! { "name": "idx_owner_created" }
        );
    }

    #[test]
    fn unique_index_keeps_identity_and_unfiltered_constraint() {
        let index = unique_index(String::from("uk_source_line"), doc! { "source_id": 1, "line_no": 1 });

        assert_eq!(index.keys.keys().map(String::as_str).collect::<Vec<_>>(), ["source_id", "line_no"]);
        assert_eq!(index.keys, doc! { "source_id": 1, "line_no": 1 });
        assert_eq!(
            serialize_to_document(index.options.as_ref().unwrap()).unwrap(),
            doc! { "name": "uk_source_line", "unique": true }
        );
    }

    #[test]
    fn partial_unique_index_preserves_nested_filter_without_extra_options() {
        let filter = doc! {
            "deleted_at": 0_i64,
            "status": { "$in": ["draft", "submitted"] },
            "source_id": { "$type": "string" },
        };
        let index =
            unique_partial_index("uk_active_source", doc! { "owner_id": 1, "source_id": 1 }, filter.clone());

        assert_eq!(index.keys.keys().map(String::as_str).collect::<Vec<_>>(), ["owner_id", "source_id"]);
        assert_eq!(index.keys, doc! { "owner_id": 1, "source_id": 1 });
        assert_eq!(
            serialize_to_document(index.options.as_ref().unwrap()).unwrap(),
            doc! { "name": "uk_active_source", "unique": true, "partialFilterExpression": filter }
        );
    }
}
