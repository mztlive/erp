//! 审计日志集合索引。

use mongodb::bson::{Document, doc};
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use crate::repository::{AuditAttemptExt, AuditExt};

/// 幂等注册日志和独立尝试的命名索引，保障 ID 唯一并支持时间线及命令追溯。现有数据违反唯一约束或 MongoDB 创建索引失败时返回原错误。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(<Database as AuditExt>::AUDIT_LOGS).create_indexes(audit_log_indexes()).await?;
    db.collection::<Document>(<Database as AuditAttemptExt>::AUDIT_ATTEMPTS)
        .create_indexes(vec![
            unique_index("uk_audit_attempts_id", doc! { "id": 1 }),
            named_index("idx_audit_attempts_command_created", doc! { "command_id": 1, "created_at": -1 }),
        ])
        .await?;
    Ok(())
}

// 声明审计日志 ID 唯一索引及时间倒序、按资源查时间线所用的索引，供启动注册。
fn audit_log_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_audit_logs_id", doc! { "id": 1 }),
        named_index("idx_audit_logs_active_created", doc! { "deleted_at": 1, "created_at": -1 }),
        named_index(
            "idx_audit_logs_resource_timeline",
            doc! { "resource_type": 1, "resource_id": 1, "deleted_at": 1, "created_at": -1, "id": -1 },
        ),
    ]
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::audit_log_indexes;

    #[test]
    fn audit_log_indexes_cover_identity_and_default_sort() {
        let indexes = audit_log_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref()) == Some("uk_audit_logs_id")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| index.keys == doc! { "deleted_at": 1, "created_at": -1 }));
    }
}
