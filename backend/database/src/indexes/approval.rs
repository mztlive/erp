//! 域 D03 审批定义与运行实例索引。
//!
//! 集合名统一取 [`ApprovalExt`] 关联常量；业务定义版本物理字段固定为
//! `definition_version`，`BaseModel.version` 只承担乐观锁并映射到 API 的
//! `instance_version` / `step_version`。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::ApprovalExt;
use crate::Result;

pub(crate) const APPROVAL_DEFINITIONS: &str = <mongodb::Database as ApprovalExt>::APPROVAL_DEFINITIONS;
pub(crate) const APPROVAL_STEP_DEFINITIONS: &str =
    <mongodb::Database as ApprovalExt>::APPROVAL_STEP_DEFINITIONS;
pub(crate) const APPROVAL_INSTANCES: &str = <mongodb::Database as ApprovalExt>::APPROVAL_INSTANCES;
pub(crate) const APPROVAL_STEP_INSTANCES: &str = <mongodb::Database as ApprovalExt>::APPROVAL_STEP_INSTANCES;

/// 创建四个审批集合的幂等命名索引。
///
/// 落地审批合同的定义版本唯一性、唯一已发布版本、非终态实例唯一性和串行当前
/// 步骤唯一性。部分唯一索引只约束运行状态，终态历史永久保留并允许新提交启动。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 已有数据违反唯一约束或 MongoDB 创建索引失败时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, APPROVAL_DEFINITIONS, approval_definition_indexes()).await?;
    create_indexes(db, APPROVAL_STEP_DEFINITIONS, approval_step_definition_indexes()).await?;
    create_indexes(db, APPROVAL_INSTANCES, approval_instance_indexes()).await?;
    create_indexes(db, APPROVAL_STEP_INSTANCES, approval_step_instance_indexes()).await?;
    Ok(())
}

async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

fn approval_definition_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_definitions_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_definitions_key_version",
            doc! { "definition_key": 1, "definition_version": 1 },
        ),
        unique_partial_index(
            "uk_approval_definitions_published_key",
            doc! { "definition_key": 1 },
            doc! { "status": "PUBLISHED" },
        ),
    ]
}

fn approval_step_definition_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_step_definitions_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_step_definitions_definition_key",
            doc! { "approval_definition_id": 1, "step_key": 1 },
        ),
        unique_index(
            "uk_approval_step_definitions_definition_sequence",
            doc! { "approval_definition_id": 1, "sequence_no": 1 },
        ),
    ]
}

fn approval_instance_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_instances_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_instances_start_idempotency",
            doc! { "definition_key": 1, "start_idempotency_key": 1 },
        ),
        unique_partial_index(
            "uk_approval_instances_non_terminal_subject",
            doc! {
                "definition_key": 1,
                "business_object_type": 1,
                "business_object_id": 1,
                "subject_version": 1,
            },
            doc! {
                "$or": [
                    { "status": "RUNNING" },
                    { "status": "BLOCKED" },
                ]
            },
        ),
        unique_partial_index(
            "uk_approval_instances_external_identity",
            doc! { "runtime_kind": 1, "external_instance_id": 1 },
            doc! { "external_instance_id": { "$type": "string" } },
        ),
        named_index(
            "idx_approval_instances_subject_history",
            doc! {
                "definition_key": 1,
                "business_object_type": 1,
                "business_object_id": 1,
                "subject_version": 1,
                "started_at": -1,
            },
        ),
        named_index(
            "idx_approval_instances_blocked_organization",
            doc! {
                "status": 1,
                "owner_organization_id": 1,
                "blocked_at": 1,
                "created_at": 1,
            },
        ),
        named_index(
            "idx_approval_instances_blocked_company",
            doc! {
                "status": 1,
                "blocked_at": 1,
                "created_at": 1,
            },
        ),
    ]
}

fn approval_step_instance_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_step_instances_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_step_instances_instance_key",
            doc! { "approval_instance_id": 1, "step_key": 1 },
        ),
        unique_partial_index(
            "uk_approval_step_instances_current",
            doc! { "approval_instance_id": 1 },
            doc! {
                "$or": [
                    { "status": "ACTIVE" },
                    { "status": "BLOCKED" },
                ]
            },
        ),
        unique_partial_index(
            "uk_approval_step_instances_external_activity",
            doc! { "approval_instance_id": 1, "external_activity_id": 1 },
            doc! { "external_activity_id": { "$type": "string" } },
        ),
        named_index(
            "idx_approval_step_instances_sequence",
            doc! { "approval_instance_id": 1, "sequence_no": 1 },
        ),
    ]
}

fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).build())
        .build()
}

fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(IndexOptions::builder().name(name.into()).unique(true).build())
        .build()
}

fn unique_partial_index(name: impl Into<String>, keys: Document, filter: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .partial_filter_expression(filter)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        approval_definition_indexes, approval_instance_indexes, approval_step_definition_indexes,
        approval_step_instance_indexes,
    };

    #[test]
    fn definition_business_version_is_unique_and_separate_from_lock_version() {
        let indexes = approval_definition_indexes();
        let version = named(&indexes, "uk_approval_definitions_key_version");
        assert_eq!(
            version.keys,
            doc! { "definition_key": 1, "definition_version": 1 }
        );
        assert!(!version.keys.contains_key("version"));
        assert_eq!(version.options.as_ref().unwrap().unique, Some(true));

        let published = named(&indexes, "uk_approval_definitions_published_key");
        assert_eq!(published.keys, doc! { "definition_key": 1 });
        assert_eq!(
            published.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "status": "PUBLISHED" })
        );
    }

    #[test]
    fn step_definition_identity_and_sequence_are_independently_unique() {
        let indexes = approval_step_definition_indexes();
        for name in [
            "uk_approval_step_definitions_definition_key",
            "uk_approval_step_definitions_definition_sequence",
        ] {
            assert_eq!(named(&indexes, name).options.as_ref().unwrap().unique, Some(true));
        }
        assert_eq!(
            named(&indexes, "uk_approval_step_definitions_definition_key").keys,
            doc! { "approval_definition_id": 1, "step_key": 1 }
        );
        assert_eq!(
            named(&indexes, "uk_approval_step_definitions_definition_sequence").keys,
            doc! { "approval_definition_id": 1, "sequence_no": 1 }
        );
    }

    #[test]
    fn non_terminal_instance_uniqueness_preserves_terminal_history() {
        let indexes = approval_instance_indexes();
        let active = named(&indexes, "uk_approval_instances_non_terminal_subject");
        assert_eq!(active.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            active.keys,
            doc! {
                "definition_key": 1,
                "business_object_type": 1,
                "business_object_id": 1,
                "subject_version": 1,
            }
        );
        assert_eq!(
            active.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! {
                "$or": [
                    { "status": "RUNNING" },
                    { "status": "BLOCKED" },
                ]
            })
        );
    }

    #[test]
    fn start_idempotency_identity_is_permanent_and_unique() {
        let indexes = approval_instance_indexes();
        let idempotency = named(&indexes, "uk_approval_instances_start_idempotency");
        assert_eq!(
            idempotency.keys,
            doc! { "definition_key": 1, "start_idempotency_key": 1 }
        );
        assert_eq!(idempotency.options.as_ref().unwrap().unique, Some(true));
        assert!(idempotency
            .options
            .as_ref()
            .unwrap()
            .partial_filter_expression
            .is_none());
    }

    #[test]
    fn blocked_queue_index_starts_with_status_and_authorized_organization() {
        let indexes = approval_instance_indexes();
        assert_eq!(
            named(&indexes, "idx_approval_instances_blocked_organization").keys,
            doc! {
                "status": 1,
                "owner_organization_id": 1,
                "blocked_at": 1,
                "created_at": 1,
            }
        );
        assert_eq!(
            named(&indexes, "idx_approval_instances_blocked_company").keys,
            doc! {
                "status": 1,
                "blocked_at": 1,
                "created_at": 1,
            }
        );
    }

    #[test]
    fn external_instance_identity_is_unique_only_when_present() {
        let indexes = approval_instance_indexes();
        let external = named(&indexes, "uk_approval_instances_external_identity");
        assert_eq!(
            external.keys,
            doc! { "runtime_kind": 1, "external_instance_id": 1 }
        );
        assert_eq!(external.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            external.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "external_instance_id": { "$type": "string" } })
        );
    }

    #[test]
    fn step_instance_current_uniqueness_covers_active_and_blocked() {
        let indexes = approval_step_instance_indexes();
        let current = named(&indexes, "uk_approval_step_instances_current");
        assert_eq!(current.keys, doc! { "approval_instance_id": 1 });
        assert_eq!(current.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            current.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! {
                "$or": [
                    { "status": "ACTIVE" },
                    { "status": "BLOCKED" },
                ]
            })
        );
        assert_eq!(
            named(&indexes, "uk_approval_step_instances_instance_key").keys,
            doc! { "approval_instance_id": 1, "step_key": 1 }
        );

        let external = named(&indexes, "uk_approval_step_instances_external_activity");
        assert_eq!(
            external.keys,
            doc! { "approval_instance_id": 1, "external_activity_id": 1 }
        );
        assert_eq!(external.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            external.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "external_activity_id": { "$type": "string" } })
        );
    }

    fn named<'a>(indexes: &'a [mongodb::IndexModel], name: &str) -> &'a mongodb::IndexModel {
        indexes
            .iter()
            .find(|index| index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name))
            .unwrap()
    }
}
