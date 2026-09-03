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
///
/// `uk_parties_id` 覆盖业务主键 `id` 的精确与 `$in` 批量读取（PROC-R10：
/// `current_legal_names_by_party_ids` 经 `list_by_ids` 按 `id $in` 批量取主体，
/// 默认 `_id` 索引不能覆盖业务字段 `id`，此前只能集合扫描）。
/// 迁移：先按 `id` 分组审计重复值（`$group`/`$match: {count: {$gt: 1}}` 为空
/// 才可继续），再执行幂等 `ensure` 创建索引，最后用 `explain` 验证 `$in`
/// 命中 `uk_parties_id` 且无 `COLLSCAN`。
/// 回滚：删除 `uk_parties_id`，批量查询退化为集合扫描，不改变数据。
/// 失败关闭：存量存在重复 `id` 时 `ensure` 返回唯一冲突错误，部署必须中止，
/// 先按审计诊断清理重复后再重跑，禁止跳过审计强行建索引。
fn party_indexes() -> Vec<IndexModel> {
    vec![
        unique_index("uk_parties_id", doc! { "id": 1 }),
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

        for name in ["uk_parties_id", "uk_parties_party_no"] {
            let index = indexes
                .iter()
                .find(|index| {
                    index.options.as_ref().and_then(|options| options.name.as_deref()) == Some(name)
                })
                .unwrap();
            assert_eq!(index.options.as_ref().unwrap().unique, Some(true));
            assert!(index
                .options
                .as_ref()
                .unwrap()
                .partial_filter_expression
                .is_none());
        }

        let party_no = indexes
            .iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref())
                    == Some("uk_parties_party_no")
            })
            .unwrap();
        assert_eq!(party_no.keys, doc! { "party_no": 1 });

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
    fn party_id_index_covers_batch_lookups() {
        let index = party_indexes()
            .into_iter()
            .find(|index| {
                index.options.as_ref().and_then(|options| options.name.as_deref()) == Some("uk_parties_id")
            })
            .unwrap();
        assert_eq!(index.keys, doc! { "id": 1 });
        assert_eq!(
            index.options.as_ref().and_then(|options| options.unique),
            Some(true)
        );
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

/// PROC-R10 主体业务主键索引的真实 MongoDB 验收（隔离库，Quality 单独执行）。
#[cfg(test)]
mod proc_r10_mongo_tests {
    use mongodb::bson::{doc, Document};
    use serde::Deserialize;
    use test_support::{require_mongo, TestDb};

    use super::{ensure, PARTIES};

    /// 重复 `id` 审计行。
    #[derive(Debug, Deserialize, PartialEq, Eq)]
    struct DuplicatePartyId {
        /// 重复的业务主键。
        id: String,
        /// 该主键出现次数。
        count: i64,
    }

    /// 插入仅携带索引相关字段的主体原始文档。
    ///
    /// # 参数
    /// * `db` - 隔离测试库
    /// * `id` - 业务主键（可故意重复）
    /// * `party_no` - 主体编号（保持唯一，避免干扰其他唯一索引）
    ///
    /// # 错误
    /// 写入失败时 panic。
    async fn insert_raw_party(db: &mongodb::Database, id: &str, party_no: &str) {
        db.collection::<Document>(PARTIES)
            .insert_one(doc! { "id": id, "party_no": party_no })
            .await
            .expect("原始主体写入失败");
    }

    /// 按 `id` 分组审计重复值，与部署前检查共用同一语义。
    ///
    /// # 参数
    /// * `db` - 隔离测试库
    ///
    /// # 返回
    /// 按 `id` 字典序排列的重复主键及出现次数。
    ///
    /// # 错误
    /// 聚合执行失败时 panic。
    async fn audit_duplicate_party_ids(db: &mongodb::Database) -> Vec<DuplicatePartyId> {
        use futures_util::TryStreamExt;
        db.collection::<Document>(PARTIES)
            .aggregate(vec![
                doc! { "$group": { "_id": "$id", "count": { "$sum": 1 } } },
                doc! { "$match": { "count": { "$gt": 1 } } },
                doc! { "$sort": { "_id": 1 } },
                doc! { "$project": { "_id": 0, "id": "$_id", "count": 1 } },
            ])
            .with_type::<DuplicatePartyId>()
            .await
            .expect("重复审计聚合失败")
            .try_collect::<Vec<_>>()
            .await
            .expect("重复审计游标读取失败")
    }

    /// 重复 `id` 必须先被审计报出，再拒绝索引迁移并输出冲突索引诊断。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// 审计命中重复主键且迁移失败关闭时通过。
    ///
    /// # 错误
    /// 审计漏报或迁移未拒绝时测试失败。
    ///
    /// # 约束
    /// 仅验证 `parties` 集合；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn duplicate_party_ids_are_audited_and_refuse_migration() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_r10_party_id_dup")
                .await
                .expect("测试数据库创建失败");
            insert_raw_party(fixture.db(), "dup-1", "P-DUP-1").await;
            insert_raw_party(fixture.db(), "dup-1", "P-DUP-2").await;

            let duplicates = audit_duplicate_party_ids(fixture.db()).await;
            assert_eq!(
                duplicates,
                vec![DuplicatePartyId {
                    id: "dup-1".to_string(),
                    count: 2
                }],
                "部署前审计必须报出重复 id"
            );

            let err = ensure(fixture.db()).await.expect_err("重复 id 必须拒绝建索引");
            let rendered = format!("{err:?}");
            assert!(
                rendered.contains("uk_parties_id"),
                "诊断必须包含冲突索引名：{rendered}"
            );
        });
    }

    /// `id $in` 批量查询的执行计划必须命中唯一索引且无集合扫描。
    ///
    /// # 参数
    /// 无，内部创建隔离库。
    ///
    /// # 返回
    /// `explain` 命中 `uk_parties_id` 的 `IXSCAN` 且无 `COLLSCAN` 时通过。
    ///
    /// # 错误
    /// 索引未命中或出现集合扫描时测试失败。
    ///
    /// # 约束
    /// 不使用 `hint`；`#[ignore]` 由 Quality 在隔离副本集执行。
    #[tokio::test]
    #[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
    async fn party_id_in_queries_use_unique_id_index() {
        require_mongo!(async {
            let fixture = TestDb::new("proc_r10_party_id_explain")
                .await
                .expect("测试数据库创建失败");
            ensure(fixture.db()).await.expect("索引创建失败");
            insert_raw_party(fixture.db(), "pty-1", "P-1").await;

            let explain = fixture
                .db()
                .run_command(doc! {
                    "explain": {
                        "find": PARTIES,
                        "filter": { "id": { "$in": ["pty-1", "pty-missing"] } },
                    },
                    "verbosity": "executionStats",
                })
                .await
                .expect("主体 id 查询 explain 失败");
            let rendered = format!("{explain:?}");
            assert!(rendered.contains("IXSCAN"), "explain 未使用 IXSCAN：{rendered}");
            assert!(
                rendered.contains("uk_parties_id"),
                "explain 未命中 uk_parties_id：{rendered}"
            );
            assert!(
                !rendered.contains("COLLSCAN"),
                "explain 出现 COLLSCAN：{rendered}"
            );
        });
    }
}
