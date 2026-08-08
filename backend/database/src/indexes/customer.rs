//! 域 D08 `customer` 的索引声明：customer_account、customer_assignment
//! （数据模型 §6.2）。
//!
//! 集合名常量取 `CustomerExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::CustomerExt;
use crate::Result;

/// `customer_account` 集合名。
pub(crate) const CUSTOMER_ACCOUNTS: &str = <mongodb::Database as CustomerExt>::CUSTOMER_ACCOUNTS;
/// `customer_assignment` 集合名。
pub(crate) const CUSTOMER_ASSIGNMENTS: &str = <mongodb::Database as CustomerExt>::CUSTOMER_ASSIGNMENTS;
/// `customer_profile_command` 集合名。
pub(crate) const CUSTOMER_PROFILE_COMMANDS: &str =
    <mongodb::Database as CustomerExt>::CUSTOMER_PROFILE_COMMANDS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.2「必需约束与索引」：一个 `party` 至多一个客户角色
/// （`party_id` 全局唯一）；客户编号全局唯一。
///
/// 身份类字段使用**全局唯一索引**（与 accounts 的处理一致）：`customer_account`
/// 软删除后仍保留身份（编号/主体归属），避免复用破坏恢复与历史单据追溯语义。
/// 「同一客户、用户、角色的有效期不得重叠」是跨行业务约束，由 P3 事务在写入
/// 前校验；本层用 `(customer_id, user_id, assignment_role, valid_from)`
/// 唯一索引兜底拒绝**完全相同**的重复归属行（数据治理，防止重复事实行）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, CUSTOMER_ACCOUNTS, customer_account_indexes()).await?;
    create_indexes(db, CUSTOMER_ASSIGNMENTS, customer_assignment_indexes()).await?;
    create_indexes(db, CUSTOMER_PROFILE_COMMANDS, customer_profile_command_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `customer_account` 的身份约束和列表查询索引。
fn customer_account_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_customer_accounts_party", doc! { "party_id": 1 }),
        unique_index("uk_customer_accounts_customer_no", doc! { "customer_no": 1 }),
        named_index("idx_customer_accounts_status", doc! { "status": 1 }),
    ]
}

/// 返回 `customer_assignment` 的重复行约束与「我的客户」查询索引。
fn customer_assignment_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_customer_assignments_window",
            doc! {
                "customer_id": 1,
                "user_id": 1,
                "assignment_role": 1,
                "valid_from": 1,
            },
        ),
        named_index(
            "idx_customer_assignments_user",
            doc! { "user_id": 1, "valid_to": 1 },
        ),
        named_index(
            "idx_customer_assignments_customer",
            doc! { "customer_id": 1, "assignment_role": 1, "valid_from": 1 },
        ),
    ]
}

/// 返回客户资料根级保存命令的幂等唯一约束。
fn customer_profile_command_indexes() -> Vec<IndexModel> {
    vec![unique_index(
        "uk_customer_profile_commands_idempotency_key",
        doc! { "idempotency_key": 1 },
    )]
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

    use super::{customer_account_indexes, customer_assignment_indexes, customer_profile_command_indexes};

    #[test]
    fn customer_account_identity_indexes_are_globally_unique() {
        let indexes = customer_account_indexes();

        for name in ["uk_customer_accounts_party", "uk_customer_accounts_customer_no"] {
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

        assert!(indexes.iter().any(|index| index.keys == doc! { "status": 1 }));
    }

    #[test]
    fn customer_assignment_indexes_cover_dedup_and_user_query() {
        let indexes = customer_assignment_indexes();

        let window = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_customer_assignments_window")
            })
            .unwrap();
        assert_eq!(
            window.keys,
            doc! {
                "customer_id": 1,
                "user_id": 1,
                "assignment_role": 1,
                "valid_from": 1,
            }
        );
        assert_eq!(window.options.as_ref().unwrap().unique, Some(true));

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "user_id": 1, "valid_to": 1 }));
    }

    #[test]
    fn customer_profile_command_key_is_unique() {
        let indexes = customer_profile_command_indexes();
        assert_eq!(indexes.len(), 1);
        assert_eq!(indexes[0].keys, doc! { "idempotency_key": 1 });
        assert_eq!(indexes[0].options.as_ref().unwrap().unique, Some(true));
    }
}
