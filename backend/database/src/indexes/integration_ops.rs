//! 域 D34 `integration_ops` 的索引声明：inbox_message、integration_error_task、reconciliation_difference(+_resolution)。
//!
//! 集合名常量取 `IntegrationOpsExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。
//!
//! 数据模型 §6.21「必需约束与索引」逐条落地（P2 §3.3 对照表见 PR 描述）：
//! - `inbox_message`：`(source_system_id, source_event_id)` 消息层唯一、
//!   非空 `business_fact_key` 在对应事实类型内唯一、`status + received_at` 积压扫描；
//! - `integration_error_task`：同一消息与错误分类只允许一个进行中任务、
//!   `status + owner_role + created_at` 工作队列；
//! - `reconciliation_difference`：对象唯一键、`business_object_type + created_at`
//!   差异查询；
//! - `reconciliation_difference_resolution`：`(reconciliation_difference_id,
//!   resolution_no)` 唯一，历史只追加。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::IntegrationOpsExt;
use crate::Result;

/// `inbox_message` 集合名。
pub(crate) const INBOX_MESSAGES: &str = <mongodb::Database as IntegrationOpsExt>::INBOX_MESSAGES;
/// `integration_error_task` 集合名。
pub(crate) const INTEGRATION_ERROR_TASKS: &str =
    <mongodb::Database as IntegrationOpsExt>::INTEGRATION_ERROR_TASKS;
/// `reconciliation_difference` 集合名。
pub(crate) const RECONCILIATION_DIFFERENCES: &str =
    <mongodb::Database as IntegrationOpsExt>::RECONCILIATION_DIFFERENCES;
/// `reconciliation_difference_resolution` 集合名。
pub(crate) const RECONCILIATION_DIFFERENCE_RESOLUTIONS: &str =
    <mongodb::Database as IntegrationOpsExt>::RECONCILIATION_DIFFERENCE_RESOLUTIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.21「必需约束与索引」；身份类字段使用全局唯一索引
/// （消息身份、业务事实键、差异对象键均不可复用，与 accounts 的 code 处理一致）。
/// `integration_error_task` 的「进行中唯一」使用部分唯一索引，理由见
/// [`integration_error_task_indexes`] 注释。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, INBOX_MESSAGES, inbox_message_indexes()).await?;
    create_indexes(db, INTEGRATION_ERROR_TASKS, integration_error_task_indexes()).await?;
    create_indexes(
        db,
        RECONCILIATION_DIFFERENCES,
        reconciliation_difference_indexes(),
    )
    .await?;
    create_indexes(
        db,
        RECONCILIATION_DIFFERENCE_RESOLUTIONS,
        reconciliation_difference_resolution_indexes(),
    )
    .await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `inbox_message` 的消息去重与积压扫描索引（§6.21）。
///
/// `business_fact_key` 在实体层强制非空（`InboxMessage::new` 校验），
/// 「非空业务事实键在对应事实类型内唯一」因此等价于
/// `(message_type, business_fact_key)` 全局唯一索引，无需部分索引。
fn inbox_message_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_inbox_messages_identity",
            doc! { "source_system_id": 1, "source_event_id": 1 },
        ),
        unique_index(
            "uk_inbox_messages_business_fact",
            doc! { "message_type": 1, "business_fact_key": 1 },
        ),
        named_index(
            "idx_inbox_messages_backlog",
            doc! { "status": 1, "received_at": 1 },
        ),
    ]
}

/// 返回 `integration_error_task` 的进行中唯一约束与工作队列索引（§6.21）。
///
/// §6.21「`(message_id, error_class)` 唯一」与「同一消息和错误分类只允许一个
/// 进行中错误任务」合起来表达：同一消息与错误分类**进行中**（未解决/未关闭）
/// 唯一；终态任务关闭后可再开新任务，且业务对象类失败（无 `message_id`）
/// 不属于消息去重域。因此使用**部分唯一索引**（P2 §2.2 允许，须写明理由）：
/// - 部分过滤条件要求 `message_id` 存在（`$type: "string"`），否则 MongoDB 会
///   把缺省字段视为 `null`，导致不同业务对象任务的 `null` 键互相冲突；
/// - 部分过滤条件只纳入三个进行中状态（`$in`，MongoDB 部分索引不支持 `$nin`/
///   `$not`），终态任务不再占用唯一键，重试可重新开单。
///
/// 回滚方式：若后续业务确认需要「消息 + 分类」全局唯一（终态不释放键），
/// 改为 `unique_index("uk_integration_error_tasks_message_class", ...)` 全量唯一即可。
fn integration_error_task_indexes() -> Vec<IndexModel> {
    vec![
        partial_unique_index(
            "uk_integration_error_tasks_message_class",
            doc! { "message_id": 1, "error_class": 1 },
            doc! {
                "message_id": { "$type": "string" },
                "status": { "$in": ["pending", "auto_retrying", "manual_required"] },
            },
        ),
        named_index(
            "idx_integration_error_tasks_work_queue",
            doc! { "status": 1, "owner_role": 1, "created_at": 1 },
        ),
    ]
}

/// 返回 `reconciliation_difference` 的对象唯一约束与差异查询索引（§6.21）。
fn reconciliation_difference_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_reconciliation_differences_object",
            doc! {
                "business_object_type": 1,
                "business_object_id": 1,
                "difference_type": 1,
            },
        ),
        named_index(
            "idx_reconciliation_differences_object_time",
            doc! { "business_object_type": 1, "created_at": 1 },
        ),
    ]
}

/// 返回 `reconciliation_difference_resolution` 的追加唯一约束与历史查询索引（§6.21）。
///
/// 处理记录不可更新或删除，只追加；`reconciliation_difference_id` 单独建普通索引
/// 支撑「读取某差异全部处理记录」与「取最后一条派生当前状态」的历史查询。
fn reconciliation_difference_resolution_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_reconciliation_difference_resolutions_no",
            doc! { "reconciliation_difference_id": 1, "resolution_no": 1 },
        ),
        named_index(
            "idx_reconciliation_difference_resolutions_difference",
            doc! { "reconciliation_difference_id": 1 },
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

/// 构建命名部分唯一索引（仅匹配 `partial_filter` 的文档参与唯一约束）。
fn partial_unique_index(name: impl Into<String>, keys: Document, partial_filter: Document) -> IndexModel {
    IndexModel::builder()
        .keys(keys)
        .options(
            IndexOptions::builder()
                .name(name.into())
                .unique(true)
                .partial_filter_expression(partial_filter)
                .build(),
        )
        .build()
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        inbox_message_indexes, integration_error_task_indexes, reconciliation_difference_indexes,
        reconciliation_difference_resolution_indexes,
    };

    #[test]
    fn inbox_message_identity_and_business_fact_indexes_are_unique() {
        let indexes = inbox_message_indexes();

        let identity = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_inbox_messages_identity")
            })
            .unwrap();
        assert_eq!(
            identity.keys,
            doc! { "source_system_id": 1, "source_event_id": 1 }
        );
        assert_eq!(identity.options.as_ref().unwrap().unique, Some(true));

        let business_fact = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_inbox_messages_business_fact")
            })
            .unwrap();
        assert_eq!(
            business_fact.keys,
            doc! { "message_type": 1, "business_fact_key": 1 }
        );
        assert_eq!(business_fact.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "status": 1, "received_at": 1 }));
    }

    #[test]
    fn error_task_unique_index_is_partial_to_active_message_tasks() {
        let indexes = integration_error_task_indexes();

        let message_class = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_integration_error_tasks_message_class")
            })
            .unwrap();
        assert_eq!(message_class.keys, doc! { "message_id": 1, "error_class": 1 });
        let options = message_class.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        assert_eq!(
            options.partial_filter_expression,
            Some(doc! {
                "message_id": { "$type": "string" },
                "status": { "$in": ["pending", "auto_retrying", "manual_required"] },
            })
        );

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "status": 1, "owner_role": 1, "created_at": 1 }));
    }

    #[test]
    fn reconciliation_indexes_cover_object_key_and_history_append_order() {
        let difference_indexes = reconciliation_difference_indexes();
        assert!(difference_indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_reconciliation_differences_object")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(difference_indexes
            .iter()
            .any(|index| { index.keys == doc! { "business_object_type": 1, "created_at": 1 } }));

        let resolution_indexes = reconciliation_difference_resolution_indexes();
        let sequence = resolution_indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_reconciliation_difference_resolutions_no")
            })
            .unwrap();
        assert_eq!(
            sequence.keys,
            doc! { "reconciliation_difference_id": 1, "resolution_no": 1 }
        );
        assert_eq!(sequence.options.as_ref().unwrap().unique, Some(true));
        assert!(resolution_indexes
            .iter()
            .any(|index| index.keys == doc! { "reconciliation_difference_id": 1 }));
    }
}
