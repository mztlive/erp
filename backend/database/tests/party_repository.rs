//! 域 D07 `party` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test party_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::PartyExt;
use database::{ensure_indexes, Error, NoTransaction, Transactional};
use entities::common::time::BusinessDate;
use entities::ids::{PartyBankAccountId, PartyId, PartyRevisionId};
use entities::party::{
    EffectiveRecordStatus, Party, PartyBankAccount, PartyBankAccountData, PartyData, PartyKind,
    PartyRevision, PartyRevisionData, PartyStatus, PartyUpdate,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 主体列表筛选条件类型（经 `PartyExt` 关联类型跨 crate 可达）。
type PartyFilter = <Database as PartyExt>::PartyFilter;
/// 主体修订列表筛选条件类型。
type PartyRevisionFilter = <Database as PartyExt>::PartyRevisionFilter;
/// 银行账户列表筛选条件类型。
type PartyBankAccountFilter = <Database as PartyExt>::PartyBankAccountFilter;

/// 指纹密钥（与生产密钥体系同构的测试密钥，仅用于生成查询指纹）。
const FINGERPRINT_KEY: &[u8] = b"test-party-fingerprint-key";

/// 由主体编号派生 18 位字母数字统一信用代码（测试数据生成）。
fn credit_code_for(party_no: &str) -> String {
    let digits: String = party_no.chars().filter(|ch| ch.is_ascii_digit()).collect();
    format!("91310000{digits:0<10}")
}

/// 构造可复用的主体实体。
fn sample_party(party_no: &str) -> Party {
    Party::new(
        PartyId::new(format!("party-{party_no}")),
        PartyData {
            party_no: party_no.to_string(),
            party_kind: PartyKind::Enterprise,
            unified_credit_code: Some(credit_code_for(party_no)),
            status: PartyStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的主体修订实体。
fn sample_revision(party_id: &PartyId, revision_no: u32, legal_name: &str) -> PartyRevision {
    PartyRevision::new(
        PartyRevisionId::new(format!("rev-{revision_no}")),
        PartyRevisionData {
            party_id: party_id.clone(),
            revision_no,
            legal_name: legal_name.to_string(),
            short_name: Some("示例科技".to_string()),
            effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            effective_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            change_reason: "首次建档".to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的银行账户实体。
fn sample_bank_account(party_id: &PartyId, account_no: &str) -> PartyBankAccount {
    PartyBankAccount::new(
        PartyBankAccountId::new(format!("ba-{account_no}")),
        PartyBankAccountData {
            bank_account_no: account_no.to_string(),
            party_id: party_id.clone(),
            account_name: "上海示例科技有限公司".to_string(),
            bank_name: "示例银行".to_string(),
            bank_branch_name: Some("示例支行".to_string()),
            account_number: format!("6222-{account_no}"),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        FINGERPRINT_KEY,
        "admin-1",
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as PartyExt>::PARTIES,
        &[
            "uk_parties_party_no",
            "uk_parties_credit_code",
            "idx_parties_kind_status",
        ],
    )
    .await
    .expect("parties 索引缺失");
    assert_indexes(
        db,
        <Database as PartyExt>::PARTY_REVISIONS,
        &[
            "uk_party_revisions_party_revision",
            "idx_party_revisions_names",
            "idx_party_revisions_history",
        ],
    )
    .await
    .expect("party_revisions 索引缺失");
    assert_indexes(
        db,
        <Database as PartyExt>::PARTY_CONTACTS,
        &[
            "idx_party_contacts_party_status",
            "idx_party_contacts_mobile_hmac",
        ],
    )
    .await
    .expect("party_contacts 索引缺失");
    assert_indexes(
        db,
        <Database as PartyExt>::PARTY_ADDRESSES,
        &["idx_party_addresses_party_type"],
    )
    .await
    .expect("party_addresses 索引缺失");
    assert_indexes(
        db,
        <Database as PartyExt>::PARTY_TAX_PROFILES,
        &["idx_party_tax_profiles_party"],
    )
    .await
    .expect("party_tax_profiles 索引缺失");
    assert_indexes(
        db,
        <Database as PartyExt>::PARTY_BANK_ACCOUNTS,
        &[
            "uk_party_bank_accounts_bank_account_no",
            "uk_party_bank_accounts_party_hmac",
            "idx_party_bank_accounts_party_status",
        ],
    )
    .await
    .expect("party_bank_accounts 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("party_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut party = sample_party("P-2026-001");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        assert_eq!(party.base.version, 1);

        let found = db
            .parties()
            .find_by_id(&party.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.party_no, "P-2026-001");
        assert_eq!(
            found.unified_credit_code.as_deref(),
            Some(credit_code_for("P-2026-001").as_str())
        );
        assert_eq!(found.stable.created_by, "admin-1");
        assert_eq!(found.party_kind, PartyKind::Enterprise);

        party
            .update(
                PartyUpdate {
                    unified_credit_code: entities::field_update::FieldUpdate::Unchanged,
                    status: Some(PartyStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.parties().update(&mut party, &mut NoTransaction).await.unwrap();
        assert_eq!(party.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(party.stable.updated_by, "admin-2");

        db.parties()
            .soft_delete(&mut party, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .parties()
            .find_by_id(&party.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.parties()
            .restore(&mut party, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .parties()
            .find_by_id(&party.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_party_no_conflict_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("party_dup_no").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party = sample_party("P-2026-002");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();

        let duplicate = sample_party("P-2026-002");
        let error = db
            .parties()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 party_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn credit_code_unique_index_is_partial_but_rejects_duplicates() {
    require_mongo!(async {
        let test_db = TestDb::new("party_dup_code").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let first = sample_party("P-2026-003");
        db.parties().create(&first, &mut NoTransaction).await.unwrap();

        let mut duplicate = sample_party("P-2026-004");
        duplicate.unified_credit_code = first.unified_credit_code.clone();
        let error = db
            .parties()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复统一信用代码必须被部分唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let empty_a = Party::new(
            PartyId::new("party-null-a"),
            PartyData {
                party_no: "P-NULL-A".to_string(),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: None,
                status: PartyStatus::Active,
            },
            "admin-1",
        )
        .unwrap();
        let empty_b = Party::new(
            PartyId::new("party-null-b"),
            PartyData {
                party_no: "P-NULL-B".to_string(),
                party_kind: PartyKind::Enterprise,
                unified_credit_code: None,
                status: PartyStatus::Active,
            },
            "admin-1",
        )
        .unwrap();
        db.parties().create(&empty_a, &mut NoTransaction).await.unwrap();
        db.parties()
            .create(&empty_b, &mut NoTransaction)
            .await
            .expect("空信用代码不参与唯一约束（部分唯一索引）");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_optimistic_locking_error() {
    require_mongo!(async {
        let test_db = TestDb::new("party_optlock").await.unwrap();
        let db = test_db.db();

        let mut party = sample_party("P-2026-005");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();

        let mut stale = party.clone();
        party
            .update(
                PartyUpdate {
                    unified_credit_code: entities::field_update::FieldUpdate::Unchanged,
                    status: Some(PartyStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.parties().update(&mut party, &mut NoTransaction).await.unwrap();

        stale
            .update(
                PartyUpdate {
                    unified_credit_code: entities::field_update::FieldUpdate::Unchanged,
                    status: None,
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .parties()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn revision_identity_unique_and_append_only() {
    require_mongo!(async {
        let test_db = TestDb::new("party_rev_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party = sample_party("P-2026-006");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        let party_id: PartyId = party.base.id.clone().into();

        let revision = sample_revision(&party_id, 1, "上海示例科技有限公司");
        db.party_revisions()
            .create(&revision, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_revision(&party_id, 1, "重复修订");
        let error = db
            .party_revisions()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 (party_id, revision_no) 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let second = sample_revision(&party_id, 2, "上海示例科技有限公司");
        db.party_revisions()
            .create(&second, &mut NoTransaction)
            .await
            .unwrap();

        let history = db
            .party_revisions()
            .list_revision_history(&party_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(history.len(), 2, "修订为追加式历史");
        assert_eq!(history[0].revision.revision_no, 1, "历史按修订序号升序");

        let found = db
            .party_revisions()
            .find_by_party_and_revision(&party_id, 2, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按 (party_id, revision_no) 应可定位");
        assert_eq!(found.revision.revision_no, 2);
    })
}

#[tokio::test]
#[ignore]
async fn bank_account_identity_duplicate_conflicts() {
    require_mongo!(async {
        let test_db = TestDb::new("party_ba_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party = sample_party("P-2026-007");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        let party_id: PartyId = party.base.id.clone().into();

        let account = sample_bank_account(&party_id, "BA-001");
        db.party_bank_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();

        let dup_no = sample_bank_account(&party_id, "BA-001");
        let error = db
            .party_bank_accounts()
            .create(&dup_no, &mut NoTransaction)
            .await
            .expect_err("重复 bank_account_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let dup_hmac = PartyBankAccount::new(
            PartyBankAccountId::new("ba-dup-hmac"),
            PartyBankAccountData {
                bank_account_no: "BA-002".to_string(),
                party_id: party_id.clone(),
                account_name: "上海示例科技有限公司".to_string(),
                bank_name: "示例银行".to_string(),
                bank_branch_name: None,
                account_number: "6222-BA-001".to_string(),
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                is_default: false,
                status: EffectiveRecordStatus::Active,
            },
            FINGERPRINT_KEY,
            "admin-1",
        )
        .unwrap();
        let error = db
            .party_bank_accounts()
            .create(&dup_hmac, &mut NoTransaction)
            .await
            .expect_err("同主体同账号指纹必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let other_party = sample_party("P-2026-008");
        db.parties()
            .create(&other_party, &mut NoTransaction)
            .await
            .unwrap();
        let other_id: PartyId = other_party.base.id.clone().into();
        let same_hmac_other_party = PartyBankAccount::new(
            PartyBankAccountId::new("ba-other-party"),
            PartyBankAccountData {
                bank_account_no: "BA-003".to_string(),
                party_id: other_id.clone(),
                account_name: "上海示例科技有限公司".to_string(),
                bank_name: "示例银行".to_string(),
                bank_branch_name: None,
                account_number: "6222-BA-001".to_string(),
                valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                valid_to: None,
                is_default: true,
                status: EffectiveRecordStatus::Active,
            },
            FINGERPRINT_KEY,
            "admin-1",
        )
        .unwrap();
        db.party_bank_accounts()
            .create(&same_hmac_other_party, &mut NoTransaction)
            .await
            .expect("不同主体允许相同账号指纹");

        let found = db
            .party_bank_accounts()
            .find_by_account_hmac(&other_id, &account.account_number_query_hmac, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按主体+指纹应可定位");
        assert_eq!(found.bank_account_no, "BA-003");
    })
}

#[tokio::test]
#[ignore]
async fn append_revision_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("party_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party = sample_party("P-2026-009");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        let party_id: PartyId = party.base.id.clone().into();
        let revision = sample_revision(&party_id, 1, "上海示例科技有限公司");

        let db_clone = db.clone();
        let revision_for_tx = revision.clone();
        let party_in_tx = party.clone();
        let updated = test_db
            .client()
            .with_transaction::<_, Party, Error>(move |session| {
                Box::pin(async move {
                    let mut party_in_tx = party_in_tx;
                    db_clone
                        .party()
                        .append_party_revision(&mut party_in_tx, &revision_for_tx, "admin-2", session)
                        .await?;
                    Ok::<Party, Error>(party_in_tx)
                })
            })
            .await
            .expect("事务提交应成功");

        assert_eq!(
            updated.stable.current_revision_id.as_deref(),
            Some(revision.base.id.as_str())
        );
        assert_eq!(updated.base.version, 2, "主体指针更新后 version 递增");

        let revision_found = db
            .party_revisions()
            .find_by_party_and_revision(&party_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_some(), "事务提交后修订必须可见");
        let party_found = db
            .parties()
            .find_by_id(&party.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("事务提交后主体必须可见");
        assert_eq!(
            party_found.stable.current_revision_id.as_deref(),
            Some(revision.base.id.as_str()),
            "主体生效指针必须指向新修订"
        );
    })
}

#[tokio::test]
#[ignore]
async fn append_revision_rolls_back_on_stale_party_version() {
    require_mongo!(async {
        let test_db = TestDb::new("party_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut party = sample_party("P-2026-010");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        let party_id: PartyId = party.base.id.clone().into();

        let mut stale = party.clone();
        stale
            .update(
                PartyUpdate {
                    unified_credit_code: entities::field_update::FieldUpdate::Unchanged,
                    status: Some(PartyStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.parties().update(&mut party, &mut NoTransaction).await.unwrap();

        let revision = sample_revision(&party_id, 1, "上海示例科技有限公司");
        let db_clone = db.clone();
        let revision_for_tx = revision.clone();
        let result: std::result::Result<(), Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .party()
                        .append_party_revision(&mut stale, &revision_for_tx, "admin-3", session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(Error::OptimisticLockingError)),
            "陈旧版本触发 CAS 失败，事务必须回滚，实际为 {result:?}"
        );

        let revision_found = db
            .party_revisions()
            .find_by_party_and_revision(&party_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(revision_found.is_none(), "回滚后修订不得残留");
        let party_found = db
            .parties()
            .find_by_id(&party.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("回滚后主体必须保持原样");
        assert!(
            party_found.stable.current_revision_id.is_none(),
            "回滚后指针不得变更"
        );
    })
}

#[tokio::test]
#[ignore]
async fn append_revision_with_no_transaction_is_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("party_tx_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut party = sample_party("P-2026-011");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        let party_id: PartyId = party.base.id.clone().into();

        let mut stale = party.clone();
        stale
            .update(
                PartyUpdate {
                    unified_credit_code: entities::field_update::FieldUpdate::Unchanged,
                    status: Some(PartyStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.parties().update(&mut party, &mut NoTransaction).await.unwrap();

        let revision = sample_revision(&party_id, 1, "上海示例科技有限公司");
        let error = db
            .party()
            .append_party_revision(&mut stale, &revision, "admin-3", &mut NoTransaction)
            .await
            .expect_err("无事务时主体 CAS 失败必须透出错误");
        assert!(matches!(error, Error::OptimisticLockingError));

        let revision_found = db
            .party_revisions()
            .find_by_party_and_revision(&party_id, 1, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            revision_found.is_some(),
            "NoTransaction 下修订已单独提交（非原子，行为可预期）"
        );
        let party_found = db
            .parties()
            .find_by_id(&party.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("主体必须仍在");
        assert!(party_found.stable.current_revision_id.is_none(), "指针未更新");
    })
}

#[tokio::test]
#[ignore]
async fn party_list_projection_pagination_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("party_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.parties()
            .create(&sample_party("P-2026-013"), &mut NoTransaction)
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let mut disabled = sample_party("P-2026-012");
        disabled.stable.status = PartyStatus::Disabled;
        db.parties().create(&disabled, &mut NoTransaction).await.unwrap();

        let filter = PartyFilter {
            keyword: Some("2026-01".to_string()),
            party_kind: Some(PartyKind::Enterprise),
            status: Some(PartyStatus::Active),
            page: 1,
            page_size: 1,
            sort_by: Some("party_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .parties()
            .search_parties(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "启用且编号含 2026-01 只有一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.party_no, "P-2026-013");
        assert_eq!(row.party_kind, PartyKind::Enterprise);
        assert_eq!(row.status, PartyStatus::Active);
        assert_eq!(
            row.unified_credit_code.as_deref(),
            Some(credit_code_for("P-2026-013").as_str())
        );
        assert_eq!(row.current_revision_id, None);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let fallback = PartyFilter {
            keyword: None,
            party_kind: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("not_whitelisted".to_string()),
            sort_ascending: true,
        };
        let page = db
            .parties()
            .search_parties(&fallback, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "白名单外排序字段回落默认并按创建时间升序");
        assert_eq!(
            page.items[0].party_no, "P-2026-013",
            "created_at 升序首条为最早创建"
        );
    })
}

#[tokio::test]
#[ignore]
async fn revision_search_and_history_query_work() {
    require_mongo!(async {
        let test_db = TestDb::new("party_rev_search").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party = sample_party("P-2026-014");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        let party_id: PartyId = party.base.id.clone().into();
        db.party_revisions()
            .create(
                &sample_revision(&party_id, 1, "上海示例科技有限公司"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.party_revisions()
            .create(
                &sample_revision(&party_id, 2, "上海晨光商贸有限公司"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let filter = PartyRevisionFilter {
            party_id: Some(party_id.clone()),
            legal_name: Some("示例".to_string()),
            short_name: None,
            page: 1,
            page_size: 20,
            sort_by: Some("revision_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .party_revisions()
            .search_party_revisions(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "法定名称模糊匹配只命中一条");
        assert_eq!(page.items.len(), 1);
        let row = &page.items[0];
        assert_eq!(row.legal_name, "上海示例科技有限公司");
        assert_eq!(row.revision_no, 1);
        assert_eq!(row.party_id, party_id);
        assert_eq!(row.effective_from, "2026-01-01");
        assert_eq!(row.effective_to.as_deref(), Some("2026-12-31"));

        let history = db
            .party_revisions()
            .list_revision_history(&party_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].revision.revision_no, 1);
        assert_eq!(history[1].revision.revision_no, 2);
    })
}

#[tokio::test]
#[ignore]
async fn bank_account_list_projection_omits_sensitive_fields() {
    require_mongo!(async {
        let test_db = TestDb::new("party_ba_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party = sample_party("P-2026-015");
        db.parties().create(&party, &mut NoTransaction).await.unwrap();
        let party_id: PartyId = party.base.id.clone().into();
        let account = sample_bank_account(&party_id, "BA-LIST-1");
        db.party_bank_accounts()
            .create(&account, &mut NoTransaction)
            .await
            .unwrap();

        let filter = PartyBankAccountFilter {
            party_id: Some(party_id),
            status: Some(EffectiveRecordStatus::Active),
            is_default: Some(true),
            page: 1,
            page_size: 20,
            sort_by: Some("bank_account_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .party_bank_accounts()
            .search_party_bank_accounts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1);
        let row = &page.items[0];
        assert_eq!(row.bank_account_no, "BA-LIST-1");
        assert_eq!(row.account_name, "上海示例科技有限公司");
        assert!(row.is_default);
        assert_eq!(row.status, EffectiveRecordStatus::Active);
    })
}
