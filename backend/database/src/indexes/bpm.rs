//! BPM 目标集合索引。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::BpmExt;
use crate::Result;

const DEFINITIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_PROCESS_DEFINITIONS;
const NODE_DEFINITIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_NODE_DEFINITIONS;
const TRANSITION_DEFINITIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_TRANSITION_DEFINITIONS;
const INSTANCES: &str = <mongodb::Database as BpmExt>::APPROVAL_PROCESS_INSTANCES;
const EXECUTIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_NODE_EXECUTIONS;
const ASSIGNEES: &str = <mongodb::Database as BpmExt>::APPROVAL_INSTANCE_ASSIGNEES;
const RECEIPTS: &str = <mongodb::Database as BpmExt>::APPROVAL_COMMAND_RECEIPTS;

/// 为目标 BPM 集合创建幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 既有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, DEFINITIONS, definition_indexes()).await?;
    create_indexes(db, NODE_DEFINITIONS, node_definition_indexes()).await?;
    create_indexes(db, TRANSITION_DEFINITIONS, transition_definition_indexes()).await?;
    create_indexes(db, INSTANCES, instance_indexes()).await?;
    create_indexes(db, EXECUTIONS, execution_indexes()).await?;
    create_indexes(db, ASSIGNEES, assignee_indexes()).await?;
    create_indexes(db, RECEIPTS, receipt_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

fn definition_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_process_definitions_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_process_definitions_kind_version",
            doc! { "process_kind": 1, "definition_version": 1 },
        ),
        unique_partial_index(
            "uk_approval_process_definitions_published_kind",
            doc! { "process_kind": 1 },
            doc! { "status": "PUBLISHED" },
        ),
        unique_partial_index(
            "uk_approval_process_definitions_active_draft_kind",
            doc! { "process_kind": 1 },
            doc! { "status": "DRAFT" },
        ),
        named_index(
            "idx_approval_process_definitions_history",
            doc! { "process_kind": 1, "definition_version": -1 },
        ),
    ]
}

fn node_definition_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_node_definitions_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_node_definitions_definition_key",
            doc! { "process_definition_id": 1, "node_key": 1 },
        ),
        unique_index(
            "uk_approval_node_definitions_definition_order",
            doc! { "process_definition_id": 1, "display_order": 1 },
        ),
        named_index(
            "idx_approval_node_definitions_definition",
            doc! { "process_definition_id": 1, "display_order": 1, "node_key": 1 },
        ),
    ]
}

fn transition_definition_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_transition_definitions_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_transition_definitions_from_event",
            doc! {
                "process_definition_id": 1,
                "from_node_key": 1,
                "event": 1,
            },
        ),
        named_index(
            "idx_approval_transition_definitions_definition",
            doc! { "process_definition_id": 1, "from_node_key": 1, "event": 1 },
        ),
    ]
}

fn instance_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_process_instances_id", doc! { "id": 1 }),
        unique_partial_index(
            "uk_approval_process_instances_active_subject",
            doc! {
                "subject.subject_kind": 1,
                "subject.subject_id": 1,
                "subject_version": 1,
            },
            running_or_blocked_filter(),
        ),
        named_index(
            "idx_approval_process_instances_subject_history",
            doc! {
                "subject.subject_kind": 1,
                "subject.subject_id": 1,
                "started_at": -1,
            },
        ),
        named_index(
            "idx_approval_process_instances_blocked",
            doc! { "status": 1, "blocked_at": -1, "id": -1 },
        ),
        named_index(
            "idx_approval_process_instances_started_by",
            doc! { "started_by": 1, "started_at": -1, "id": -1 },
        ),
        named_index(
            "idx_approval_process_instances_updated",
            doc! { "updated_at": -1, "id": -1 },
        ),
        named_index(
            "idx_approval_process_instances_status_updated",
            doc! { "status": 1, "updated_at": -1, "id": -1 },
        ),
    ]
}

fn execution_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_node_executions_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_node_executions_instance_no",
            doc! { "process_instance_id": 1, "execution_no": 1 },
        ),
        named_index(
            "idx_approval_node_executions_round_node",
            doc! {
                "process_instance_id": 1,
                "round_no": 1,
                "node_key": 1,
                "execution_no": 1,
            },
        ),
        unique_partial_index(
            "uk_approval_node_executions_current",
            doc! { "process_instance_id": 1 },
            active_or_blocked_filter(),
        ),
        named_index(
            "idx_approval_node_executions_round",
            doc! { "process_instance_id": 1, "round_no": 1, "execution_no": 1 },
        ),
        named_index(
            "idx_approval_node_executions_assignee",
            doc! { "assignee_participant_id": 1, "status": 1, "activated_at": 1 },
        ),
    ]
}

fn assignee_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_instance_assignees_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_instance_assignees_instance_node",
            doc! { "process_instance_id": 1, "node_key": 1 },
        ),
    ]
}

fn receipt_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_approval_command_receipts_id", doc! { "id": 1 }),
        unique_index(
            "uk_approval_command_receipts_idempotency",
            doc! { "command_kind": 1, "scope_id": 1, "idempotency_key": 1 },
        ),
    ]
}

fn running_or_blocked_filter() -> Document {
    doc! {
        "$or": [
            { "status": "RUNNING" },
            { "status": "BLOCKED" },
        ]
    }
}

fn active_or_blocked_filter() -> Document {
    doc! {
        "$or": [
            { "status": "ACTIVE" },
            { "status": "BLOCKED" },
        ]
    }
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
        assignee_indexes, definition_indexes, execution_indexes, instance_indexes, node_definition_indexes,
        receipt_indexes, transition_definition_indexes,
    };

    #[test]
    fn definition_indexes_cover_published_and_active_draft_partial_uniques() {
        let indexes = definition_indexes();
        let published = index_named(&indexes, "uk_approval_process_definitions_published_kind");
        assert_eq!(published.keys, doc! { "process_kind": 1 });
        assert_eq!(published.options.as_ref().unwrap().unique, Some(true));
        assert_eq!(
            published.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "status": "PUBLISHED" })
        );

        let draft = index_named(&indexes, "uk_approval_process_definitions_active_draft_kind");
        assert_eq!(
            draft.options.as_ref().unwrap().partial_filter_expression,
            Some(doc! { "status": "DRAFT" })
        );
        assert_eq!(
            index_named(&indexes, "uk_approval_process_definitions_kind_version").keys,
            doc! { "process_kind": 1, "definition_version": 1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_process_definitions_history").keys,
            doc! { "process_kind": 1, "definition_version": -1 }
        );
    }

    #[test]
    fn node_and_transition_indexes_use_definition_prefix() {
        let nodes = node_definition_indexes();
        assert_eq!(
            index_named(&nodes, "uk_approval_node_definitions_definition_key").keys,
            doc! { "process_definition_id": 1, "node_key": 1 }
        );
        assert_eq!(
            index_named(&nodes, "uk_approval_node_definitions_definition_order").keys,
            doc! { "process_definition_id": 1, "display_order": 1 }
        );
        assert_eq!(
            index_named(&nodes, "idx_approval_node_definitions_definition").keys,
            doc! { "process_definition_id": 1, "display_order": 1, "node_key": 1 }
        );
        assert_ne!(
            index_named(&nodes, "idx_approval_node_definitions_definition")
                .options
                .as_ref()
                .and_then(|options| options.unique),
            Some(true)
        );

        let transitions = transition_definition_indexes();
        assert_eq!(
            index_named(&transitions, "uk_approval_transition_definitions_from_event").keys,
            doc! {
                "process_definition_id": 1,
                "from_node_key": 1,
                "event": 1,
            }
        );
        assert_eq!(
            index_named(&transitions, "idx_approval_transition_definitions_definition").keys,
            doc! { "process_definition_id": 1, "from_node_key": 1, "event": 1 }
        );
        assert_ne!(
            index_named(&transitions, "idx_approval_transition_definitions_definition")
                .options
                .as_ref()
                .and_then(|options| options.unique),
            Some(true)
        );
    }

    #[test]
    fn instance_indexes_exclude_definition_id_and_external_runtime() {
        let indexes = instance_indexes();
        let active = index_named(&indexes, "uk_approval_process_instances_active_subject");
        assert_eq!(
            active.keys,
            doc! {
                "subject.subject_kind": 1,
                "subject.subject_id": 1,
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
        assert!(!indexes.iter().any(|index| {
            index.keys.contains_key("runtime_kind") || index.keys.contains_key("external_instance_id")
        }));
        assert_eq!(
            index_named(&indexes, "idx_approval_process_instances_blocked").keys,
            doc! { "status": 1, "blocked_at": -1, "id": -1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_process_instances_started_by").keys,
            doc! { "started_by": 1, "started_at": -1, "id": -1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_process_instances_updated").keys,
            doc! { "updated_at": -1, "id": -1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_process_instances_status_updated").keys,
            doc! { "status": 1, "updated_at": -1, "id": -1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_process_instances_subject_history").keys,
            doc! {
                "subject.subject_kind": 1,
                "subject.subject_id": 1,
                "started_at": -1,
            }
        );
        assert_ne!(
            index_named(&indexes, "idx_approval_process_instances_subject_history")
                .options
                .as_ref()
                .and_then(|options| options.unique),
            Some(true)
        );
    }

    #[test]
    fn execution_current_token_is_partial_unique_and_round_history_is_not() {
        let indexes = execution_indexes();
        let current = index_named(&indexes, "uk_approval_node_executions_current");
        assert_eq!(current.keys, doc! { "process_instance_id": 1 });
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
        let history = index_named(&indexes, "idx_approval_node_executions_round_node");
        assert_ne!(
            history.options.as_ref().and_then(|options| options.unique),
            Some(true)
        );
        assert_eq!(
            history.keys,
            doc! {
                "process_instance_id": 1,
                "round_no": 1,
                "node_key": 1,
                "execution_no": 1,
            }
        );
        assert_eq!(
            index_named(&indexes, "uk_approval_node_executions_instance_no").keys,
            doc! { "process_instance_id": 1, "execution_no": 1 }
        );
        assert_eq!(
            index_named(&indexes, "uk_approval_node_executions_instance_no")
                .options
                .as_ref()
                .unwrap()
                .unique,
            Some(true)
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_node_executions_round").keys,
            doc! { "process_instance_id": 1, "round_no": 1, "execution_no": 1 }
        );
        assert_eq!(
            index_named(&indexes, "idx_approval_node_executions_assignee").keys,
            doc! { "assignee_participant_id": 1, "status": 1, "activated_at": 1 }
        );
    }

    #[test]
    fn assignee_and_receipt_uniques_match_contract_keys() {
        assert_eq!(
            index_named(
                &assignee_indexes(),
                "uk_approval_instance_assignees_instance_node"
            )
            .keys,
            doc! { "process_instance_id": 1, "node_key": 1 }
        );
        assert_eq!(
            index_named(&receipt_indexes(), "uk_approval_command_receipts_idempotency").keys,
            doc! { "command_kind": 1, "scope_id": 1, "idempotency_key": 1 }
        );
        assert_eq!(
            index_named(&receipt_indexes(), "uk_approval_command_receipts_idempotency")
                .options
                .as_ref()
                .unwrap()
                .unique,
            Some(true)
        );
    }

    fn index_named<'a>(indexes: &'a [mongodb::IndexModel], name: &str) -> &'a mongodb::IndexModel {
        indexes
            .iter()
            .find(|index| index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name))
            .unwrap_or_else(|| panic!("missing index {name}"))
    }
}
