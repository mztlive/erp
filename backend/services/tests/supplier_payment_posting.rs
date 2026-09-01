//! 供应商付款过账（FIN-R05 / FIN-E02）服务编排真实 MongoDB 验收。
//!
//! `post_supplier_payment_in_transaction` 的数据面已归位 `PaymentAllocationLedger`
//! （净额/余额/序号/构造）与批量仓储（分录/子账去重批量读、按账户聚合的条件
//! 更新、分配批量插入），事务、任务责任、收款账户 CAS 与审计仍由 Service
//! 编排。本文件以真实 Mongo 驱动公开 `PayableService::commit_supplier_payment`
//! 全链路，验证：重复 entry 行与同账户多分录的金额守恒、序号连续、子账进度
//! 按聚合值推进；跨供应商、超分录开放余额、超子账开放余额、超付款总额与
//! 付款任务版本冲突（任务同步失败）时全事务回滚、零残留且错误稳定。

use std::str::FromStr;

use database::{
    ensure_indexes, AccessControlExt, FileAssetExt, NoTransaction, PartyExt, PayableExt, SupplierExt,
    WorkItemExt,
};
use entities::catalog::EnableStatus;
use entities::common::time::{BusinessDate, Instant};
use entities::file_asset::{ContentHmac, FileAsset, FileAssetData, RetentionClass, SensitivityClass};
use entities::ids::{
    FileAssetId, PartyBankAccountId, PartyId, PayableAccountId, PayableEntryId, SupplierAccountId,
    SupplierPaymentId, WorkItemId,
};
use entities::money::Amount;
use entities::party::{EffectiveRecordStatus, PartyBankAccount, PartyBankAccountData};
use entities::payable::{
    EntryDirection, PayableAccount, PayableAccountData, PayableEntry, PayableEntryData, PayableEntryType,
    PayableSourceType, SupplierPaymentStatus,
};
use entities::supplier::{SupplierAccount, SupplierAccountData, SupplierAccountStatus};
use entities::work_item::{
    AssignmentSource, FinanceResponsibilityOperation, FinanceResponsibilityRule,
    FinanceResponsibilityRuleData, FinanceResponsibilityScope, WorkItem, WorkItemData, WorkItemPriority,
    WorkItemStatus, WorkItemType,
};
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Role, RoleData, Secret,
};
use id_generator::next_id;
use mongodb::bson::{doc, Document};
use mongodb::Database;
use services::audit::AuditActor;
use services::iam;
use services::payable::{
    CommitSupplierPaymentRequest, CreateSupplierPaymentRequest, PayableService, PaymentAllocationLineRequest,
};
use test_support::{require_mongo, TestDb};

/// 财务操作人账号（与任务责任人、账号种子一致）。
const ACTOR_ID: &str = "finance-1";
/// 付款执行角色（与 `payment_task` 的 `PAYMENT_OWNER_ROLE` 一致）。
const ROLE_ID: &str = "role-finance";
/// 供应商默认收款账户。
const BANK_ACCOUNT_ID: &str = "bank-1";
/// 银行回单文件资产。
const RECEIPT_ASSET_ID: &str = "asset-receipt-1";

/// 付款执行任务需要的完整权限集合（与实体注册的执行权限一致）。
const EXECUTION_PERMISSIONS: &[&str] = &[
    "payable_account:list",
    "payable_account:detail",
    "party_bank_account:reveal",
    "supplier_payment:list",
    "supplier_payment:detail",
    "supplier_payment:commit",
];

/// 解析测试金额。
fn amount(value: &str) -> Amount {
    Amount::from_str(value).unwrap()
}

/// 构造与持久化账号一致的审计身份。
fn actor() -> AuditActor {
    AuditActor::new(ACTOR_ID.to_string(), ACTOR_ID.to_string(), AccountKind::Admin)
}

/// 构造与 Casbin Mongo Adapter 完全同构的规则文档。
fn casbin_rule(sec: &str, ptype: &str, values: &[&str]) -> Document {
    let values = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<String>>();
    let id = format!("{sec}\u{1f}{ptype}\u{1f}{}", values.join("\u{1f}"));
    doc! { "_id": id, "sec": sec, "ptype": ptype, "values": values }
}

/// 建立付款过账环境：账号/角色/Casbin 授权、财务责任规则、供应商、应付子账
/// 与分录、默认收款账户、银行回单与开放付款执行任务。
///
/// # 参数
/// * `gross` - 应付子账含税总额（同时作为可收票额度）
/// * `entries` - `(分录 ID, 金额)` 列表，全部挂到 `acct-1`
///
/// # 返回
/// 返回已写入的开放付款执行任务。
async fn seed_payment_env(db: &Database, gross: &str, entries: &[(&str, &str)]) -> WorkItem {
    seed_authorization(db).await;
    seed_finance_responsibility(db).await;
    seed_supplier(db).await;
    seed_account_and_entries(db, gross, entries).await;
    seed_bank_account(db).await;
    seed_receipt_asset(db).await;
    seed_payment_task(db, gross).await
}

/// 写入账号、启用角色与 Casbin 权限/绑定规则。
async fn seed_authorization(db: &Database) {
    let account = AccountCore::new(
        ACTOR_ID.to_string(),
        AccountCoreData {
            secret: Secret::new(
                LoginAccount::new(ACTOR_ID.to_string()).expect("测试登录账号"),
                "test-only-password",
            )
            .expect("测试凭证"),
            name: "财务".to_string(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .expect("测试账号");
    db.accounts()
        .create(&account, &mut NoTransaction)
        .await
        .expect("账号写入失败");

    let role = Role::new(
        ROLE_ID.to_string(),
        RoleData {
            name: "财务".to_string(),
            description: None,
            system: false,
        },
    )
    .expect("测试角色");
    db.roles()
        .create(&role, &mut NoTransaction)
        .await
        .expect("角色写入失败");

    let subject = iam::subject(AccountKind::Admin, ACTOR_ID);
    let role_key = format!("role:{ROLE_ID}");
    let mut rules: Vec<Document> = EXECUTION_PERMISSIONS
        .iter()
        .map(|permission| {
            let (resource, action) = permission.split_once(':').expect("权限格式固定");
            casbin_rule("p", "p", &[&role_key, resource, action])
        })
        .collect();
    rules.push(casbin_rule("g", "g", &[&subject, &role_key]));
    db.collection::<Document>("casbin_rules")
        .insert_many(rules)
        .await
        .expect("策略写入失败");
}

/// 写入供应商付款的精确财务责任规则（负责人为财务操作人）。
async fn seed_finance_responsibility(db: &Database) {
    let rule = FinanceResponsibilityRule::new(
        "fr-rule-1",
        FinanceResponsibilityRuleData {
            operation: FinanceResponsibilityOperation::SupplierPayment,
            scope: FinanceResponsibilityScope::Counterparty,
            counterparty_id: Some("sup-1".to_string()),
            owner_user_id: ACTOR_ID.to_string(),
            status: EnableStatus::Active,
        },
        "tester",
    )
    .expect("责任规则构造失败");
    db.finance_responsibility_rules()
        .create(&rule, &mut NoTransaction)
        .await
        .expect("责任规则写入失败");
}

/// 写入收款供应商（party-1）。
async fn seed_supplier(db: &Database) {
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
}

/// 写入应付子账 `acct-1` 与挂账分录。
async fn seed_account_and_entries(db: &Database, gross: &str, entries: &[(&str, &str)]) {
    let account = PayableAccount::new(
        PayableAccountId::new("acct-1"),
        PayableAccountData {
            source_document_id: "PO-1".to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            source_type: PayableSourceType::PurchaseOrder,
            gross_total: amount(gross),
            settled_total: amount("0.00"),
            invoiceable_total: amount(gross),
            invoiced_total: amount("0.00"),
        },
        "tester",
    )
    .expect("子账构造失败");
    db.payable_accounts()
        .create(&account, &mut NoTransaction)
        .await
        .expect("子账写入失败");

    for (index, (entry_id, entry_amount)) in entries.iter().enumerate() {
        let entry = PayableEntry::new(
            PayableEntryId::new(*entry_id),
            PayableEntryData {
                payable_account_id: PayableAccountId::new("acct-1"),
                entry_type: PayableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: amount(entry_amount),
                due_date: BusinessDate::from_ymd(2026, 10, 1).unwrap(),
                source_fact_type: "PURCHASE_ORDER".to_string(),
                source_document_id: "PO-1".to_string(),
                source_revision_id: "rev-1".to_string(),
                source_sequence: (index + 1) as u32,
                posted_at: Instant::now(),
            },
        )
        .expect("分录构造失败");
        db.payable_entries()
            .create(&entry, &mut NoTransaction)
            .await
            .expect("分录写入失败");
    }
}

/// 写入供应商所属主体的唯一当前默认收款账户。
async fn seed_bank_account(db: &Database) {
    let bank = PartyBankAccount::new(
        PartyBankAccountId::new(BANK_ACCOUNT_ID),
        PartyBankAccountData {
            bank_account_no: "BA-001".to_string(),
            party_id: PartyId::new("party-1"),
            account_name: "供应商收款户".to_string(),
            bank_name: "测试银行".to_string(),
            bank_branch_name: None,
            account_number: "6222021234567890123".to_string(),
            valid_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            valid_to: None,
            is_default: true,
            status: EffectiveRecordStatus::Active,
        },
        b"test-fingerprint-key-0123456789abcdef",
        "tester",
    )
    .expect("收款账户构造失败");
    db.party_bank_accounts()
        .create(&bank, &mut NoTransaction)
        .await
        .expect("收款账户写入失败");
}

/// 写入可用的银行回单文件资产（敏感、长期保留图片）。
async fn seed_receipt_asset(db: &Database) {
    let asset = FileAsset::new(
        FileAssetId::new(RECEIPT_ASSET_ID),
        FileAssetData {
            storage_object_key: "receipts/test-1.png".to_string(),
            file_name: "回单.png".to_string(),
            content_type: "image/png".to_string(),
            byte_size: 1024,
            content_hmac: ContentHmac::parse("a".repeat(64)).expect("指纹"),
            sensitivity_class: SensitivityClass::Sensitive,
            retention_class: RetentionClass::LongTerm,
            expires_at: None,
            created_by: "tester".to_string(),
        },
    )
    .expect("回单构造失败");
    db.file_assets()
        .create(&asset, &mut NoTransaction)
        .await
        .expect("回单写入失败");
}

/// 写入 `acct-1` 的开放付款执行任务（责任人为财务操作人）。
async fn seed_payment_task(db: &Database, gross: &str) -> WorkItem {
    let task = WorkItem::new_with_responsibility_key(
        WorkItemId::new(next_id()),
        WorkItemData {
            work_item_type: WorkItemType::SupplierPaymentExecution,
            business_object_type: "payable_account".to_string(),
            business_object_id: "acct-1".to_string(),
            subject_version: "1".to_string(),
            owner_role: ROLE_ID.to_string(),
            owner_organization_id: "party-1".to_string(),
            owner_user_id: ACTOR_ID.to_string(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("PAYABLE_PAYMENT_REQUIRED".to_string()),
            impact_summary: Some(format!("未付金额 ¥{gross}，请按付款条件登记付款")),
        },
        "finance:SUPPLIER_PAYMENT:task-1",
    )
    .expect("任务构造失败");
    db.work_items()
        .create(&task, &mut NoTransaction)
        .await
        .expect("任务写入失败");
    task
}

/// 建立第二个供应商及其应付子账/分录（用于跨供应商场景）。
async fn seed_other_supplier(db: &Database) {
    let supplier = SupplierAccount::new(
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
    db.supplier_accounts()
        .create(&supplier, &mut NoTransaction)
        .await
        .expect("供应商写入失败");

    let account = PayableAccount::new(
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
    db.payable_accounts()
        .create(&account, &mut NoTransaction)
        .await
        .expect("子账写入失败");

    let entry = PayableEntry::new(
        PayableEntryId::new("entry-9"),
        PayableEntryData {
            payable_account_id: PayableAccountId::new("acct-9"),
            entry_type: PayableEntryType::Original,
            direction: EntryDirection::Increase,
            amount: amount("1000.00"),
            due_date: BusinessDate::from_ymd(2026, 10, 1).unwrap(),
            source_fact_type: "PURCHASE_ORDER".to_string(),
            source_document_id: "PO-9".to_string(),
            source_revision_id: "rev-9".to_string(),
            source_sequence: 1,
            posted_at: Instant::now(),
        },
    )
    .expect("分录构造失败");
    db.payable_entries()
        .create(&entry, &mut NoTransaction)
        .await
        .expect("分录写入失败");
}

/// 构造付款提交请求（固定供应商/收款账户/回单）。
fn commit_request(
    task: &WorkItem,
    payment_no: &str,
    payment_amount: &str,
    expected_task_version: String,
    allocations: Vec<PaymentAllocationLineRequest>,
) -> CommitSupplierPaymentRequest {
    CommitSupplierPaymentRequest {
        idempotency_key: format!("post-key-{payment_no}"),
        work_item_id: WorkItemId::new(task.base.id.clone()),
        expected_task_version,
        expected_payee_bank_account_id: BANK_ACCOUNT_ID.to_string(),
        expected_payee_bank_account_version: 1,
        payment: CreateSupplierPaymentRequest {
            payment_no: payment_no.to_string(),
            supplier_id: SupplierAccountId::new("sup-1"),
            paid_at: Instant::now(),
            amount: amount(payment_amount),
            bank_reference: Some("BANK-REF-1".to_string()),
            bank_receipt_asset_id: FileAssetId::new(RECEIPT_ASSET_ID),
        },
        allocations,
    }
}

/// 构造核销分配行。
fn line(entry_id: &str, allocated_amount: &str) -> PaymentAllocationLineRequest {
    PaymentAllocationLineRequest {
        payable_entry_id: PayableEntryId::new(entry_id),
        allocated_amount: amount(allocated_amount),
    }
}

/// 断言付款提交失败后无任何半写入：付款单、分配、单据注册与审计全部为空，
/// 子账进度为零，任务保持原版本且仍为开放状态。
async fn assert_zero_residue(db: &Database, payment_no: &str, account_ids: &[&str], task: &WorkItem) {
    let payment = db
        .supplier_payments()
        .find_by_payment_no(payment_no, &mut NoTransaction)
        .await
        .expect("付款单查询失败");
    assert!(payment.is_none(), "回滚后不得留下付款单");
    let allocations = db
        .collection::<Document>("payment_allocations")
        .count_documents(doc! {})
        .await
        .expect("分配计数失败");
    assert_eq!(allocations, 0, "回滚后不得留下核销分配");
    let documents = db
        .collection::<Document>("business_documents")
        .count_documents(doc! {})
        .await
        .expect("单据计数失败");
    assert_eq!(documents, 0, "回滚后不得留下单据注册");
    let audits = db
        .collection::<Document>("audit_logs")
        .count_documents(doc! {})
        .await
        .expect("审计计数失败");
    assert_eq!(audits, 0, "回滚后不得留下审计");
    for account_id in account_ids {
        let account = db
            .payable_accounts()
            .find_by_id(account_id, &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(
            account.settled_total,
            amount("0.00"),
            "回滚后不得推进子账核销进度"
        );
    }
    let current = db
        .work_items()
        .find_by_id(&task.base.id, &mut NoTransaction)
        .await
        .expect("任务查询失败")
        .expect("任务必须存在");
    assert_eq!(current.base.version, task.base.version, "回滚后任务版本不得变化");
    assert_eq!(current.status, WorkItemStatus::Open, "回滚后任务必须保持开放");
}

/// 成功过账：同一分录出现两行、同账户多分录时金额守恒，序号连续，子账进度
/// 按聚合值推进，任务随结清自动完成。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn posting_conserves_duplicate_entry_and_account_amounts() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_post_success")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = seed_payment_env(
            fixture.db(),
            "1000.00",
            &[("entry-1", "600.00"), ("entry-2", "400.00")],
        )
        .await;
        let service = PayableService::new(fixture.db().clone());

        let view = service
            .commit_supplier_payment(
                commit_request(
                    &task,
                    "PAY-001",
                    "1000.00",
                    task.base.version.to_string(),
                    vec![
                        line("entry-1", "300.00"),
                        line("entry-1", "300.00"),
                        line("entry-2", "400.00"),
                    ],
                ),
                &actor(),
            )
            .await
            .expect("过账必须成功");

        assert_eq!(view.status, SupplierPaymentStatus::Posted);
        assert_eq!(view.allocated_total, amount("1000.00"));
        assert_eq!(view.unallocated_amount, amount("0.00"));
        assert_eq!(view.allocations.len(), 3);

        let payment = fixture
            .db()
            .supplier_payments()
            .find_by_id(&view.id, &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("付款单必须存在");
        assert_eq!(payment.status, SupplierPaymentStatus::Posted);

        let allocations = fixture
            .db()
            .payment_allocations()
            .find_allocations_by_payments(&[SupplierPaymentId::new(view.id.clone())], &mut NoTransaction)
            .await
            .expect("分配查询失败");
        assert_eq!(allocations.len(), 3);
        let mut seqs: Vec<u32> = allocations.iter().map(|row| row.allocation_seq).collect();
        seqs.sort_unstable();
        assert_eq!(seqs, vec![1, 2, 3], "序号必须从 1 连续");
        let total: Amount = allocations
            .iter()
            .fold(amount("0.00"), |sum, row| sum.checked_add(row.allocated_amount));
        assert_eq!(total, amount("1000.00"), "分配总额必须与付款金额精确守恒");
        let entry_one_total: Amount = allocations
            .iter()
            .filter(|row| row.payable_entry_id.as_ref() == "entry-1")
            .fold(amount("0.00"), |sum, row| sum.checked_add(row.allocated_amount));
        assert_eq!(
            entry_one_total,
            amount("600.00"),
            "重复 entry 行合计必须等于分录开放余额且守恒"
        );

        let account = fixture
            .db()
            .payable_accounts()
            .find_by_id("acct-1", &mut NoTransaction)
            .await
            .expect("读取失败")
            .expect("子账必须存在");
        assert_eq!(account.settled_total, amount("1000.00"), "子账进度必须为聚合值");
        assert_eq!(account.open_total, amount("0.00"));
        assert!(account.is_settled(), "开放余额归零必须结清");

        let current_task = fixture
            .db()
            .work_items()
            .find_by_id(&task.base.id, &mut NoTransaction)
            .await
            .expect("任务查询失败")
            .expect("任务必须存在");
        assert_eq!(
            current_task.status,
            WorkItemStatus::Completed,
            "结清后任务必须自动完成"
        );
    });
}

/// 跨供应商核销：付款任务只允许核销本子账分录，异供应商分录整体拒绝，
/// 全事务回滚零残留。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn cross_supplier_allocation_rolled_back_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_post_cross_supplier")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = seed_payment_env(
            fixture.db(),
            "1000.00",
            &[("entry-1", "600.00"), ("entry-2", "400.00")],
        )
        .await;
        seed_other_supplier(fixture.db()).await;
        let service = PayableService::new(fixture.db().clone());

        let err = service
            .commit_supplier_payment(
                commit_request(
                    &task,
                    "PAY-002",
                    "1000.00",
                    task.base.version.to_string(),
                    vec![line("entry-1", "600.00"), line("entry-9", "400.00")],
                ),
                &actor(),
            )
            .await
            .expect_err("跨供应商核销必须拒绝");
        assert!(
            err.to_string()
                .contains("一次付款只能核销当前任务绑定应付子账中的分录"),
            "错误必须稳定，实际：{err}"
        );

        assert_zero_residue(fixture.db(), "PAY-002", &["acct-1", "acct-9"], &task).await;
    });
}

/// 超分录开放余额：单笔核销超过分录金额时整体拒绝，全事务回滚零残留。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn over_entry_open_balance_rolled_back_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_post_over_entry")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = seed_payment_env(
            fixture.db(),
            "1000.00",
            &[("entry-1", "600.00"), ("entry-2", "400.00")],
        )
        .await;
        let service = PayableService::new(fixture.db().clone());

        let err = service
            .commit_supplier_payment(
                commit_request(
                    &task,
                    "PAY-003",
                    "700.00",
                    task.base.version.to_string(),
                    vec![line("entry-1", "700.00")],
                ),
                &actor(),
            )
            .await
            .expect_err("超分录开放余额必须拒绝");
        assert!(
            err.to_string().contains("核销金额超过应付分录开放余额"),
            "错误必须稳定，实际：{err}"
        );

        assert_zero_residue(fixture.db(), "PAY-003", &["acct-1"], &task).await;
    });
}

/// 超子账开放余额：分录内金额足够但子账聚合开放余额不足时，批量条件核销拒绝
/// 并整体回滚，不产生半写入。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn over_account_open_balance_rolled_back_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_post_over_account")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        // 子账开放余额 500，分录金额 700：逐分录校验可通过，聚合条件更新必须拒绝。
        let task = seed_payment_env(fixture.db(), "500.00", &[("entry-1", "700.00")]).await;
        let service = PayableService::new(fixture.db().clone());

        let err = service
            .commit_supplier_payment(
                commit_request(
                    &task,
                    "PAY-004",
                    "700.00",
                    task.base.version.to_string(),
                    vec![line("entry-1", "700.00")],
                ),
                &actor(),
            )
            .await
            .expect_err("超子账开放余额必须拒绝");
        assert!(
            err.to_string().contains("子账剩余开放余额不足，核销被拒绝"),
            "错误必须稳定，实际：{err}"
        );

        assert_zero_residue(fixture.db(), "PAY-004", &["acct-1"], &task).await;
    });
}

/// 超付款总额：分配合计超过付款金额时账本构造整体拒绝，全事务回滚零残留。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn over_payment_total_rolled_back_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_post_over_total")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = seed_payment_env(fixture.db(), "1500.00", &[("entry-1", "1500.00")]).await;
        let service = PayableService::new(fixture.db().clone());

        let err = service
            .commit_supplier_payment(
                commit_request(
                    &task,
                    "PAY-005",
                    "1000.00",
                    task.base.version.to_string(),
                    vec![line("entry-1", "1200.00")],
                ),
                &actor(),
            )
            .await
            .expect_err("超付款总额必须拒绝");
        assert!(
            err.to_string().contains("核销合计超过付款金额"),
            "错误必须稳定，实际：{err}"
        );

        assert_zero_residue(fixture.db(), "PAY-005", &["acct-1"], &task).await;
    });
}

/// 付款任务版本冲突（任务同步失败）：页面冻结版本漂移时任务活动记录失败，
/// 全事务回滚，付款单/分配/子账进度/审计零残留，任务保持原版本。
#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn task_version_conflict_rolled_back_without_partial_writes() {
    require_mongo!(async {
        let fixture = TestDb::new("payable_post_task_conflict")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let task = seed_payment_env(
            fixture.db(),
            "1000.00",
            &[("entry-1", "600.00"), ("entry-2", "400.00")],
        )
        .await;
        let service = PayableService::new(fixture.db().clone());

        let err = service
            .commit_supplier_payment(
                commit_request(
                    &task,
                    "PAY-006",
                    "1000.00",
                    (task.base.version + 1).to_string(),
                    vec![line("entry-1", "600.00"), line("entry-2", "400.00")],
                ),
                &actor(),
            )
            .await
            .expect_err("任务版本冲突必须拒绝");
        assert!(
            err.to_string()
                .contains("付款任务版本已变化，请刷新工作台任务后重试"),
            "错误必须稳定，实际：{err}"
        );

        assert_zero_residue(fixture.db(), "PAY-006", &["acct-1"], &task).await;
    });
}
