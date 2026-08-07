//! 域 D07 `party` 的索引声明：party、party_revision、party_contact、
//! party_address、party_tax_profile、party_bank_account（数据模型 §6.2）。
//!
//! 集合名常量取 `PartyExt` 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 均为冻结声明下的私有子树，模块路径无法互相引用，
//! 关联常量随 trait 公开可达，两侧共用同一值，禁止字面量重复。

use mongodb::{
    bson::{doc, Document},
    options::IndexOptions,
    Database, IndexModel,
};

use crate::repository::extensions::PartyExt;
use crate::Result;

/// `party` 集合名。
pub(crate) const PARTIES: &str = <mongodb::Database as PartyExt>::PARTIES;
/// `party_revision` 集合名。
pub(crate) const PARTY_REVISIONS: &str = <mongodb::Database as PartyExt>::PARTY_REVISIONS;
/// `party_contact` 集合名。
pub(crate) const PARTY_CONTACTS: &str = <mongodb::Database as PartyExt>::PARTY_CONTACTS;
/// `party_address` 集合名。
pub(crate) const PARTY_ADDRESSES: &str = <mongodb::Database as PartyExt>::PARTY_ADDRESSES;
/// `party_tax_profile` 集合名。
pub(crate) const PARTY_TAX_PROFILES: &str = <mongodb::Database as PartyExt>::PARTY_TAX_PROFILES;
/// `party_bank_account` 集合名。
pub(crate) const PARTY_BANK_ACCOUNTS: &str = <mongodb::Database as PartyExt>::PARTY_BANK_ACCOUNTS;

/// 创建本域集合的幂等命名索引。
///
/// 逐条落地数据模型 §6.2「必需约束与索引」：`party_no` 与规范化统一信用
/// 代码全局唯一；`(party_id, revision_no)` 唯一；法定名称/简称搜索索引；
/// 银行账户编号与 `(party_id, 账号HMAC)` 唯一。
///
/// 身份类字段使用**全局唯一索引**（与 accounts 的处理一致）：`party` 软
/// 删除后仍保留身份（编号/信用代码），避免复用破坏恢复与历史追溯语义。
/// 统一信用代码允许为空（历史数据），MongoDB 唯一索引把缺失字段视为 null，
/// 直接建全局唯一会拒绝多个空值，因此采用**部分唯一索引**只约束非空代码；
/// 回滚方式：清空该集合后删除并重建索引，或改为 Service 层查重（不推荐）。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
///
/// # 错误
/// 当已有数据违反唯一约束或 MongoDB 无法创建索引时返回错误。
pub(crate) async fn ensure(db: &Database) -> Result<()> {
    create_indexes(db, PARTIES, party_indexes()).await?;
    create_indexes(db, PARTY_REVISIONS, party_revision_indexes()).await?;
    create_indexes(db, PARTY_CONTACTS, party_contact_indexes()).await?;
    create_indexes(db, PARTY_ADDRESSES, party_address_indexes()).await?;
    create_indexes(db, PARTY_TAX_PROFILES, party_tax_profile_indexes()).await?;
    create_indexes(db, PARTY_BANK_ACCOUNTS, party_bank_account_indexes()).await?;
    Ok(())
}

/// 为单个集合创建一组幂等命名索引。
async fn create_indexes(db: &Database, collection: &str, indexes: Vec<IndexModel>) -> Result<()> {
    db.collection::<Document>(collection)
        .create_indexes(indexes)
        .await?;
    Ok(())
}

/// 返回 `party` 的身份约束和列表查询索引。
fn party_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_parties_party_no", doc! { "party_no": 1 }),
        partial_unique_index(
            "uk_parties_credit_code",
            doc! { "unified_credit_code": 1 },
            doc! { "unified_credit_code": { "$type": "string" } },
        ),
        named_index("idx_parties_kind_status", doc! { "party_kind": 1, "status": 1 }),
    ]
}

/// 返回 `party_revision` 的版本唯一约束与名称搜索索引。
fn party_revision_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_party_revisions_party_revision",
            doc! { "party_id": 1, "revision_no": 1 },
        ),
        named_index(
            "idx_party_revisions_names",
            doc! { "legal_name": 1, "short_name": 1 },
        ),
    ]
}

/// 返回 `party_contact` 的主体/状态列表与手机指纹查询索引。
fn party_contact_indexes() -> Vec<IndexModel> {
    vec![
        named_index(
            "idx_party_contacts_party_status",
            doc! { "party_id": 1, "status": 1, "is_default": 1 },
        ),
        named_index("idx_party_contacts_mobile_hmac", doc! { "mobile_query_hmac": 1 }),
    ]
}

/// 返回 `party_address` 的主体/类型列表索引。
fn party_address_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_party_addresses_party_type",
        doc! { "party_id": 1, "address_type": 1, "status": 1 },
    )]
}

/// 返回 `party_tax_profile` 的主体列表索引。
fn party_tax_profile_indexes() -> Vec<IndexModel> {
    vec![named_index(
        "idx_party_tax_profiles_party",
        doc! { "party_id": 1, "status": 1, "is_default": 1 },
    )]
}

/// 返回 `party_bank_account` 的身份约束和列表查询索引。
fn party_bank_account_indexes() -> Vec<IndexModel> {
    vec![
        unique_index(
            "uk_party_bank_accounts_bank_account_no",
            doc! { "bank_account_no": 1 },
        ),
        unique_index(
            "uk_party_bank_accounts_party_hmac",
            doc! { "party_id": 1, "account_number_query_hmac": 1 },
        ),
        named_index(
            "idx_party_bank_accounts_party_status",
            doc! { "party_id": 1, "status": 1, "is_default": 1 },
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

/// 构建命名部分唯一索引。
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
    use mongodb::bson::{doc, Bson};

    use super::{party_bank_account_indexes, party_indexes, party_revision_indexes};

    #[test]
    fn party_identity_indexes_are_globally_unique_with_partial_credit_code() {
        let indexes = party_indexes();

        let party_no = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_parties_party_no")
            })
            .unwrap();
        assert_eq!(party_no.keys, doc! { "party_no": 1 });
        assert_eq!(party_no.options.as_ref().unwrap().unique, Some(true));
        assert!(party_no
            .options
            .as_ref()
            .unwrap()
            .partial_filter_expression
            .is_none());

        let credit_code = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_parties_credit_code")
            })
            .unwrap();
        assert_eq!(credit_code.keys, doc! { "unified_credit_code": 1 });
        let options = credit_code.options.as_ref().unwrap();
        assert_eq!(options.unique, Some(true));
        let partial = options.partial_filter_expression.as_ref().unwrap();
        assert!(matches!(
            partial.get("unified_credit_code"),
            Some(Bson::Document(_))
        ));
    }

    #[test]
    fn party_revision_indexes_cover_identity_and_names() {
        let indexes = party_revision_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_party_revisions_party_revision")
                && index.keys == doc! { "party_id": 1, "revision_no": 1 }
        }));
        assert!(indexes
            .iter()
            .any(|index| { index.keys == doc! { "legal_name": 1, "short_name": 1 } }));
    }

    #[test]
    fn bank_account_identity_indexes_cover_number_and_hmac_pair() {
        let indexes = party_bank_account_indexes();

        assert!(indexes.iter().any(|index| {
            index.options.as_ref().and_then(|options| options.name.as_deref())
                == Some("uk_party_bank_accounts_bank_account_no")
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
        assert!(indexes.iter().any(|index| {
            index.keys
                == doc! {
                    "party_id": 1,
                    "account_number_query_hmac": 1,
                }
                && index.options.as_ref().and_then(|options| options.unique) == Some(true)
        }));
    }
}
