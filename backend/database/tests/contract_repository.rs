//! 域 D12 `contract` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test contract_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//! `contract_revision` 是不可变修订（事实类），不提供软删除方法。

use database::repository::extensions::ContractExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::BusinessDate;
use entities::contract::{
    ArchiveSource, Contract, ContractData, ContractRevision, ContractRevisionData, ContractStatus,
};
use entities::ids::{ContractId, ContractRevisionId, CustomerAccountId, FileAssetId, PartyId};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 合同列表筛选条件类型（经 `ContractExt` 关联类型跨 crate 可达）。
type ContractFilter = <Database as ContractExt>::ContractFilter;

/// 构造可复用的合同实体。
fn sample_contract(contract_no: &str) -> Contract {
    Contract::new(
        ContractId::new(format!("contract-{contract_no}")),
        ContractData {
            contract_no: contract_no.to_string(),
            customer_id: CustomerAccountId::new("cust-1"),
            settlement_party_id: PartyId::new("party-1"),
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的合同版本实体。
fn sample_revision(contract: &Contract, revision_no: u32) -> ContractRevision {
    ContractRevision::new(
        ContractRevisionId::new(format!("rev-{}-{revision_no}", contract.base.id)),
        contract.base.id.clone().into(),
        revision_no,
        ContractRevisionData {
            contract_no: contract.contract_no.clone(),
            customer_name: "东方企业".to_string(),
            contract_pdf_file_id: FileAssetId::new("file-1"),
            archive_source: ArchiveSource::ContractCenter,
            settlement_party_id: PartyId::new("party-1"),
            settlement_party_name: "集团结算中心".to_string(),
            payment_term_code: "NET30".to_string(),
            payment_term_name: "月结 30 天".to_string(),
            invoice_type: "增值税专用发票".to_string(),
            tax_point: "6".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: Some(BusinessDate::from_ymd(2026, 12, 31).unwrap()),
            signed_at: BusinessDate::from_ymd(2025, 12, 20).unwrap(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as ContractExt>::CONTRACTS,
        &["uk_contracts_contract_no", "idx_contracts_customer_status"],
    )
    .await
    .expect("contracts 索引缺失");
    assert_indexes(
        db,
        <Database as ContractExt>::CONTRACT_REVISIONS,
        &[
            "uk_contract_revisions_contract_revision",
            "idx_contract_revisions_validity",
        ],
    )
    .await
    .expect("contract_revisions 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_contract_with_revision_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("contract_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut contract = sample_contract("HT-2026-0088");
        let revision = sample_revision(&contract, 1);
        let db_clone = db.clone();
        let mut contract_for_tx = contract.clone();
        let revision_for_tx = revision.clone();
        contract = test_db
            .client()
            .with_transaction::<_, Contract, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision_for_tx, session)
                        .await?;
                    Ok(contract_for_tx)
                })
            })
            .await
            .expect("事务提交应成功");
        assert_eq!(contract.base.version, 2, "创建加绑定指针共两次写入，版本递增");
        assert_eq!(
            contract.stable.current_revision_id.as_deref(),
            Some(revision.base.id.as_str()),
            "当前版本指针指向首个版本"
        );

        let found = db
            .contracts()
            .find_by_contract_no("HT-2026-0088", &mut NoTransaction)
            .await
            .unwrap()
            .expect("按编号应可读回");
        assert_eq!(found.customer_id, CustomerAccountId::new("cust-1"));

        let rev = db
            .contract_revisions()
            .find_by_contract_and_no(&contract.base.id.clone().into(), 1, &mut NoTransaction)
            .await
            .unwrap()
            .expect("按 (合同, 版本号) 应可读回");
        assert_eq!(rev.contract_no, "HT-2026-0088");
        assert_eq!(rev.revision.revision_no, 1);
        assert_eq!(
            rev.valid_from,
            BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            "业务日期往返一致"
        );
        assert_eq!(rev.customer_snapshot.customer_name, "东方企业");
        assert_eq!(rev.contract_pdf_file_id, FileAssetId::new("file-1"));
    })
}

#[tokio::test]
#[ignore]
async fn archive_revision_attaches_new_pointer_and_optimistic_lock_guards_updates() {
    require_mongo!(async {
        let test_db = TestDb::new("contract_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut contract = sample_contract("HT-2026-0099");
        let revision = sample_revision(&contract, 1);
        let db_clone = db.clone();
        let mut contract_for_tx = contract.clone();
        let revision_for_tx = revision.clone();
        contract = test_db
            .client()
            .with_transaction::<_, Contract, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision_for_tx, session)
                        .await?;
                    Ok(contract_for_tx)
                })
            })
            .await
            .expect("事务提交应成功");

        contract
            .update(
                entities::contract::ContractUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-2")),
                    settlement_party_id: None,
                },
                "admin-2",
            )
            .unwrap();
        db.contracts()
            .update(&mut contract, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(contract.base.version, 3, "乐观锁成功后 version 递增");

        let mut stale = contract.clone();
        let mut live = contract.clone();
        live.update(
            entities::contract::ContractUpdate {
                customer_id: Some(CustomerAccountId::new("cust-3")),
                settlement_party_id: None,
            },
            "admin-3",
        )
        .unwrap();
        db.contracts()
            .update(&mut live, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                entities::contract::ContractUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-4")),
                    settlement_party_id: None,
                },
                "admin-4",
            )
            .unwrap();
        let error = db
            .contracts()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn contract_soft_delete_and_restore_keep_identity() {
    require_mongo!(async {
        let test_db = TestDb::new("contract_softdel").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut contract = sample_contract("HT-2026-0101");
        let revision = sample_revision(&contract, 1);
        let db_clone = db.clone();
        let mut contract_for_tx = contract.clone();
        let revision_for_tx = revision.clone();
        contract = test_db
            .client()
            .with_transaction::<_, Contract, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision_for_tx, session)
                        .await?;
                    Ok(contract_for_tx)
                })
            })
            .await
            .expect("事务提交应成功");

        db.contracts()
            .soft_delete(&mut contract, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .contracts()
            .find_by_contract_no("HT-2026-0101", &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按编号不可见");

        db.contracts()
            .restore(&mut contract, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .contracts()
            .find_by_contract_no("HT-2026-0101", &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按编号重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn unique_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("contract_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let contract = sample_contract("HT-2026-0201");
        let revision = sample_revision(&contract, 1);
        let db_clone = db.clone();
        let mut contract_for_tx = contract.clone();
        let revision_for_tx = revision.clone();
        let created = test_db
            .client()
            .with_transaction::<_, Contract, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision_for_tx, session)
                        .await?;
                    Ok(contract_for_tx)
                })
            })
            .await
            .expect("事务提交应成功");

        let duplicate = sample_contract("HT-2026-0201");
        let error = db
            .contracts()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("重复 contract_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let duplicate_revision = sample_revision(&created, 1);
        let error = db
            .contract_revisions()
            .create(&duplicate_revision, &mut NoTransaction)
            .await
            .expect_err("重复 (contract_id, revision_no) 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_contract_and_revision() {
    require_mongo!(async {
        let test_db = TestDb::new("contract_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let contract = sample_contract("HT-2026-0301");
        let revision = sample_revision(&contract, 1);
        let db_clone = db.clone();
        let mut contract_for_tx = contract.clone();
        let revision_for_tx = revision.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let found = db
            .contracts()
            .find_by_contract_no("HT-2026-0301", &mut NoTransaction)
            .await
            .unwrap();
        assert!(found.is_none(), "回滚后合同不得残留");
        let rev = db
            .contract_revisions()
            .find_by_id(&revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(rev.is_none(), "回滚后合同版本不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn archive_revision_conflict_rolls_back_whole_batch() {
    require_mongo!(async {
        let test_db = TestDb::new("contract_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut contract = sample_contract("HT-2026-0401");
        let revision = sample_revision(&contract, 1);
        let db_clone = db.clone();
        let mut contract_for_tx = contract.clone();
        let revision_for_tx = revision.clone();
        contract = test_db
            .client()
            .with_transaction::<_, Contract, database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .contract()
                        .create_contract_with_revision(&mut contract_for_tx, &revision_for_tx, session)
                        .await?;
                    Ok(contract_for_tx)
                })
            })
            .await
            .expect("事务提交应成功");

        let second_revision = sample_revision(&contract, 2);
        let stale = contract.clone();
        contract
            .update(
                entities::contract::ContractUpdate {
                    customer_id: Some(CustomerAccountId::new("cust-9")),
                    settlement_party_id: None,
                },
                "admin-9",
            )
            .unwrap();
        db.contracts()
            .update(&mut contract, &mut NoTransaction)
            .await
            .unwrap();

        let db_clone = db.clone();
        let mut stale_for_tx = stale.clone();
        let second_for_tx = second_revision.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .contract()
                        .archive_contract_revision(&mut stale_for_tx, &second_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(result.is_err(), "陈旧版本归档必须被 CAS 拒绝");

        let rev = db
            .contract_revisions()
            .find_by_id(&second_revision.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(rev.is_none(), "冲突回滚后新版本不得残留");
        let found = db
            .contracts()
            .find_by_id(&contract.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("主表不受影响");
        assert_eq!(
            found.stable.current_revision_id.as_deref(),
            Some(revision.base.id.as_str())
        );
    })
}

#[tokio::test]
#[ignore]
async fn projection_list_search_respects_filters_and_pagination() {
    require_mongo!(async {
        let test_db = TestDb::new("contract_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        for (i, no) in ["HT-2026-0501", "HT-2026-0502", "HT-2026-0503"]
            .iter()
            .enumerate()
        {
            let mut contract = sample_contract(no);
            contract.base.created_at = 1_700_000_000 + i as u64;
            let revision = sample_revision(&contract, 1);
            let db_clone = db.clone();
            let mut contract_for_tx = contract.clone();
            let revision_for_tx = revision.clone();
            contract = test_db
                .client()
                .with_transaction::<_, Contract, database::Error>(move |session| {
                    Box::pin(async move {
                        db_clone
                            .contract()
                            .create_contract_with_revision(&mut contract_for_tx, &revision_for_tx, session)
                            .await?;
                        Ok(contract_for_tx)
                    })
                })
                .await
                .expect("事务提交应成功");
            if *no == "HT-2026-0502" {
                let mut terminated = contract.clone();
                terminated.terminate("admin-9").unwrap();
                db.contracts()
                    .update(&mut terminated, &mut NoTransaction)
                    .await
                    .unwrap();
            }
        }

        let filter = ContractFilter {
            contract_no: Some("HT-2026".to_string()),
            customer_id: None,
            status: Some(ContractStatus::Effective),
            page: 1,
            page_size: 2,
            sort_by: Some("contract_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .contracts()
            .search_contracts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "生效且匹配编号前缀只有两条");
        assert_eq!(page.items.len(), 2);
        assert_eq!(page.items[0].contract_no, "HT-2026-0501", "按编号升序");
        assert_eq!(page.items[0].customer_id, "cust-1");
        assert_eq!(page.items[0].status, ContractStatus::Effective);
        assert!(page.items[0].version >= 1);
        assert!(page.items[0].created_at > 0);

        let second = ContractFilter {
            contract_no: None,
            customer_id: None,
            status: None,
            page: 2,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let tail = db
            .contracts()
            .search_contracts(&second, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(tail.total, 3, "不含状态筛选时命中三条");
        assert_eq!(tail.items.len(), 1, "分页边界：第二页一条");
        assert_eq!(
            tail.items[0].contract_no, "HT-2026-0502",
            "按创建时间倒序取第二页"
        );
    })
}
