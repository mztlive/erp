//! 域 D12 `contract` 的索引声明：contract、contract_revision（页面：W04）。
//!
//! 集合名常量取 `ContractExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::ContractExt;
use crate::Result;

/// `contract` 集合名。
pub(crate) const CONTRACTS: &str = <mongodb::Database as ContractExt>::CONTRACTS;
/// `contract_revision` 集合名。
pub(crate) const CONTRACT_REVISIONS: &str = <mongodb::Database as ContractExt>::CONTRACT_REVISIONS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.4「必需约束与索引」。`contract_no` 是身份类字段，使用
/// **全局唯一索引**（与 accounts 的 code 处理一致）：软删除后仍保留编号，避免
/// 复用破坏合同追溯与恢复语义。`customer_id + status + valid_to` 跨两张表：
/// `valid_to` 保存在不可变版本上，因此拆为合同主表的
/// `customer_id + status` 与版本表的 `contract_id + valid_to` 两个索引。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, CONTRACTS, contract_indexes()).await?;
    create_indexes(db, CONTRACT_REVISIONS, contract_revision_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `contract` 的身份约束和列表查询索引。
fn contract_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_contracts_contract_no", doc! { "contract_no": 1 }),
        named_index(
            "idx_contracts_customer_status",
            doc! { "customer_id": 1, "status": 1 },
        ),
    ]
}

/// 返回 `contract_revision` 的版本唯一约束与有效期查询索引。
fn contract_revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_contract_revisions_contract_revision",
            doc! { "contract_id": 1, "revision_no": 1 },
        ),
        named_index(
            "idx_contract_revisions_validity",
            doc! { "contract_id": 1, "valid_to": 1 },
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

    use super::{contract_indexes, contract_revision_indexes};

    #[test]
    fn contract_identity_index_is_globally_unique() {
        let indexes = contract_indexes();

        let no_index = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_contracts_contract_no")
            })
            .unwrap();
        assert_eq!(no_index.keys, doc! { "contract_no": 1 });
        assert_eq!(no_index.options.as_ref().unwrap().unique, Some(true));
        assert!(no_index
            .options
            .as_ref()
            .unwrap()
            .partial_filter_expression
            .is_none());

        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "customer_id": 1, "status": 1 }));
    }

    #[test]
    fn contract_revision_indexes_cover_version_uniqueness_and_validity() {
        let indexes = contract_revision_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_contract_revisions_contract_revision")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
                && index.keys == doc! { "contract_id": 1, "revision_no": 1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| index.keys == doc! { "contract_id": 1, "valid_to": 1 }));
    }
}
