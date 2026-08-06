//! 域 D08 `customer` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test customer_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::CustomerExt;
use database::{ensure_indexes, Error, NoTransaction, Transactional};
use entities::common::time::BusinessDate;
use entities::customer::{
    AssignmentRole, CustomerAccount, CustomerAccountData, CustomerAccountStatus, CustomerAccountUpdate,
    CustomerAssignment, CustomerAssignmentData,
};
use entities::field_update::FieldUpdate;
use entities::ids::{CustomerAccountId, CustomerAssignmentId, PartyId};
use mongodb::Database;
use std::str::FromStr;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 客户角色列表筛选条件类型（经 `CustomerExt` 关联类型跨 crate 可达）。
type CustomerAccountFilter = <Database as CustomerExt>::CustomerAccountFilter;
/// 客户归属列表筛选条件类型。
type CustomerAssignmentFilter = <Database as CustomerExt>::CustomerAssignmentFilter;

/// 构造可复用的客户角色实体。
fn sample_customer(party_id: &PartyId, customer_no: &str) -> CustomerAccount {
    CustomerAccount::new(
        CustomerAccountId::new(format!("customer-{customer_no}")),
        CustomerAccountData {
            party_id: party_id.clone(),
            customer_no: customer_no.to_string(),
            default_payment_term_id: Some("net30".to_string()),
            status: CustomerAccountStatus::Active,
        },
        "admin-1",
    )
    .unwrap()
}

/// 构造可复用的客户归属实体。
fn sample_assignment(
    customer_id: &CustomerAccountId,
    user_id: &str,
    role: AssignmentRole,
    valid_from: &str,
) -> CustomerAssignment {
    CustomerAssignment::new(
        CustomerAssignmentId::new(format!("assign-{user_id}-{valid_from}")),
        CustomerAssignmentData {
            customer_id: customer_id.clone(),
            user_id: user_id.to_string(),
            assignment_role: role,
            valid_from: BusinessDate::from_str(valid_from).unwrap(),
            valid_to: Some(BusinessDate::from_str("2026-12-31").unwrap()),
            change_reason: "首次分配".to_string(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as CustomerExt>::CUSTOMER_ACCOUNTS,
        &[
            "uk_customer_accounts_party",
            "uk_customer_accounts_customer_no",
            "idx_customer_accounts_status",
        ],
    )
    .await
    .expect("customer_accounts 索引缺失");
    assert_indexes(
        db,
        <Database as CustomerExt>::CUSTOMER_ASSIGNMENTS,
        &[
            "uk_customer_assignments_window",
            "idx_customer_assignments_user",
            "idx_customer_assignments_customer",
        ],
    )
    .await
    .expect("customer_assignments 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_update_soft_delete_restore_roundtrip() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-cust-1");
        let mut customer = sample_customer(&party_id, "C-2026-001");
        db.customer_accounts()
            .create(&customer, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(customer.base.version, 1);

        let found = db
            .customer_accounts()
            .find_by_id(&customer.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.customer_no, "C-2026-001");
        assert_eq!(found.party_id, party_id);
        assert_eq!(found.stable.created_by, "admin-1");

        customer
            .update(
                CustomerAccountUpdate {
                    default_payment_term_id: FieldUpdate::Set("net60".to_string()),
                    status: Some(CustomerAccountStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.customer_accounts()
            .update(&mut customer, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(customer.base.version, 2, "乐观锁成功后 version 递增");

        db.customer_accounts()
            .soft_delete(&mut customer, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .customer_accounts()
            .find_by_id(&customer.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.customer_accounts()
            .restore(&mut customer, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .customer_accounts()
            .find_by_id(&customer.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_optimistic_locking_error() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_optlock").await.unwrap();
        let db = test_db.db();

        let party_id = PartyId::new("party-cust-2");
        let mut customer = sample_customer(&party_id, "C-2026-002");
        db.customer_accounts()
            .create(&customer, &mut NoTransaction)
            .await
            .unwrap();

        let mut stale = customer.clone();
        customer
            .update(
                CustomerAccountUpdate {
                    default_payment_term_id: FieldUpdate::Unchanged,
                    status: Some(CustomerAccountStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();
        db.customer_accounts()
            .update(&mut customer, &mut NoTransaction)
            .await
            .unwrap();

        stale
            .update(
                CustomerAccountUpdate {
                    default_payment_term_id: FieldUpdate::Unchanged,
                    status: None,
                },
                "admin-3",
            )
            .unwrap();
        let error = db
            .customer_accounts()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn unique_customer_no_and_party_conflicts_surface_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-cust-3");
        let customer = sample_customer(&party_id, "C-2026-003");
        db.customer_accounts()
            .create(&customer, &mut NoTransaction)
            .await
            .unwrap();

        let dup_no = sample_customer(&PartyId::new("party-cust-4"), "C-2026-003");
        let error = db
            .customer_accounts()
            .create(&dup_no, &mut NoTransaction)
            .await
            .expect_err("重复 customer_no 必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let dup_party = sample_customer(&party_id, "C-2026-004");
        let error = db
            .customer_accounts()
            .create(&dup_party, &mut NoTransaction)
            .await
            .expect_err("同一 party 第二个客户角色必须被唯一索引拒绝");
        assert!(
            matches!(error, Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn replace_assignments_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_assign_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-cust-5");
        let customer = sample_customer(&party_id, "C-2026-005");
        db.customer_accounts()
            .create(&customer, &mut NoTransaction)
            .await
            .unwrap();
        let customer_id: CustomerAccountId = customer.base.id.clone().into();

        let old_assignments = vec![sample_assignment(
            &customer_id,
            "sales-1",
            AssignmentRole::Owner,
            "2026-01-01",
        )];
        db.customer()
            .replace_customer_assignments(&customer_id, &old_assignments, &mut NoTransaction)
            .await
            .unwrap();

        let new_assignments = vec![
            sample_assignment(&customer_id, "sales-2", AssignmentRole::Owner, "2026-02-01"),
            sample_assignment(
                &customer_id,
                "sales-1",
                AssignmentRole::Collaborator,
                "2026-02-01",
            ),
        ];
        let db_clone = db.clone();
        let customer_id_for_tx = customer_id.clone();
        test_db
            .client()
            .with_transaction::<_, (), Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .customer()
                        .replace_customer_assignments(&customer_id_for_tx, &new_assignments, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let filter = CustomerAssignmentFilter {
            customer_id: Some(customer_id),
            user_id: None,
            assignment_role: None,
            page: 1,
            page_size: 20,
            sort_by: Some("valid_from".to_string()),
            sort_ascending: true,
        };
        let page = db
            .customer_assignments()
            .search_customer_assignments(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "替换后只剩新归属");
        let users: Vec<&str> = page.items.iter().map(|row| row.user_id.as_str()).collect();
        assert!(users.contains(&"sales-2"));
        assert!(users.contains(&"sales-1"));
    })
}

#[tokio::test]
#[ignore]
async fn replace_assignments_rolls_back_on_duplicate_window_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_assign_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-cust-6");
        let customer = sample_customer(&party_id, "C-2026-006");
        db.customer_accounts()
            .create(&customer, &mut NoTransaction)
            .await
            .unwrap();
        let customer_id: CustomerAccountId = customer.base.id.clone().into();

        let existing = vec![sample_assignment(
            &customer_id,
            "sales-1",
            AssignmentRole::Owner,
            "2026-01-01",
        )];
        db.customer()
            .replace_customer_assignments(&customer_id, &existing, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_window = vec![
            sample_assignment(&customer_id, "sales-2", AssignmentRole::Owner, "2026-02-01"),
            sample_assignment(&customer_id, "sales-2", AssignmentRole::Owner, "2026-02-01"),
        ];
        let db_clone = db.clone();
        let customer_id_for_tx = customer_id.clone();
        let result: std::result::Result<(), Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .customer()
                        .replace_customer_assignments(&customer_id_for_tx, &duplicate_window, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(Error::DuplicateKey(_))),
            "重复归属窗口触发唯一索引冲突，事务必须回滚，实际为 {result:?}"
        );

        let found = db
            .customer_assignments()
            .find_by_id(&existing[0].base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(found.is_some(), "回滚后旧归属必须保留");
    })
}

#[tokio::test]
#[ignore]
async fn replace_assignments_with_no_transaction_is_predictable() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_assign_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-cust-7");
        let customer = sample_customer(&party_id, "C-2026-007");
        db.customer_accounts()
            .create(&customer, &mut NoTransaction)
            .await
            .unwrap();
        let customer_id: CustomerAccountId = customer.base.id.clone().into();

        let existing = vec![sample_assignment(
            &customer_id,
            "sales-1",
            AssignmentRole::Owner,
            "2026-01-01",
        )];
        db.customer()
            .replace_customer_assignments(&customer_id, &existing, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate_window = vec![
            sample_assignment(&customer_id, "sales-2", AssignmentRole::Owner, "2026-02-01"),
            sample_assignment(&customer_id, "sales-2", AssignmentRole::Owner, "2026-02-01"),
        ];
        let error = db
            .customer()
            .replace_customer_assignments(&customer_id, &duplicate_window, &mut NoTransaction)
            .await
            .expect_err("NoTransaction 下写入冲突必须透出错误");
        assert!(matches!(error, Error::DuplicateKey(_)));

        let found = db
            .customer_assignments()
            .find_by_id(&existing[0].base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            found.is_none(),
            "NoTransaction 下删除已提交、批量写入失败（非原子，行为可预期）"
        );
    })
}

#[tokio::test]
#[ignore]
async fn customer_list_projection_pagination_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.customer_accounts()
            .create(
                &sample_customer(&PartyId::new("party-cust-9"), "C-2026-009"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        let mut disabled = sample_customer(&PartyId::new("party-cust-8"), "C-2026-008");
        disabled.stable.status = CustomerAccountStatus::Disabled;
        db.customer_accounts()
            .create(&disabled, &mut NoTransaction)
            .await
            .unwrap();

        let filter = CustomerAccountFilter {
            keyword: Some("2026-00".to_string()),
            party_id: None,
            status: Some(CustomerAccountStatus::Active),
            page: 1,
            page_size: 1,
            sort_by: Some("customer_no".to_string()),
            sort_ascending: true,
        };
        let page = db
            .customer_accounts()
            .search_customer_accounts(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 1, "启用且编号含 2026-00 只有一条");
        let row = &page.items[0];
        assert_eq!(row.customer_no, "C-2026-009");
        assert_eq!(row.status, CustomerAccountStatus::Active);
        assert_eq!(row.default_payment_term_id.as_deref(), Some("net30"));
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let fallback = CustomerAccountFilter {
            keyword: None,
            party_id: None,
            status: None,
            page: 1,
            page_size: 20,
            sort_by: Some("not_whitelisted".to_string()),
            sort_ascending: false,
        };
        let page = db
            .customer_accounts()
            .search_customer_accounts(&fallback, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "白名单外排序字段回落默认 created_at 降序");
        assert_eq!(
            page.items[0].customer_no, "C-2026-008",
            "created_at 降序首条为最新创建"
        );
    })
}

#[tokio::test]
#[ignore]
async fn assignment_user_query_finds_active_assignments() {
    require_mongo!(async {
        let test_db = TestDb::new("cust_assign_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let party_id = PartyId::new("party-cust-10");
        let customer = sample_customer(&party_id, "C-2026-010");
        db.customer_accounts()
            .create(&customer, &mut NoTransaction)
            .await
            .unwrap();
        let customer_id: CustomerAccountId = customer.base.id.clone().into();

        let assignments = vec![
            sample_assignment(&customer_id, "sales-1", AssignmentRole::Owner, "2026-01-01"),
            sample_assignment(
                &customer_id,
                "sales-1",
                AssignmentRole::Collaborator,
                "2026-03-01",
            ),
        ];
        db.customer()
            .replace_customer_assignments(&customer_id, &assignments, &mut NoTransaction)
            .await
            .unwrap();

        let active = db
            .customer_assignments()
            .find_active_assignments_for_user(
                "sales-1",
                BusinessDate::from_str("2026-04-01").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(active.len(), 2, "两个角色都在 2026-12-31 前生效");

        let none = db
            .customer_assignments()
            .find_active_assignments_for_user(
                "sales-1",
                BusinessDate::from_str("2027-01-01").unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert!(none.is_empty(), "超过有效期末尾后不再命中");
    })
}
