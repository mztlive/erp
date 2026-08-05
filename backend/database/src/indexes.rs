use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::{casbin_adapter::CASBIN_RULES, Result};

const ACCOUNTS: &str = "accounts";
const CONSUMERS: &str = "consumers";
const ROLES: &str = "roles";
const AUDIT_LOGS: &str = "audit_logs";

/// 创建当前持久化模型依赖的唯一约束和查询索引。
///
/// 账号在软删除后仍保留原身份，因此 `account` 使用全局唯一索引，避免账号复用
/// 破坏后续恢复语义。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub async fn ensure_indexes(db: &Database) -> Result<()> {
    create_indexes(db, ACCOUNTS, account_indexes()).await?;
    create_indexes(db, CONSUMERS, consumer_indexes()).await?;
    create_indexes(db, ROLES, role_indexes()).await?;
    create_indexes(db, AUDIT_LOGS, audit_log_indexes()).await?;
    create_indexes(db, CASBIN_RULES, casbin_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回账号集合的身份约束和列表查询索引。
fn account_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_accounts_id", doc! { "id": 1 }),
        unique_index("uk_accounts_account", doc! { "account": 1 }),
        named_index(
            "idx_accounts_kind_active_created",
            doc! { "kind": 1, "deleted_at": 1, "created_at": -1 },
        ),
    ]
}

/// 返回消费者集合的身份约束和列表查询索引。
fn consumer_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_consumers_id", doc! { "id": 1 }),
        unique_index("uk_consumers_account", doc! { "account": 1 }),
        named_index(
            "idx_consumers_active_created",
            doc! { "deleted_at": 1, "created_at": -1 },
        ),
    ]
}

/// 返回角色身份约束和可分配角色查询索引。
fn role_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_roles_id", doc! { "id": 1 }),
        named_index(
            "idx_roles_active_enabled",
            doc! { "deleted_at": 1, "disabled": 1 },
        ),
    ]
}

/// 返回审计日志身份约束及时间倒序列表索引。
fn audit_log_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_audit_logs_id", doc! { "id": 1 }),
        named_index(
            "idx_audit_logs_active_created",
            doc! { "deleted_at": 1, "created_at": -1 },
        ),
    ]
}

/// 返回 Casbin 按主体或角色清理 policy 所需的查询索引。
///
/// 规则身份由 MongoDB 内建的 `_id` 唯一索引保证。
fn casbin_indexes() -> Vec<IndexModel> {
    vec![
        named_index(
            "idx_casbin_ptype_value0",
            doc! { "sec": 1, "ptype": 1, "values.0": 1 },
        ),
        named_index(
            "idx_casbin_ptype_value1",
            doc! { "sec": 1, "ptype": 1, "values.1": 1 },
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

    use super::{account_indexes, audit_log_indexes, casbin_indexes};

    #[test]
    fn account_identity_indexes_are_globally_unique() {
        let indexes = account_indexes();

        for name in ["uk_accounts_id", "uk_accounts_account"] {
            let index = indexes
                .iter()
                .find(|index| {
                    index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                })
                .unwrap();
            let options = index.options.as_ref().unwrap();
            assert_eq!(options.unique, Some(true));
            assert!(options.partial_filter_expression.is_none());
        }
    }

    #[test]
    fn casbin_indexes_cover_both_grouping_policy_positions() {
        let indexes = casbin_indexes();

        assert!(indexes.iter().any(|index| index.keys.contains_key("values.0")));
        assert!(indexes.iter().any(|index| index.keys.contains_key("values.1")));
    }

    #[test]
    fn audit_log_indexes_cover_identity_and_default_sort() {
        let indexes = audit_log_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref()) == Some("uk_audit_logs_id")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "deleted_at": 1, "created_at": -1 }));
    }
}
