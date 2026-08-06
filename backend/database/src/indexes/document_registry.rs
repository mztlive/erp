//! 域 D02 `document_registry` 的索引声明：business_document、document_relation、document_participant、workflow_action。
//!
//! 集合名常量取 `DocumentRegistryExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::DocumentRegistryExt;
use crate::Result;

/// `business_document` 集合名。
pub(crate) const BUSINESS_DOCUMENTS: &str = <mongodb::Database as DocumentRegistryExt>::BUSINESS_DOCUMENTS;
/// `document_relation` 集合名。
pub(crate) const DOCUMENT_RELATIONS: &str = <mongodb::Database as DocumentRegistryExt>::DOCUMENT_RELATIONS;
/// `document_participant` 集合名。
pub(crate) const DOCUMENT_PARTICIPANTS: &str =
    <mongodb::Database as DocumentRegistryExt>::DOCUMENT_PARTICIPANTS;
/// `workflow_action` 集合名。
pub(crate) const WORKFLOW_ACTIONS: &str = <mongodb::Database as DocumentRegistryExt>::WORKFLOW_ACTIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.1「必需约束与索引」：
/// - `business_document`：`(document_type, document_no)` 全局唯一（跨域注册表
///   身份，软删除后仍保留身份，避免复用破坏恢复语义）；`document_no` 全局搜索索引；
/// - `document_relation`：`(from_document_id, to_document_id, relation_type)` 唯一，
///   `to_document_id + relation_type` 反向查询索引；
/// - `workflow_action`：`document_id + created_at` 历史索引、`actor_id + created_at`
///   审计索引（§6.1 的 `recorded_at` 由 `BaseModel.created_at` 承载）；
/// - `document_participant`：追加式参与记录（§4.6 只追加不删除）无自然业务唯一键，
///   用 `id` 唯一索引防止重复身份静默写入。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, BUSINESS_DOCUMENTS, business_document_indexes()).await?;
    create_indexes(db, DOCUMENT_RELATIONS, document_relation_indexes()).await?;
    create_indexes(db, DOCUMENT_PARTICIPANTS, document_participant_indexes()).await?;
    create_indexes(db, WORKFLOW_ACTIONS, workflow_action_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `business_document` 的身份约束与全局编号搜索索引。
fn business_document_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_business_documents_identity",
            doc! { "document_type": 1, "document_no": 1 },
        ),
        named_index("idx_business_documents_no", doc! { "document_no": 1 }),
    ]
}

/// 返回 `document_relation` 的关系唯一约束与反向查询索引。
fn document_relation_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_document_relations_link",
            doc! {
                "from_document_id": 1,
                "to_document_id": 1,
                "relation_type": 1,
            },
        ),
        named_index(
            "idx_document_relations_reverse",
            doc! { "to_document_id": 1, "relation_type": 1 },
        ),
    ]
}

/// 返回 `document_participant` 的参与人查询索引。
fn document_participant_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_document_participants_id", doc! { "id": 1 }),
        named_index(
            "idx_document_participants_document_user",
            doc! { "document_id": 1, "participant_user_id": 1 },
        ),
        named_index(
            "idx_document_participants_user",
            doc! { "participant_user_id": 1, "created_at": -1 },
        ),
    ]
}

/// 返回 `workflow_action` 的身份约束、单据历史与操作者审计索引。
fn workflow_action_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_workflow_actions_id", doc! { "id": 1 }),
        named_index(
            "idx_workflow_actions_document_created",
            doc! { "document_id": 1, "created_at": -1 },
        ),
        named_index(
            "idx_workflow_actions_actor_created",
            doc! { "actor_id": 1, "created_at": -1 },
        ),
    ]
}

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        business_document_indexes, document_participant_indexes, document_relation_indexes,
        workflow_action_indexes,
    };

    #[test]
    fn business_document_identity_index_is_globally_unique() {
        let indexes = business_document_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_business_documents_identity")
            })
            .unwrap();
        assert_eq!(identity.keys, doc! { "document_type": 1, "document_no": 1 });
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));
        assert!(identity
            .options
            .as_ref()
            .unwrap()
            .partial_filter_expression
            .is_none());

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "document_no": 1 }));
    }

    #[test]
    fn document_relation_indexes_cover_link_and_reverse_query() {
        let indexes = document_relation_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_document_relations_link")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "to_document_id": 1, "relation_type": 1 } }));
    }

    #[test]
    fn document_participant_indexes_cover_id_and_user_lookups() {
        let indexes = document_participant_indexes();

        let id_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_document_participants_id")
            })
            .unwrap();
        assert_eq!(id_index.options.as_ref().unwrap().unique, Some(true));
        assert!(indexes
            .iter()
            .any(|index| index.keys.contains_key("participant_user_id")));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "document_id": 1, "participant_user_id": 1 }));
    }

    #[test]
    fn workflow_action_indexes_cover_document_and_actor_history() {
        let indexes = workflow_action_indexes();

        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "document_id": 1, "created_at": -1 } }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "actor_id": 1, "created_at": -1 } }));
        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_workflow_actions_id")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }
}
