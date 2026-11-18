//! 命名 MongoDB 索引的通用构造；索引内容与登记顺序由拥有领域提供。

use mongodb::IndexModel;
use mongodb::bson::Document;
use mongodb::options::IndexOptions;

/// 为领域登记普通索引时只设置稳定名称和有序键文档，保留字段顺序与方向，不加唯一或部分过滤，也不直接创建索引。
pub fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder().keys(keys).options(IndexOptions::builder().name(name.into()).build()).build()
}

/// 领域需要对全集合强制唯一性时构造带稳定名称的索引，保留复合键顺序；存量数据能否满足约束由实际创建操作判断。
pub fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

/// 仅匹配领域过滤条件的文档须唯一时使用，原样保留复合键顺序与过滤表达式；建索引时由数据库检查表达式和数据。
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
