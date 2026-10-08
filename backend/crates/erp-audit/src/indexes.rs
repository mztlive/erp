//! 审计日志集合索引。

use mongodb::bson::{Document, doc};
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;

use crate::repository::AuditAttemptExt;

const AUDIT_LOGS: &str = "audit_logs";

/// 为 `audit_logs` 与 `audit_attempts` 创建幂等命名索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库。
///
/// # 返回
/// 两个集合的索引都创建完成时返回 `Ok(())`。
///
/// # 错误
/// 已有数据违反唯一约束，或 MongoDB 无法创建索引时返回错误。
pub async fn ensure(db: &Database) -> Result<()> {
    db.collection::<Document>(AUDIT_LOGS).create_indexes(audit_log_indexes()).await?;
    db.collection::<Document>(<Database as AuditAttemptExt>::AUDIT_ATTEMPTS)
        .create_indexes(vec![
            unique_index("uk_audit_attempts_id", doc! { "id": 1 }),
            named_index("idx_audit_attempts_command_created", doc! { "command_id": 1, "created_at": -1 }),
        ])
        .await?;
    Ok(())
}

/// 返回审计日志身份约束及时间倒序列表索引。
fn audit_log_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_audit_logs_id", doc! { "id": 1 }),
        named_index("idx_audit_logs_active_created", doc! { "deleted_at": 1, "created_at": -1 }),
    ]
}

/// 构建命名普通索引。
fn named_index(name: impl Into<String>, keys: Document) -> IndexModel {
    build_index(name, keys, false)
}

/// 构建命名唯一索引。
fn unique_index(name: impl Into<String>, keys: Document) -> IndexModel {
    build_index(name, keys, true)
}

/// 索引构造的唯一入口（erp-audit-009）。
///
/// 普通索引与唯一索引仅 `unique` 选项不同；索引名与键保持不变。
fn build_index(name: impl Into<String>, keys: Document, unique: bool) -> IndexModel {
    let name = name.into();
    let options = if unique {
        IndexOptions::builder().name(name).unique(true).build()
    } else {
        IndexOptions::builder().name(name).build()
    };
    IndexModel::builder().keys(keys).options(options).build()
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
