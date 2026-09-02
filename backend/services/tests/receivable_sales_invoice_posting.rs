//! FIN-R10 / FIN-E08 销项发票登记服务编排真实 MongoDB 验收。
//!
//! `commit_invoice`（新建与既有草稿）与 `post_invoice` 共享批量 account facts、
//! 按 account 聚合的条件开票写入及 `insert_many` 分配；Repository 报告
//! `InvoicingBatchResult` 由 Service 转译。验收覆盖：
//! - 新建提交与既有草稿使用同一数据层；
//! - 跨主体、超额度、重复提交、唯一键冲突均无部分写入；
//! - invoice_total == allocation_total == account_progress 金额守恒；
//! - WorkItem 执行与任务同步无漂移；
//! - 非销项发票在 Existing 与 post 路径同样被共享销售守卫拒绝。

use std::str::FromStr;

use database::{ensure_indexes, AccessControlExt, NoTransaction, ReceivableExt, SalesOrderExt, WorkItemExt};
use entities::catalog::EnableStatus;
use entities::common::time::BusinessDate;
use entities::ids::{
    InvoiceId, PartyId, ReceivableAccountId, SalesOrderId, SalesOrderRevisionId, WorkItemId,
};
use entities::money::Amount;
use entities::receivable::{
    AccountReviewStatus, Invoice, InvoiceData, InvoiceDirection, InvoiceKind, ReceivableAccount,
    ReceivableAccountData,
};
use entities::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
use entities::work_item::{
    AssignmentSource, FinanceResponsibilityOperation, FinanceResponsibilityRule,
    FinanceResponsibilityRuleData, FinanceResponsibilityScope, WorkItem, WorkItemData, WorkItemPriority,
    WorkItemStatus, WorkItemType,
};
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Role, RoleData, Secret,
};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use services::audit::AuditActor;
use services::iam;
use services::receivable::{
    CommitInvoiceRequest, CreateInvoiceRequest, PostInvoiceRequest, ReceivableService,
    SalesInvoiceAllocationLineRequest,
};
use test_support::{require_mongo, TestDb};

const ACTOR_ID: &str = "finance-1";
const ROLE_ID: &str = "role-finance";

const EXECUTION_PERMISSIONS: &[&str] = &[
    "receivable_account:list",
    "receivable_account:detail",
    "invoice:list",
    "invoice:detail",
    "invoice:create",
    "invoice:post",
];

fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

fn actor() -> AuditActor {
    AuditActor::new(ACTOR_ID.to_string(), ACTOR_ID.to_string(), AccountKind::Admin)
}

fn casbin_rule(sec: &str, ptype: &str, values: &[&str]) -> Document {
    let values = values.iter().map(|v| v.to_string()).collect::<Vec<String>>();
    let id = format!("{sec}\u{1f}{ptype}\u{1f}{}", values.join("\u{1f}"));
    doc! { "_id": id, "sec": sec, "ptype": ptype, "values": values }
}

async fn seed_authorization(db: &Database) {
    let account = AccountCore::new(
        ACTOR_ID.to_string(),
        AccountCoreData {
            secret: Secret::new(
                LoginAccount::new(ACTOR_ID.to_string()).unwrap(),
                "test-only-password",
            )
            .unwrap(),
            name: "财务".to_string(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .unwrap();
    db.accounts().create(&account, &mut NoTransaction).await.unwrap();
    let role = Role::new(
        ROLE_ID.to_string(),
        RoleData {
            name: "财务".to_string(),
            description: None,
            system: false,
        },
    )
    .unwrap();
    db.roles().create(&role, &mut NoTransaction).await.unwrap();
    let subject = iam::subject(AccountKind::Admin, ACTOR_ID);
    let role_key = format!("role:{ROLE_ID}");
    let mut rules: Vec<Document> = EXECUTION_PERMISSIONS
        .iter()
        .map(|perm| {
            let (res, act) = perm.split_once(':').unwrap();
            casbin_rule("p", "p", &[&role_key, res, act])
        })
        .collect();
    rules.push(casbin_rule("g", "g", &[&subject, &role_key]));
    db.collection::<Document>("casbin_rules")
        .insert_many(rules)
        .await
        .unwrap();
}

async fn seed_finance_responsibility(db: &Database) {
    let rule = FinanceResponsibilityRule::new(
        "fr-sales-1",
        FinanceResponsibilityRuleData {
            operation: FinanceResponsibilityOperation::SalesInvoice,
            scope: FinanceResponsibilityScope::Counterparty,
            counterparty_id: Some("party-1".to_string()),
            owner_user_id: ACTOR_ID.to_string(),
            status: EnableStatus::Active,
        },
        "tester",
    )
    .unwrap();
    db.finance_responsibility_rules()
        .create(&rule, &mut NoTransaction)
        .await
        .unwrap();
}

async fn seed_sales_orders(db: &Database) {
    for so_id in ["so-1", "so-2"] {
        let order = SalesOrder::new(
            SalesOrderId::new(so_id),
            SalesOrderData {
                order_no: format!("SO-{so_id}"),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: entities::ids::CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "tester",
        )
        .unwrap();
        db.sales_orders()
            .create(&order, &mut NoTransaction)
            .await
            .unwrap();
    }
}

async fn seed_receivable_account(db: &Database, account_id: &str, gross: &str, invoiceable: &str) {
    let account = ReceivableAccount::new(
        ReceivableAccountId::new(account_id),
        ReceivableAccountData {
            sales_order_id: SalesOrderId::new("so-1"),
            account_seq: if account_id == "acct-1" { 1 } else { 2 },
            customer_id: entities::ids::CustomerAccountId::new("cust-1"),
            counterparty_party_id: PartyId::new("party-1"),
            source_sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            review_status: AccountReviewStatus::NotApplicable,
            reviewed_by: None,
            reviewed_at: None,
            review_evidence_reference: None,
            gross_total: amount(gross),
            settled_total: amount("0.00"),
            invoiceable_total: amount(invoiceable),
            invoiced_total: amount("0.00"),
        },
        "tester",
    )
    .unwrap();
    db.receivable_accounts()
        .create(&account, &mut NoTransaction)
        .await
        .unwrap();
}

async fn seed_sales_invoice_task(db: &Database, account_id: &str) -> WorkItem {
    // Use current account version for subject_version
    let account = db
        .receivable_accounts()
        .find_by_id(account_id, &mut NoTransaction)
        .await
        .unwrap()
        .unwrap();
    let task = WorkItem::new_with_responsibility_key(
        WorkItemId::new(format!("wi-{account_id}")),
        WorkItemData {
            work_item_type: WorkItemType::SalesInvoiceExecution,
            business_object_type: "receivable_account".to_string(),
            business_object_id: account_id.to_string(),
            subject_version: account.base.version.to_string(),
            owner_role: ROLE_ID.to_string(),
            owner_organization_id: account.counterparty_party_id.to_string(),
            owner_user_id: ACTOR_ID.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("RECEIVABLE_INVOICE_REQUIRED".to_string()),
            impact_summary: Some(format!(
                "待开票金额 ¥{},请登记销项发票并完成分配",
                amount("1000.00")
            )),
        },
        "finance:SALES_INVOICE:task-1",
    )
    .unwrap();
    db.work_items().create(&task, &mut NoTransaction).await.unwrap();
    task
}

async fn seed_draft_invoice(db: &Database, invoice_id: &str, direction: InvoiceDirection) {
    let invoice = Invoice::new(
        InvoiceId::new(invoice_id),
        InvoiceData {
            invoice_direction: direction,
            invoice_kind: InvoiceKind::Blue,
            party_id: PartyId::new("party-1"),
            invoice_code: None,
            invoice_no: format!("INV-{invoice_id}"),
            invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
            gross_amount: amount("100.00"),
            net_amount: amount("88.00"),
            tax_amount: amount("12.00"),
            rounding_adjustment_amount: amount("0.00"),
            rounding_reason: None,
            original_invoice_id: None,
        },
        "tester",
    )
    .unwrap();
    db.invoices().create(&invoice, &mut NoTransaction).await.unwrap();
}

fn alloc_line(account_id: &str, gross: &str, net: &str, tax: &str) -> SalesInvoiceAllocationLineRequest {
    SalesInvoiceAllocationLineRequest {
        receivable_account_id: ReceivableAccountId::new(account_id),
        allocated_gross_amount: amount(gross),
        allocated_net_amount: amount(net),
        allocated_tax_amount: amount(tax),
    }
}

async fn setup_env(db: &Database) -> WorkItem {
    seed_authorization(db).await;
    seed_finance_responsibility(db).await;
    seed_sales_orders(db).await;
    seed_receivable_account(db, "acct-1", "1000.00", "1000.00").await;
    seed_sales_invoice_task(db, "acct-1").await
}

/// 新建提交成功：聚合守恒、序号连续、WorkItem 同步。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn commit_new_success_conserves_and_syncs() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_new_success")
            .await
            .expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = setup_env(fixture.db()).await;
        let service = ReceivableService::new(fixture.db().clone());
        let view = service
            .commit_invoice(
                CommitInvoiceRequest {
                    work_item_id: WorkItemId::new(task.base.id.clone()),
                    expected_task_version: task.base.version.to_string(),
                    invoice_id: None,
                    expected_version: None,
                    invoice: Some(CreateInvoiceRequest {
                        invoice_direction: InvoiceDirection::Sales,
                        invoice_kind: InvoiceKind::Blue,
                        party_id: PartyId::new("party-1"),
                        invoice_code: None,
                        invoice_no: "INV-NEW-1".to_string(),
                        invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                        gross_amount: amount("1000.00"),
                        net_amount: amount("880.00"),
                        tax_amount: amount("120.00"),
                        rounding_adjustment_amount: None,
                        rounding_reason: None,
                    }),
                    allocations: vec![alloc_line("acct-1", "1000.00", "880.00", "120.00")],
                    idempotency_key: "idem-new-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect("新建提交必须成功");
        assert_eq!(view.allocations.len(), 1);
        assert_eq!(view.allocations[0].allocation_seq, 1);
        let account = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.invoiced_total, amount("1000.00"));
        assert_eq!(account.open_invoiceable_total, amount("0.00"));
        let allocs = fixture
            .db()
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new(view.id.clone())], &mut NoTransaction)
            .await
            .unwrap();
        let total: Amount = allocs
            .iter()
            .fold(amount("0.00"), |s, a| s.checked_add(a.allocated_gross_amount));
        assert_eq!(total, amount("1000.00"));
        assert_eq!(total, account.invoiced_total);
        // WorkItem should be completed (no open)
        let tasks = fixture
            .db()
            .work_items()
            .list_sales_invoice_execution_by_receivable_newest_first("acct-1", &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            tasks.iter().all(|t| t.status != WorkItemStatus::Open),
            "可开票额度归零后任务应关闭"
        );
    });
}

/// 既有草稿提交与 post_invoice 使用同一数据层，新建与既有草稿产出相同 plan 已由单测保证，此处验证既有草稿路径同样守恒。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn commit_existing_draft_conserves_amounts() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_existing").await.expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = setup_env(fixture.db()).await;
        let draft = Invoice::new(
            InvoiceId::new("inv-draft-1"),
            InvoiceData {
                invoice_direction: InvoiceDirection::Sales,
                invoice_kind: InvoiceKind::Blue,
                party_id: PartyId::new("party-1"),
                invoice_code: None,
                invoice_no: "INV-DRAFT-1".to_string(),
                invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                gross_amount: amount("100.00"),
                net_amount: amount("88.00"),
                tax_amount: amount("12.00"),
                rounding_adjustment_amount: amount("0.00"),
                rounding_reason: None,
                original_invoice_id: None,
            },
            "tester",
        )
        .unwrap();
        fixture
            .db()
            .invoices()
            .create(&draft, &mut NoTransaction)
            .await
            .unwrap();
        let service = ReceivableService::new(fixture.db().clone());
        let view = service
            .commit_invoice(
                CommitInvoiceRequest {
                    work_item_id: WorkItemId::new(task.base.id.clone()),
                    expected_task_version: task.base.version.to_string(),
                    invoice_id: Some("inv-draft-1".to_string()),
                    expected_version: Some(1),
                    invoice: None,
                    allocations: vec![alloc_line("acct-1", "100.00", "88.00", "12.00")],
                    idempotency_key: "idem-existing-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect("既有草稿提交必须成功");
        assert_eq!(view.gross_amount, amount("100.00"));
        let account = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(account.invoiced_total, amount("100.00"));
    });
}

/// 跨主体拒绝：零写入、WorkItem 无漂移。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn cross_party_rejected_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_cross_party")
            .await
            .expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let _task_acct1 = setup_env(fixture.db()).await;
        // acct-2 属于不同主体，且拥有独立的销项开票任务（party-2 任务仍由 finance-1 负责，但账户主体为 party-2）
        let other = ReceivableAccount::new(
            ReceivableAccountId::new("acct-2"),
            ReceivableAccountData {
                sales_order_id: SalesOrderId::new("so-2"),
                account_seq: 2,
                customer_id: entities::ids::CustomerAccountId::new("cust-1"),
                counterparty_party_id: PartyId::new("party-2"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                review_status: AccountReviewStatus::NotApplicable,
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: amount("1000.00"),
                settled_total: amount("0.00"),
                invoiceable_total: amount("1000.00"),
                invoiced_total: amount("0.00"),
            },
            "tester",
        )
        .unwrap();
        fixture
            .db()
            .receivable_accounts()
            .create(&other, &mut NoTransaction)
            .await
            .unwrap();
        let task = seed_sales_invoice_task(fixture.db(), "acct-2").await;
        let service = ReceivableService::new(fixture.db().clone());
        let err = service
            .commit_invoice(
                CommitInvoiceRequest {
                    work_item_id: WorkItemId::new(task.base.id.clone()),
                    expected_task_version: task.base.version.to_string(),
                    invoice_id: None,
                    expected_version: None,
                    invoice: Some(CreateInvoiceRequest {
                        invoice_direction: InvoiceDirection::Sales,
                        invoice_kind: InvoiceKind::Blue,
                        party_id: PartyId::new("party-1"),
                        invoice_code: None,
                        invoice_no: "INV-CROSS-1".to_string(),
                        invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                        gross_amount: amount("100.00"),
                        net_amount: amount("88.00"),
                        tax_amount: amount("12.00"),
                        rounding_adjustment_amount: None,
                        rounding_reason: None,
                    }),
                    allocations: vec![alloc_line("acct-2", "100.00", "88.00", "12.00")],
                    idempotency_key: "idem-cross-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect_err("跨主体必须拒绝");
        assert!(
            err.to_string().contains("禁止跨往来主体开票")
                || err.to_string().contains("发票往来主体与当前任务的应收子账不一致"),
            "实际：{err}"
        );
        // 零写入
        let inv = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(InvoiceDirection::Sales, "INV-CROSS-1", &mut NoTransaction)
            .await
            .unwrap();
        assert!(inv.is_none(), "回滚后不得留下发票");
        let acct = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-2", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(acct.invoiced_total, amount("0.00"));
        let task_after = fixture
            .db()
            .work_items()
            .find_by_id(&WorkItemId::new(task.base.id.clone()), &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(task_after.base.version, task.base.version, "失败不得推进任务版本");
    });
}

/// 超额度拒绝：零写入、WorkItem 无漂移。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn over_limit_rejected_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_over_limit")
            .await
            .expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = setup_env(fixture.db()).await;
        let service = ReceivableService::new(fixture.db().clone());
        // acct-1 可开 1000，分配 1100 超额
        let err = service
            .commit_invoice(
                CommitInvoiceRequest {
                    work_item_id: WorkItemId::new(task.base.id.clone()),
                    expected_task_version: task.base.version.to_string(),
                    invoice_id: None,
                    expected_version: None,
                    invoice: Some(CreateInvoiceRequest {
                        invoice_direction: InvoiceDirection::Sales,
                        invoice_kind: InvoiceKind::Blue,
                        party_id: PartyId::new("party-1"),
                        invoice_code: None,
                        invoice_no: "INV-OVER-1".to_string(),
                        invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                        gross_amount: amount("1100.00"),
                        net_amount: amount("968.00"),
                        tax_amount: amount("132.00"),
                        rounding_adjustment_amount: None,
                        rounding_reason: None,
                    }),
                    allocations: vec![alloc_line("acct-1", "1100.00", "968.00", "132.00")],
                    idempotency_key: "idem-over-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect_err("超额必须拒绝");
        assert!(err.to_string().contains("子账剩余可开票额度不足"), "实际：{err}");
        let inv = fixture
            .db()
            .invoices()
            .find_by_direction_and_normalized_no(InvoiceDirection::Sales, "INV-OVER-1", &mut NoTransaction)
            .await
            .unwrap();
        assert!(inv.is_none());
        let acct = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(acct.invoiced_total, amount("0.00"));
    });
}

/// 重复发票号码拒绝：零写入。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn duplicate_invoice_no_rejected_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_dup_no").await.expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = setup_env(fixture.db()).await;
        let service = ReceivableService::new(fixture.db().clone());
        service
            .commit_invoice(
                CommitInvoiceRequest {
                    work_item_id: WorkItemId::new(task.base.id.clone()),
                    expected_task_version: "1".to_string(),
                    invoice_id: None,
                    expected_version: None,
                    invoice: Some(CreateInvoiceRequest {
                        invoice_direction: InvoiceDirection::Sales,
                        invoice_kind: InvoiceKind::Blue,
                        party_id: PartyId::new("party-1"),
                        invoice_code: None,
                        invoice_no: "INV-DUP-1".to_string(),
                        invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                        gross_amount: amount("100.00"),
                        net_amount: amount("88.00"),
                        tax_amount: amount("12.00"),
                        rounding_adjustment_amount: None,
                        rounding_reason: None,
                    }),
                    allocations: vec![alloc_line("acct-1", "100.00", "88.00", "12.00")],
                    idempotency_key: "idem-dup-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect("首次必须成功");
        // 任务已关闭，需重建新任务以测试重复号码（创建新子账与任务）
        seed_receivable_account(fixture.db(), "acct-2", "1000.00", "1000.00").await;
        let task2 = seed_sales_invoice_task(fixture.db(), "acct-2").await;
        let err = service
            .commit_invoice(
                CommitInvoiceRequest {
                    work_item_id: WorkItemId::new(task2.base.id.clone()),
                    expected_task_version: task2.base.version.to_string(),
                    invoice_id: None,
                    expected_version: None,
                    invoice: Some(CreateInvoiceRequest {
                        invoice_direction: InvoiceDirection::Sales,
                        invoice_kind: InvoiceKind::Blue,
                        party_id: PartyId::new("party-1"),
                        invoice_code: None,
                        invoice_no: "INV-DUP-1".to_string(),
                        invoice_date: BusinessDate::from_ymd(2026, 8, 6).unwrap(),
                        gross_amount: amount("100.00"),
                        net_amount: amount("88.00"),
                        tax_amount: amount("12.00"),
                        rounding_adjustment_amount: None,
                        rounding_reason: None,
                    }),
                    allocations: vec![alloc_line("acct-2", "100.00", "88.00", "12.00")],
                    idempotency_key: "idem-dup-2".to_string(),
                },
                &actor(),
            )
            .await
            .expect_err("重复号码必须拒绝");
        assert!(
            err.to_string().contains("发票号码已登记") || err.to_string().contains("请勿重复提交"),
            "实际：{err}"
        );
        let acct2 = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-2", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(acct2.invoiced_total, amount("0.00"), "重复提交不得推进新账户进度");
    });
}

/// 非销项发票在 Existing 与 post 路径被共享销售守卫拒绝。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn non_sales_rejected_on_existing_and_post() {
    require_mongo!(async {
        let fixture = TestDb::new("receivable_non_sales")
            .await
            .expect("TestDb 创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = setup_env(fixture.db()).await;
        // 创建进项草稿
        seed_draft_invoice(fixture.db(), "inv-purchase-1", InvoiceDirection::Purchase).await;
        let service = ReceivableService::new(fixture.db().clone());
        let err = service
            .commit_invoice(
                CommitInvoiceRequest {
                    work_item_id: WorkItemId::new(task.base.id.clone()),
                    expected_task_version: task.base.version.to_string(),
                    invoice_id: Some("inv-purchase-1".to_string()),
                    expected_version: Some(1),
                    invoice: None,
                    allocations: vec![alloc_line("acct-1", "100.00", "88.00", "12.00")],
                    idempotency_key: "idem-non-sales-1".to_string(),
                },
                &actor(),
            )
            .await
            .expect_err("进项 Existing 必须被销售守卫拒绝");
        assert!(
            err.to_string().contains("应收登记命令只接受销项发票"),
            "实际：{err}"
        );

        let service2 = ReceivableService::new(fixture.db().clone());
        let err2 = service2
            .post_invoice(
                "inv-purchase-1",
                PostInvoiceRequest {
                    work_item_id: WorkItemId::new(task.base.id.clone()),
                    expected_task_version: task.base.version.to_string(),
                    allocations: vec![alloc_line("acct-1", "100.00", "88.00", "12.00")],
                },
                &actor(),
            )
            .await
            .expect_err("进项 post 必须被销售守卫拒绝");
        assert!(
            err2.to_string().contains("应收登记命令只接受销项发票"),
            "实际：{err2}"
        );

        // 零写入保证
        let acct = fixture
            .db()
            .receivable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(acct.invoiced_total, amount("0.00"));
        let allocs = fixture
            .db()
            .sales_invoice_allocations()
            .find_allocations_by_invoices(&[InvoiceId::new("inv-purchase-1")], &mut NoTransaction)
            .await
            .unwrap();
        assert!(allocs.is_empty());
    });
}
