use std::str::FromStr;

use erp_core::ids::{PayableAccountId, SupplierAccountId};
use erp_core::money::Amount;
use mongodb::bson::{Bson, doc};
use persistence_core::{NoTransaction, QueryFilter, Transactional};

use super::super::sort_doc;
use super::invoicing::invoicing_guard;
use super::settlement::settlement_guard;
use super::write::{amount_bson, progress_pipeline};
use super::{PayableAccountFilter, PayableAccountInvoicingExt, PayableAccountSettlementExt};
use crate::entity::payable::{PayableAccount, PayableAccountData, PayableAccountStatus, PayableSourceType};
use crate::repository::PayableExt;
use crate::repository::owned::PayableAccountRepository;

#[test]
fn account_filter_applies_optional_fields_and_deleted_filter() {
    let filter = PayableAccountFilter {
        source_document_id: None,
        keyword_ids: None,
        supplier_id: Some(SupplierAccountId::new("sup-1")),
        source_type: Some(PayableSourceType::PurchaseOrder),
        status: Some(PayableAccountStatus::Open),
        page: 1,
        page_size: 20,
        sort_by: None,
        sort_ascending: false,
    };

    let document = filter.to_doc();
    assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
    assert_eq!(document.get_str("supplier_id").unwrap(), "sup-1");
    assert_eq!(document.get_str("source_type").unwrap(), "purchase_order");
    assert_eq!(document.get_str("status").unwrap(), "open");
}

#[test]
fn sort_doc_maps_whitelisted_fields_and_falls_back() {
    assert_eq!(
        sort_doc(Some("open_total"), true, &["open_total", "gross_total"]),
        doc! { "open_total": 1, "id": 1 }
    );
    assert_eq!(sort_doc(Some("status"), false, &["gross_total"]), doc! { "created_at": -1, "id": -1 });
}

#[test]
fn amount_bson_keeps_decimal128_fidelity() {
    let amount = Amount::from_str("1234.56").unwrap();
    assert!(matches!(amount_bson(&amount).unwrap(), Bson::Decimal128(_)));
}

#[test]
fn apply_pipeline_guards_status_and_keeps_decimal_fidelity() {
    let amount = Amount::from_str("100.50").unwrap();
    let pipeline =
        progress_pipeline("settled_total", "open_total", &amount_bson(&amount).unwrap(), true, "admin-1");

    let set = pipeline[0].get_document("$set").unwrap();
    let add = set.get_document("settled_total").unwrap().get_array("$add").unwrap();
    assert_eq!(add[0], Bson::String("$settled_total".to_string()));
    assert!(matches!(add[1], Bson::Decimal128(_)));
    assert!(set.get_document("status").unwrap().get("$cond").is_some());
}

#[test]
fn revert_pipeline_reduces_progress_without_status_cond_misuse() {
    let amount = Amount::from_str("50.00").unwrap();
    let pipeline = progress_pipeline(
        "invoiced_total",
        "open_invoiceable_total",
        &amount_bson(&amount).unwrap(),
        false,
        "sys",
    );

    let set = pipeline[0].get_document("$set").unwrap();
    assert!(set.contains_key("invoiced_total"));
    assert!(set.contains_key("open_invoiceable_total"));
    assert!(!set.contains_key("status"), "收票进度不派生状态");
}

#[test]
fn revert_pipeline_derives_open_when_progress_reaches_zero() {
    let amount = Amount::from_str("1000.00").unwrap();
    let pipeline =
        progress_pipeline("settled_total", "open_total", &amount_bson(&amount).unwrap(), false, "sys");

    let set = pipeline[0].get_document("$set").unwrap();
    let cond = set.get_document("status").unwrap().get_array("$cond").unwrap();
    assert!(cond[0].as_document().unwrap().get_array("$eq").is_ok());
    assert_eq!(cond[1], Bson::String("settled".to_string()), "开放余额归零为已结清");
    let nested = cond[2].as_document().unwrap().get_array("$cond").unwrap();
    assert!(nested[0].as_document().unwrap().get_array("$eq").is_ok());
    assert_eq!(nested[1], Bson::String("open".to_string()), "已核销归零为未结");
    assert_eq!(nested[2], Bson::String("partially_settled".to_string()));
}

#[test]
fn settlement_guard_builds_expected_guard() {
    let amount = amount_bson(&Amount::from_str("100.50").unwrap()).unwrap();
    let guard = settlement_guard("acct-1", &amount);
    assert_eq!(guard.get_str("id").unwrap(), "acct-1");
    assert_eq!(guard.get_i64("deleted_at").unwrap(), 0);
    let expr = guard.get_document("$expr").unwrap();
    let lte = expr.get_array("$lte").unwrap();
    let add = lte[0].as_document().unwrap().get_array("$add").unwrap();
    assert_eq!(add[0], Bson::String("$settled_total".to_string()));
    assert!(matches!(add[1], Bson::Decimal128(_)));
    assert_eq!(lte[1], Bson::String("$gross_total".to_string()));
}

/// 空输入必须返回空结果且不访问数据库。
#[tokio::test]
async fn apply_settlements_many_empty_input_returns_empty_without_db() {
    let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.expect("客户端句柄创建失败");
    let database = client.database("unused");
    let repository: PayableAccountRepository<'_> =
        PayableAccountRepository::new(&database, <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS);
    let result = repository
        .apply_settlements_many(&[], "tester", &mut NoTransaction)
        .await
        .expect("空输入批量核销必须成功");
    assert!(result.applied.is_empty());
    assert!(result.rejected.is_empty());
}

/// 批量条件核销：聚合增量逐账户生效，超出开放余额的账户被拒绝且金额不变。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn batch_settlement_applies_aggregated_deltas_and_reports_rejected() {
    use crate::repository::test_fixture::{TestDb, require_mongo};

    require_mongo!(async {
        let fixture = TestDb::new("payable_settle_batch").await.expect("测试数据库创建失败");
        crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
        let accounts = fixture.db().payable_accounts();
        let account_one = PayableAccount::new(
            PayableAccountId::new("acct-1"),
            PayableAccountData {
                source_document_id: "PO-1".to_string(),
                supplier_id: SupplierAccountId::new("sup-1"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: Amount::from_str("1000.00").unwrap(),
                settled_total: Amount::from_str("0.00").unwrap(),
                invoiceable_total: Amount::from_str("1000.00").unwrap(),
                invoiced_total: Amount::from_str("0.00").unwrap(),
            },
            "tester",
        )
        .unwrap();
        let account_two = PayableAccount::new(
            PayableAccountId::new("acct-2"),
            PayableAccountData {
                source_document_id: "PO-2".to_string(),
                supplier_id: SupplierAccountId::new("sup-1"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: Amount::from_str("1000.00").unwrap(),
                settled_total: Amount::from_str("0.00").unwrap(),
                invoiceable_total: Amount::from_str("1000.00").unwrap(),
                invoiced_total: Amount::from_str("0.00").unwrap(),
            },
            "tester",
        )
        .unwrap();
        accounts.create(&account_one, &mut NoTransaction).await.expect("子账写入失败");
        accounts.create(&account_two, &mut NoTransaction).await.expect("子账写入失败");

        let deltas = [
            (PayableAccountId::new("acct-1"), Amount::from_str("400.00").unwrap()),
            (PayableAccountId::new("acct-2"), Amount::from_str("600.00").unwrap()),
        ];
        let result = accounts
            .apply_settlements_many(&deltas, "tester", &mut NoTransaction)
            .await
            .expect("批量核销失败");
        assert!(result.rejected.is_empty());
        assert_eq!(result.applied, vec![PayableAccountId::new("acct-1"), PayableAccountId::new("acct-2")]);

        let one =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(one.settled_total, Amount::from_str("400.00").unwrap());
        assert_eq!(one.open_total, Amount::from_str("600.00").unwrap());
        let two =
            accounts.find_by_id("acct-2", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(two.settled_total, Amount::from_str("600.00").unwrap());

        // 超出剩余开放余额的账户被拒绝且金额不变
        let over = [(PayableAccountId::new("acct-1"), Amount::from_str("700.00").unwrap())];
        let result =
            accounts.apply_settlements_many(&over, "tester", &mut NoTransaction).await.expect("批量核销失败");
        assert!(result.applied.is_empty());
        assert_eq!(result.rejected, vec![PayableAccountId::new("acct-1")]);
        let one =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(one.settled_total, Amount::from_str("400.00").unwrap());
    });
}

/// 任一账户被拒绝时整个事务回滚，不产生半写入。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn batch_settlement_rejected_rolls_back_whole_transaction() {
    use crate::repository::test_fixture::{TestDb, require_mongo};

    require_mongo!(async {
        let fixture = TestDb::new("payable_settle_tx").await.expect("测试数据库创建失败");
        crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
        let accounts = fixture.db().payable_accounts();
        accounts
            .create(
                &PayableAccount::new(
                    PayableAccountId::new("acct-1"),
                    PayableAccountData {
                        source_document_id: "PO-1".to_string(),
                        supplier_id: SupplierAccountId::new("sup-1"),
                        source_type: PayableSourceType::PurchaseOrder,
                        gross_total: Amount::from_str("1000.00").unwrap(),
                        settled_total: Amount::from_str("0.00").unwrap(),
                        invoiceable_total: Amount::from_str("1000.00").unwrap(),
                        invoiced_total: Amount::from_str("0.00").unwrap(),
                    },
                    "tester",
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .expect("子账写入失败");
        accounts
            .create(
                &PayableAccount::new(
                    PayableAccountId::new("acct-2"),
                    PayableAccountData {
                        source_document_id: "PO-2".to_string(),
                        supplier_id: SupplierAccountId::new("sup-1"),
                        source_type: PayableSourceType::PurchaseOrder,
                        gross_total: Amount::from_str("1000.00").unwrap(),
                        settled_total: Amount::from_str("0.00").unwrap(),
                        invoiceable_total: Amount::from_str("1000.00").unwrap(),
                        invoiced_total: Amount::from_str("0.00").unwrap(),
                    },
                    "tester",
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .expect("子账写入失败");

        let deltas = [
            (PayableAccountId::new("acct-1"), Amount::from_str("400.00").unwrap()),
            (PayableAccountId::new("acct-2"), Amount::from_str("1100.00").unwrap()),
        ];
        let db_handle = fixture.db().clone();
        let outcome = fixture
            .client()
            .with_transaction::<_, _, persistence_core::Error>(move |session| {
                Box::pin(async move {
                    let accounts: PayableAccountRepository<'_> = PayableAccountRepository::new(
                        &db_handle,
                        <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS,
                    );
                    let result = accounts.apply_settlements_many(&deltas, "tester", session).await?;
                    if !result.rejected.is_empty() {
                        return Err(persistence_core::Error::DatabaseError(mongodb::error::Error::custom(
                            "expected rejection",
                        )));
                    }
                    Ok(())
                })
            })
            .await;
        assert!(outcome.is_err(), "任一账户被拒绝必须使整个事务失败");
        let one =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(
            one.settled_total,
            Amount::from_str("0.00").unwrap(),
            "回滚后不得留下 acct-1 的半写入进度"
        );
    });
}

/// 并发批量核销：同一账户额度只允许一次命中，绝不产生超额核销。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_batch_settlement_never_exceeds_open_balance() {
    use crate::repository::test_fixture::{TestDb, require_mongo};

    require_mongo!(async {
        let fixture = TestDb::new("payable_settle_race").await.expect("测试数据库创建失败");
        crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
        let accounts = fixture.db().payable_accounts();
        accounts
            .create(
                &PayableAccount::new(
                    PayableAccountId::new("acct-1"),
                    PayableAccountData {
                        source_document_id: "PO-1".to_string(),
                        supplier_id: SupplierAccountId::new("sup-1"),
                        source_type: PayableSourceType::PurchaseOrder,
                        gross_total: Amount::from_str("1000.00").unwrap(),
                        settled_total: Amount::from_str("0.00").unwrap(),
                        invoiceable_total: Amount::from_str("1000.00").unwrap(),
                        invoiced_total: Amount::from_str("0.00").unwrap(),
                    },
                    "tester",
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .expect("子账写入失败");

        // 两个并发写入方各自尝试核销 700（总额 1400 > 开放余额 1000）
        let deltas = vec![(PayableAccountId::new("acct-1"), Amount::from_str("700.00").unwrap())];
        let db_handle_a = fixture.db().clone();
        let deltas_a = deltas.clone();
        let task_a = tokio::spawn(async move {
            let repository: PayableAccountRepository<'_> = PayableAccountRepository::new(
                &db_handle_a,
                <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS,
            );
            repository
                .apply_settlements_many(&deltas_a, "tester-a", &mut NoTransaction)
                .await
                .expect("写入方 A 失败")
        });
        let db_handle_b = fixture.db().clone();
        let task_b = tokio::spawn(async move {
            let repository: PayableAccountRepository<'_> = PayableAccountRepository::new(
                &db_handle_b,
                <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS,
            );
            repository
                .apply_settlements_many(&deltas, "tester-b", &mut NoTransaction)
                .await
                .expect("写入方 B 失败")
        });
        let result_a = task_a.await.expect("任务 A 失败");
        let result_b = task_b.await.expect("任务 B 失败");
        let applied_count = result_a.applied.len() + result_b.applied.len();
        assert_eq!(applied_count, 1, "额度只允许一方命中");
        let rejected_count = result_a.rejected.len() + result_b.rejected.len();
        assert_eq!(rejected_count, 1, "另一方必须被拒绝");

        let account =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(account.settled_total, Amount::from_str("700.00").unwrap());
        assert_eq!(account.open_total, Amount::from_str("300.00").unwrap());
        assert!(!account.open_total.to_decimal().is_sign_negative());
    });
}

#[test]
fn invoicing_guard_builds_expected_guard() {
    let amount = amount_bson(&Amount::from_str("100.50").unwrap()).unwrap();
    let guard = invoicing_guard("acct-1", &amount);
    assert_eq!(guard.get_str("id").unwrap(), "acct-1");
    assert_eq!(guard.get_i64("deleted_at").unwrap(), 0);
    let expr = guard.get_document("$expr").unwrap();
    let lte = expr.get_array("$lte").unwrap();
    let add = lte[0].as_document().unwrap().get_array("$add").unwrap();
    assert_eq!(add[0], Bson::String("$invoiced_total".to_string()));
    assert!(matches!(add[1], Bson::Decimal128(_)));
    assert_eq!(lte[1], Bson::String("$invoiceable_total".to_string()));
}

/// 空输入必须返回空结果且不访问数据库。
#[tokio::test]
async fn apply_invoicings_many_empty_input_returns_empty_without_db() {
    let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.expect("客户端句柄创建失败");
    let database = client.database("unused");
    let repository: PayableAccountRepository<'_> =
        PayableAccountRepository::new(&database, <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS);
    let result = repository
        .apply_invoicings_many(&[], "tester", &mut NoTransaction)
        .await
        .expect("空输入批量收票必须成功");
    assert!(result.applied.is_empty());
    assert!(result.rejected.is_empty());
}

/// 空输入批量红冲直接成功且不访问数据库（FIN-R11）。
#[tokio::test]
async fn revert_invoicings_many_empty_input_returns_empty_without_db() {
    let client = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1").await.expect("客户端句柄创建失败");
    let database = client.database("unused");
    let repository: PayableAccountRepository<'_> =
        PayableAccountRepository::new(&database, <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS);
    let result = repository
        .revert_invoicings_many(&[], "tester", &mut NoTransaction)
        .await
        .expect("空输入批量红冲必须成功");
    assert!(result.applied.is_empty());
    assert!(result.rejected.is_empty());
}

/// 批量条件收票：聚合增量逐账户生效，超出可收票额度的账户被拒绝且金额不变。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn batch_invoicing_applies_aggregated_deltas_and_reports_rejected() {
    use crate::repository::test_fixture::{TestDb, require_mongo};

    require_mongo!(async {
        let fixture = TestDb::new("payable_invoice_batch").await.expect("测试数据库创建失败");
        crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
        let accounts = fixture.db().payable_accounts();
        let account_one = PayableAccount::new(
            PayableAccountId::new("acct-1"),
            PayableAccountData {
                source_document_id: "PO-1".to_string(),
                supplier_id: SupplierAccountId::new("sup-1"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: Amount::from_str("1000.00").unwrap(),
                settled_total: Amount::from_str("0.00").unwrap(),
                invoiceable_total: Amount::from_str("1000.00").unwrap(),
                invoiced_total: Amount::from_str("0.00").unwrap(),
            },
            "tester",
        )
        .unwrap();
        let account_two = PayableAccount::new(
            PayableAccountId::new("acct-2"),
            PayableAccountData {
                source_document_id: "PO-2".to_string(),
                supplier_id: SupplierAccountId::new("sup-1"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: Amount::from_str("1000.00").unwrap(),
                settled_total: Amount::from_str("0.00").unwrap(),
                invoiceable_total: Amount::from_str("1000.00").unwrap(),
                invoiced_total: Amount::from_str("0.00").unwrap(),
            },
            "tester",
        )
        .unwrap();
        accounts.create(&account_one, &mut NoTransaction).await.expect("子账写入失败");
        accounts.create(&account_two, &mut NoTransaction).await.expect("子账写入失败");

        let deltas = [
            (PayableAccountId::new("acct-1"), Amount::from_str("400.00").unwrap()),
            (PayableAccountId::new("acct-2"), Amount::from_str("600.00").unwrap()),
        ];
        let result = accounts
            .apply_invoicings_many(&deltas, "tester", &mut NoTransaction)
            .await
            .expect("批量收票失败");
        assert!(result.rejected.is_empty());
        assert_eq!(result.applied, vec![PayableAccountId::new("acct-1"), PayableAccountId::new("acct-2")]);

        let one =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(one.invoiced_total, Amount::from_str("400.00").unwrap());
        assert_eq!(one.open_invoiceable_total, Amount::from_str("600.00").unwrap());
        let two =
            accounts.find_by_id("acct-2", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(two.invoiced_total, Amount::from_str("600.00").unwrap());

        // 超出剩余可收票额度的账户被拒绝且金额不变
        let over = [(PayableAccountId::new("acct-1"), Amount::from_str("700.00").unwrap())];
        let result =
            accounts.apply_invoicings_many(&over, "tester", &mut NoTransaction).await.expect("批量收票失败");
        assert!(result.applied.is_empty());
        assert_eq!(result.rejected, vec![PayableAccountId::new("acct-1")]);
        let one =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(one.invoiced_total, Amount::from_str("400.00").unwrap());
    });
}

/// 任一账户被拒绝时整个事务回滚，不产生半写入。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn batch_invoicing_rejected_rolls_back_whole_transaction() {
    use crate::repository::test_fixture::{TestDb, require_mongo};

    require_mongo!(async {
        let fixture = TestDb::new("payable_invoice_tx").await.expect("测试数据库创建失败");
        crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
        let accounts = fixture.db().payable_accounts();
        accounts
            .create(
                &PayableAccount::new(
                    PayableAccountId::new("acct-1"),
                    PayableAccountData {
                        source_document_id: "PO-1".to_string(),
                        supplier_id: SupplierAccountId::new("sup-1"),
                        source_type: PayableSourceType::PurchaseOrder,
                        gross_total: Amount::from_str("1000.00").unwrap(),
                        settled_total: Amount::from_str("0.00").unwrap(),
                        invoiceable_total: Amount::from_str("1000.00").unwrap(),
                        invoiced_total: Amount::from_str("0.00").unwrap(),
                    },
                    "tester",
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .expect("子账写入失败");
        accounts
            .create(
                &PayableAccount::new(
                    PayableAccountId::new("acct-2"),
                    PayableAccountData {
                        source_document_id: "PO-2".to_string(),
                        supplier_id: SupplierAccountId::new("sup-1"),
                        source_type: PayableSourceType::PurchaseOrder,
                        gross_total: Amount::from_str("1000.00").unwrap(),
                        settled_total: Amount::from_str("0.00").unwrap(),
                        invoiceable_total: Amount::from_str("1000.00").unwrap(),
                        invoiced_total: Amount::from_str("0.00").unwrap(),
                    },
                    "tester",
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .expect("子账写入失败");

        let deltas = [
            (PayableAccountId::new("acct-1"), Amount::from_str("400.00").unwrap()),
            (PayableAccountId::new("acct-2"), Amount::from_str("1100.00").unwrap()),
        ];
        let db_handle = fixture.db().clone();
        let outcome = fixture
            .client()
            .with_transaction::<_, _, persistence_core::Error>(move |session| {
                Box::pin(async move {
                    let accounts: PayableAccountRepository<'_> = PayableAccountRepository::new(
                        &db_handle,
                        <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS,
                    );
                    let result = accounts.apply_invoicings_many(&deltas, "tester", session).await?;
                    if !result.rejected.is_empty() {
                        return Err(persistence_core::Error::DatabaseError(mongodb::error::Error::custom(
                            "expected rejection",
                        )));
                    }
                    Ok(())
                })
            })
            .await;
        assert!(outcome.is_err(), "任一账户被拒绝必须使整个事务失败");
        let one =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(
            one.invoiced_total,
            Amount::from_str("0.00").unwrap(),
            "回滚后不得留下 acct-1 的半写入进度"
        );
    });
}

/// 并发批量收票：同一账户额度只允许一次命中，绝不产生超额收票。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_batch_invoicing_never_exceeds_invoiceable_balance() {
    use crate::repository::test_fixture::{TestDb, require_mongo};

    require_mongo!(async {
        let fixture = TestDb::new("payable_invoice_race").await.expect("测试数据库创建失败");
        crate::indexes::ensure(fixture.db()).await.expect("索引创建失败");
        let accounts = fixture.db().payable_accounts();
        accounts
            .create(
                &PayableAccount::new(
                    PayableAccountId::new("acct-1"),
                    PayableAccountData {
                        source_document_id: "PO-1".to_string(),
                        supplier_id: SupplierAccountId::new("sup-1"),
                        source_type: PayableSourceType::PurchaseOrder,
                        gross_total: Amount::from_str("1000.00").unwrap(),
                        settled_total: Amount::from_str("0.00").unwrap(),
                        invoiceable_total: Amount::from_str("1000.00").unwrap(),
                        invoiced_total: Amount::from_str("0.00").unwrap(),
                    },
                    "tester",
                )
                .unwrap(),
                &mut NoTransaction,
            )
            .await
            .expect("子账写入失败");

        // 两个并发写入方各自尝试收票 700（总额 1400 > 可收票额度 1000）
        let deltas = vec![(PayableAccountId::new("acct-1"), Amount::from_str("700.00").unwrap())];
        let db_handle_a = fixture.db().clone();
        let deltas_a = deltas.clone();
        let task_a = tokio::spawn(async move {
            let repository: PayableAccountRepository<'_> = PayableAccountRepository::new(
                &db_handle_a,
                <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS,
            );
            repository
                .apply_invoicings_many(&deltas_a, "tester-a", &mut NoTransaction)
                .await
                .expect("写入方 A 失败")
        });
        let db_handle_b = fixture.db().clone();
        let task_b = tokio::spawn(async move {
            let repository: PayableAccountRepository<'_> = PayableAccountRepository::new(
                &db_handle_b,
                <mongodb::Database as PayableExt>::PAYABLE_ACCOUNTS,
            );
            repository
                .apply_invoicings_many(&deltas, "tester-b", &mut NoTransaction)
                .await
                .expect("写入方 B 失败")
        });
        let result_a = task_a.await.expect("任务 A 失败");
        let result_b = task_b.await.expect("任务 B 失败");
        let applied_count = result_a.applied.len() + result_b.applied.len();
        assert_eq!(applied_count, 1, "额度只允许一方命中");
        let rejected_count = result_a.rejected.len() + result_b.rejected.len();
        assert_eq!(rejected_count, 1, "另一方必须被拒绝");

        let account =
            accounts.find_by_id("acct-1", &mut NoTransaction).await.expect("读取失败").expect("子账必须存在");
        assert_eq!(account.invoiced_total, Amount::from_str("700.00").unwrap());
        assert_eq!(account.open_invoiceable_total, Amount::from_str("300.00").unwrap());
        assert!(!account.open_invoiceable_total.to_decimal().is_sign_negative());
    });
}
