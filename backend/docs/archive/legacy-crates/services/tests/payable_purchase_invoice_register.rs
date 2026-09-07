//! 进项发票登记（FIN-E03 / FIN-R04）真实 MongoDB 验收。
//!
//! `register_purchase_invoice` 的金额口径/序号/实体构造已归位
//! `PurchaseInvoiceAllocationPlan`，账户与供应商事实按去重集合批量装载，
//! 收票进度按账户聚合后批量条件更新，分配行批量插入。本测试以真实 Mongo
//! 驱动完整 Service 路径，验证：同账户多行金额守恒且进度只按聚合值推进；
//! 跨供应商、发票总额不符、额度不足、重复号码与并发唯一冲突时全事务回滚、
//! 零残留写入且错误稳定。

use std::str::FromStr;

use database::{ensure_indexes, NoTransaction, PayableExt, ReceivableExt, SupplierExt};
use entities::common::time::BusinessDate;
use entities::ids::{InvoiceId, PartyId, PayableAccountId, SupplierAccountId};
use entities::money::Amount;
use entities::payable::{PayableAccount, PayableAccountData, PayableSourceType, PurchaseInvoiceAllocation};
use entities::supplier::{SupplierAccount, SupplierAccountData, SupplierAccountStatus};
use services::audit::AuditActor;
use services::payable::{
    PayableService, PurchaseInvoiceAllocationLineRequest, RegisterPurchaseInvoiceRequest,
};
use test_support::{require_mongo, TestDb};

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn actor() -> AuditActor {
    AuditActor::new(
        "finance-1".to_string(),
        "finance-1".to_string(),
        entities::AccountKind::Admin,
    )
}

/// 建立供应商与两个应付子账：acct-1 可收票 1000，acct-2 可收票 500。
async fn seed(db: &mongodb::Database) -> (SupplierAccountId, PayableAccountId, PayableAccountId) {
    let supplier = SupplierAccount::new(
        SupplierAccountId::new("sup-1"),
        SupplierAccountData {
            party_id: PartyId::new("party-1"),
            supplier_no: "SUP-001".to_string(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        },
        "tester",
    )
    .expect("供应商构造失败");
    db.supplier_accounts()
        .create(&supplier, &mut NoTransaction)
        .await
        .expect("供应商写入失败");

    let account_one = PayableAccount::new(
        PayableAccountId::new("acct-1"),
        PayableAccountData {
            source_document_id: "PO-1".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: amount("1000.00"),
            settled_total: amount("0.00"),
            invoiceable_total: amount("1000.00"),
            invoiced_total: amount("0.00"),
        },
        "tester",
    )
    .expect("子账构造失败");
    let account_two = PayableAccount::new(
        PayableAccountId::new("acct-2"),
        PayableAccountData {
            source_document_id: "PO-2".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: amount("500.00"),
            settled_total: amount("0.00"),
            invoiceable_total: amount("500.00"),
            invoiced_total: amount("0.00"),
        },
        "tester",
    )
    .expect("子账构造失败");
    db.payable_accounts()
        .create(&account_one, &mut NoTransaction)
        .await
        .expect("子账一写入失败");
    db.payable_accounts()
        .create(&account_two, &mut NoTransaction)
        .await
        .expect("子账二写入失败");
    (
        SupplierAccountId::new("sup-1"),
        PayableAccountId::new("acct-1"),
        PayableAccountId::new("acct-2"),
    )
}

fn line(
    account_id: PayableAccountId,
    gross: &str,
    net: &str,
    tax: &str,
) -> PurchaseInvoiceAllocationLineRequest {
    PurchaseInvoiceAllocationLineRequest {
        payable_account_id: account_id,
        allocated_gross_amount: amount(gross),
        allocated_net_amount: amount(net),
        allocated_tax_amount: amount(tax),
    }
}

/// 成功登记：同账户多行金额守恒、进度只按聚合值推进、序号从 1 连续。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn register_applies_aggregated_progress_and_conserves_amounts() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_register_success")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let (supplier_id, account_one, account_two) = seed(fixture.db()).await;
        let service = PayableService::new(fixture.db().clone());

        let view = service
            .register_purchase_invoice(
                RegisterPurchaseInvoiceRequest {
                    idempotency_key: "register-key-1".to_string(),
                    invoice_code: None,
                    invoice_no: "INV-1001".to_string(),
                    invoice_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
                    gross_amount: amount("1000.00"),
                    net_amount: amount("940.00"),
                    tax_amount: amount("60.00"),
                    supplier_id: supplier_id.clone(),
                    allocations: vec![
                        line(account_one.clone(), "400.00", "376.00", "24.00"),
                        line(account_one.clone(), "300.00", "282.00", "18.00"),
                        line(account_two.clone(), "300.00", "282.00", "18.00"),
                    ],
                },
                &actor(),
            )
            .await
            .expect("登记必须成功");
        assert_eq!(view.allocations.len(), 3);
        assert_eq!(
            view.allocations
                .iter()
                .map(|row| row.allocation_seq)
                .collect::<Vec<_>>(),
            vec![1, 2, 3],
            "序号必须按输入顺序从 1 连续"
        );

        // 同账户多行只按聚合值推进一次
        let one = fixture
            .db()
            .payable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(
            one.invoiced_total,
            amount("700.00"),
            "acct-1 进度必须为聚合值 700"
        );
        assert_eq!(one.open_invoiceable_total, amount("300.00"));
        let two = fixture
            .db()
            .payable_accounts()
            .find_by_id("acct-2", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(two.invoiced_total, amount("300.00"));

        // 发票与分配行落库，金额与视图守恒
        let invoice = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(
                entities::receivable::InvoiceDirection::Purchase,
                "INV-1001",
                &mut NoTransaction,
            )
            .await
            .expect("发票查询失败")
            .expect("发票必须存在");
        assert_eq!(invoice.gross_amount, amount("1000.00"));
        let allocations: Vec<PurchaseInvoiceAllocation> = fixture
            .db()
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new(invoice.base.id.clone())], &mut NoTransaction)
            .await
            .expect("分配查询失败");
        assert_eq!(allocations.len(), 3);
        let gross: Amount = allocations.iter().fold(amount("0.00"), |sum, line| {
            sum.checked_add(line.allocated_gross_amount)
        });
        assert_eq!(gross, amount("1000.00"), "分配总额必须与发票应付额精确守恒");
    });
}

/// 重复发票号码：冲突错误稳定，进度不重复推进、分配不重复写入。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn duplicate_invoice_no_conflicts_without_double_advance() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_register_duplicate")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let (supplier_id, account_one, _) = seed(fixture.db()).await;
        let service = PayableService::new(fixture.db().clone());
        let req = RegisterPurchaseInvoiceRequest {
            idempotency_key: "register-key-2".to_string(),
            invoice_code: None,
            invoice_no: "INV-1002".to_string(),
            invoice_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
            gross_amount: amount("1000.00"),
            net_amount: amount("940.00"),
            tax_amount: amount("60.00"),
            supplier_id: supplier_id.clone(),
            allocations: vec![line(account_one.clone(), "1000.00", "940.00", "60.00")],
        };
        service
            .register_purchase_invoice(req.clone(), &actor())
            .await
            .expect("首次登记必须成功");
        let err = service
            .register_purchase_invoice(
                RegisterPurchaseInvoiceRequest {
                    idempotency_key: "register-key-2b".to_string(),
                    ..req
                },
                &actor(),
            )
            .await
            .expect_err("重复号码必须冲突");
        assert!(
            err.to_string().contains("发票号码已登记，请勿重复提交"),
            "错误必须稳定，实际：{err}"
        );

        let one = fixture
            .db()
            .payable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(one.invoiced_total, amount("1000.00"), "重复提交不得重复推进进度");
        let invoice = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(
                entities::receivable::InvoiceDirection::Purchase,
                "INV-1002",
                &mut NoTransaction,
            )
            .await
            .expect("发票查询失败")
            .expect("发票必须存在");
        let allocations = fixture
            .db()
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new(invoice.base.id)], &mut NoTransaction)
            .await
            .expect("分配查询失败");
        assert_eq!(allocations.len(), 1, "重复提交不得重复写入分配");
    });
}

/// 跨供应商收票：全事务回滚，零残留写入。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn cross_supplier_registration_rejected_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_register_cross_supplier")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let (supplier_id, account_one, _) = seed(fixture.db()).await;

        // 第二个供应商（不同 party）及其子账
        let other_supplier = SupplierAccount::new(
            SupplierAccountId::new("sup-2"),
            SupplierAccountData {
                party_id: PartyId::new("party-2"),
                supplier_no: "SUP-002".to_string(),
                default_payment_term_id: None,
                current_commercial_profile_revision_id: None,
                status: SupplierAccountStatus::Active,
            },
            "tester",
        )
        .expect("供应商构造失败");
        fixture
            .db()
            .supplier_accounts()
            .create(&other_supplier, &mut NoTransaction)
            .await
            .expect("供应商写入失败");
        let other_account = PayableAccount::new(
            PayableAccountId::new("acct-9"),
            PayableAccountData {
                source_document_id: "PO-9".to_string(),
                supplier_id: SupplierAccountId::new("sup-2"),
                source_type: PayableSourceType::PurchaseOrder,
                gross_total: amount("1000.00"),
                settled_total: amount("0.00"),
                invoiceable_total: amount("1000.00"),
                invoiced_total: amount("0.00"),
            },
            "tester",
        )
        .expect("子账构造失败");
        fixture
            .db()
            .payable_accounts()
            .create(&other_account, &mut NoTransaction)
            .await
            .expect("子账写入失败");

        let service = PayableService::new(fixture.db().clone());
        let err = service
            .register_purchase_invoice(
                RegisterPurchaseInvoiceRequest {
                    idempotency_key: "register-key-3".to_string(),
                    invoice_code: None,
                    invoice_no: "INV-1003".to_string(),
                    invoice_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
                    gross_amount: amount("1000.00"),
                    net_amount: amount("940.00"),
                    tax_amount: amount("60.00"),
                    supplier_id: supplier_id.clone(),
                    allocations: vec![
                        line(account_one.clone(), "600.00", "564.00", "36.00"),
                        line(PayableAccountId::new("acct-9"), "400.00", "376.00", "24.00"),
                    ],
                },
                &actor(),
            )
            .await
            .expect_err("跨供应商收票必须拒绝");
        assert!(
            err.to_string().contains("禁止跨供应商收票"),
            "错误必须稳定，实际：{err}"
        );

        // 零残留：无发票、无分配、进度未推进
        let invoice = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(
                entities::receivable::InvoiceDirection::Purchase,
                "INV-1003",
                &mut NoTransaction,
            )
            .await
            .expect("发票查询失败");
        assert!(invoice.is_none(), "回滚后不得留下发票");
        let allocations = fixture
            .db()
            .purchase_invoice_allocations()
            .find_allocations_by_accounts(
                &[account_one, PayableAccountId::new("acct-9")],
                &mut NoTransaction,
            )
            .await
            .expect("分配查询失败");
        assert!(allocations.is_empty(), "回滚后不得留下分配");
        for id in ["acct-1", "acct-9"] {
            let account = fixture
                .db()
                .payable_accounts()
                .find_by_id(id, &mut NoTransaction)
                .await
                .expect("读取失败")
                .expect("子账必须存在");
            assert_eq!(account.invoiced_total, amount("0.00"), "回滚后不得推进收票进度");
        }
    });
}

/// 发票总额不符：计划整体拒绝，零写入。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn invoice_total_mismatch_rolled_back_without_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_register_total_mismatch")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let (supplier_id, account_one, _) = seed(fixture.db()).await;
        let service = PayableService::new(fixture.db().clone());

        // 少分：分配 900 vs 发票 1000
        let err = service
            .register_purchase_invoice(
                RegisterPurchaseInvoiceRequest {
                    idempotency_key: "register-key-4".to_string(),
                    invoice_code: None,
                    invoice_no: "INV-1004".to_string(),
                    invoice_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
                    gross_amount: amount("1000.00"),
                    net_amount: amount("940.00"),
                    tax_amount: amount("60.00"),
                    supplier_id: supplier_id.clone(),
                    allocations: vec![line(account_one.clone(), "900.00", "846.00", "54.00")],
                },
                &actor(),
            )
            .await
            .expect_err("少分必须拒绝");
        assert!(
            err.to_string().contains("发票分配合计必须等于发票金额"),
            "错误必须稳定，实际：{err}"
        );

        let invoice = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(
                entities::receivable::InvoiceDirection::Purchase,
                "INV-1004",
                &mut NoTransaction,
            )
            .await
            .expect("发票查询失败");
        assert!(invoice.is_none());
        let one = fixture
            .db()
            .payable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(one.invoiced_total, amount("0.00"));
    });
}

/// 额度不足：批量条件收票拒绝，全事务回滚且错误稳定。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn insufficient_quota_rolled_back_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_register_quota")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let (supplier_id, account_one, account_two) = seed(fixture.db()).await;
        let service = PayableService::new(fixture.db().clone());

        // acct-2 可收票仅 500，分配 600 超出额度
        let err = service
            .register_purchase_invoice(
                RegisterPurchaseInvoiceRequest {
                    idempotency_key: "register-key-5".to_string(),
                    invoice_code: None,
                    invoice_no: "INV-1005".to_string(),
                    invoice_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
                    gross_amount: amount("1100.00"),
                    net_amount: amount("1034.00"),
                    tax_amount: amount("66.00"),
                    supplier_id: supplier_id.clone(),
                    allocations: vec![
                        line(account_one.clone(), "500.00", "470.00", "30.00"),
                        line(account_two.clone(), "600.00", "564.00", "36.00"),
                    ],
                },
                &actor(),
            )
            .await
            .expect_err("超额收票必须拒绝");
        assert!(
            err.to_string().contains("子账剩余可收票额度不足，收票被拒绝"),
            "错误必须稳定，实际：{err}"
        );

        // 零残留：acct-1 的 500 也必须回滚
        let one = fixture
            .db()
            .payable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(one.invoiced_total, amount("0.00"), "acct-1 不得留下半写入进度");
        let invoice = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(
                entities::receivable::InvoiceDirection::Purchase,
                "INV-1005",
                &mut NoTransaction,
            )
            .await
            .expect("发票查询失败");
        assert!(invoice.is_none(), "回滚后不得留下发票");
    });
}

/// 并发同号码登记：唯一冲突只允许一方成功，败者零残留。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn concurrent_registration_leaves_no_half_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_register_race")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let (supplier_id, account_one, _) = seed(fixture.db()).await;

        let db_a = fixture.db().clone();
        let db_b = fixture.db().clone();
        let supplier_a = supplier_id.clone();
        let supplier_b = supplier_id.clone();
        let account_a = account_one.clone();
        let account_b = account_one.clone();
        let task_a = tokio::spawn(async move {
            PayableService::new(db_a)
                .register_purchase_invoice(
                    RegisterPurchaseInvoiceRequest {
                        idempotency_key: "register-key-6a".to_string(),
                        invoice_code: None,
                        invoice_no: "INV-1006".to_string(),
                        invoice_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
                        gross_amount: amount("1000.00"),
                        net_amount: amount("940.00"),
                        tax_amount: amount("60.00"),
                        supplier_id: supplier_a,
                        allocations: vec![line(account_a, "1000.00", "940.00", "60.00")],
                    },
                    &actor(),
                )
                .await
        });
        let task_b = tokio::spawn(async move {
            PayableService::new(db_b)
                .register_purchase_invoice(
                    RegisterPurchaseInvoiceRequest {
                        idempotency_key: "register-key-6b".to_string(),
                        invoice_code: None,
                        invoice_no: "INV-1006".to_string(),
                        invoice_date: BusinessDate::from_ymd(2026, 9, 1).unwrap(),
                        gross_amount: amount("1000.00"),
                        net_amount: amount("940.00"),
                        tax_amount: amount("60.00"),
                        supplier_id: supplier_b,
                        allocations: vec![line(account_b, "1000.00", "940.00", "60.00")],
                    },
                    &actor(),
                )
                .await
        });
        let result_a = task_a.await.expect("任务 A 失败");
        let result_b = task_b.await.expect("任务 B 失败");
        let succeeded = [&result_a, &result_b]
            .iter()
            .filter(|result| result.is_ok())
            .count();
        assert_eq!(succeeded, 1, "同号码并发登记只允许一方成功");

        let one = fixture
            .db()
            .payable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(one.invoiced_total, amount("1000.00"), "进度只推进一次");
        let invoice = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(
                entities::receivable::InvoiceDirection::Purchase,
                "INV-1006",
                &mut NoTransaction,
            )
            .await
            .expect("发票查询失败")
            .expect("发票必须存在");
        let allocations = fixture
            .db()
            .purchase_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new(invoice.base.id)], &mut NoTransaction)
            .await
            .expect("分配查询失败");
        assert_eq!(allocations.len(), 1, "败者不得留下分配行");
    });
}
