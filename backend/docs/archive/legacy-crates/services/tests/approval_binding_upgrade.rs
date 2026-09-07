//! APP-S06：审批绑定升级强事实、授权、幂等与事务原子性的真实 MongoDB 验收。
//!
//! 用例只使用随机独立数据库；`ERP_TEST_MONGO_URI` 必须指向启用
//! `enableTestCommands` 的 MongoDB 7 副本集。测试结束时精确 drop 随机库，
//! 不停止或重置共享 MongoDB 容器。

use std::str::FromStr;
use std::sync::Arc;

use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use bpm::model::types::ApprovalTransitionEvent;
use bpm::model::{
    ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, NewNodeDefinition,
    ParticipantId, ProcessKind, Timestamp,
};
use casbin::Adapter;
use database::{
    ensure_indexes, AccessControlExt, BpmExt, CustomerExt, DocumentRegistryExt, InventoryExt,
    MongoCasbinAdapter, NoTransaction, PayableExt, PurchaseOrderExt, ReceivableExt, ReturnsExt,
    SalesOrderExt, SalesReviewExt, SupplierExt,
};
use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
use entities::common::time::Instant;
use entities::customer::{CustomerAccount, CustomerAccountData, CustomerAccountStatus};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, BusinessDocumentData, BusinessDocumentId, DocumentType};
use entities::ids::{
    CustomerAccountId, CustomerReceiptId, CustomerRefundId, DataScopeId, FileAssetId, PartyBankAccountId,
    PartyId, PaymentReversalId, PurchaseChangeOrderId, PurchaseOrderId, PurchaseOrderRevisionId,
    ReceiptReversalId, SalesChangeOrderId, SalesOrderId, SalesOrderRevisionId, StockAdjustmentId,
    SupplierAccountId, SupplierPaymentId, SupplierRefundId, WarehouseId,
};
use entities::inventory::{AdjustmentReasonType, StockAdjustment, StockAdjustmentData};
use entities::money::Amount;
use entities::payable::{SupplierPayment, SupplierPaymentData};
use entities::purchase_order::{
    FulfillmentResponsibility, PurchaseChangeOrder, PurchaseChangeOrderData, PurchaseOrder,
    PurchaseOrderData, PurchaseType,
};
use entities::receivable::{CustomerReceipt, CustomerReceiptData};
use entities::returns::{
    CustomerRefund, CustomerRefundData, PaymentReversal, PaymentReversalData, ReceiptReversal,
    ReceiptReversalData, SupplierRefund, SupplierRefundData,
};
use entities::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
use entities::sales_review::{SalesChangeOrder, SalesChangeOrderData, SalesChangeType};
use entities::supplier::{SupplierAccount, SupplierAccountData, SupplierAccountStatus};
use entities::{
    AccountCore, AccountCoreData, AccountKind, AccountStatus, LoginAccount, Permission, Role, RoleData,
    RoleUpdate, Secret,
};
use mongodb::bson::{doc, Bson, Document};
use mongodb::Database;
use services::approval::binding::UpgradeBindingOutcome;
use services::approval::execution::{ApprovalRuntimeService, UpgradeBindingCommand};
use services::approval::{
    ensure_initial_unsubmitted_approval_upgrade_subject, load_approval_upgrade_subject_facts,
    ApprovalUpgradeSubjectFacts,
};
use services::audit::AuditActor;
use services::iam::{self, subject};
use services::{Error, ErrorCode};
use test_support::{require_mongo, TestDb};

const GOODS_ORDER_ID: &str = "upgrade-sales-goods";
const VOUCHER_ORDER_ID: &str = "upgrade-sales-voucher";
const SALES_CHANGE_ID: &str = "upgrade-sales-change";
const PURCHASE_ID: &str = "upgrade-purchase";
const PURCHASE_CHANGE_ID: &str = "upgrade-purchase-change";
const STOCK_ID: &str = "upgrade-stock";
const RECEIPT_ID: &str = "upgrade-customer-receipt";
const CUSTOMER_REFUND_ID: &str = "upgrade-customer-refund";
const SUPPLIER_REFUND_ID: &str = "upgrade-supplier-refund";
const RECEIPT_REVERSAL_ID: &str = "upgrade-receipt-reversal";
const PAYMENT_REVERSAL_ID: &str = "upgrade-payment-reversal";
const CUSTOMER_ID: &str = "upgrade-customer";
const SUPPLIER_ID: &str = "upgrade-supplier";
const SUPPLIER_PAYMENT_ID: &str = "upgrade-supplier-payment";
const SALES_ORG: &str = "party-sales-org";
const VOUCHER_ORG: &str = "party-voucher-org";
const RECEIPT_ORG: &str = "party-receipt-org";
const CUSTOMER_ORG: &str = "party-customer-org";
const SUPPLIER_ORG: &str = "party-supplier-org";
const WAREHOUSE_ORG: &str = "warehouse-upgrade-org";
const WRONG_WAREHOUSE: &str = "warehouse-upgrade-other";
const ADMIN_ID: &str = "upgrade-binding-admin";
const APPROVER_ID: &str = "upgrade-binding-approver";
const RUNTIME_ADMIN_ID: &str = "upgrade-binding-runtime";
const WRONG_ORG_ID: &str = "upgrade-binding-wrong-org";
const NO_READ_ID: &str = "upgrade-binding-no-read";
const DISABLED_ID: &str = "upgrade-binding-disabled";
const CREATOR_ID: &str = "upgrade-binding-creator";
const ROLE_ADMIN: &str = "upgrade-role-admin";
const ROLE_APPROVER: &str = "upgrade-role-approver";
const ROLE_RUNTIME: &str = "upgrade-role-runtime";
const ROLE_WRONG_ORG: &str = "upgrade-role-wrong-org";
const ROLE_NO_READ: &str = "upgrade-role-no-read";
const ROLE_DISABLED: &str = "upgrade-role-disabled";
const DEF_V1: &str = "upgrade-def-v1";
const DEF_V2: &str = "upgrade-def-v2";
const NODE_V1: &str = "upgrade-node-v1";
const NODE_V2: &str = "upgrade-node-v2";
const ADJUSTMENT_ID: &str = "upgrade-binding-stock";
const STARTED_ID: &str = "upgrade-binding-started";
const RECEIPTS: &str = "approval_command_receipts";
const ACTIONS: &str = "workflow_actions";
const DOCUMENTS: &str = "business_documents";
const ADJUSTMENTS: &str = "stock_adjustments";
const AUDITS: &str = "audit_logs";
const DEFINITIONS: &str = "approval_process_definitions";

/// 一条强事实矩阵的期望值。
#[derive(Debug, Clone)]
struct ExpectedSubjectFacts {
    document_type: DocumentType,
    document_id: &'static str,
    document_no: &'static str,
    responsible_org_id: &'static str,
    creator_id: &'static str,
}

fn amount() -> Amount {
    Amount::from_str("100.00").expect("测试金额")
}

fn sales_order(
    id: &str,
    order_no: &str,
    business_type: BusinessType,
    organization_id: &str,
    creator_id: &str,
) -> SalesOrder {
    SalesOrder::new(
        SalesOrderId::new(id),
        SalesOrderData {
            order_no: order_no.to_string(),
            business_type,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new(CUSTOMER_ID),
            contract_id: None,
            settlement_party_id: PartyId::new(organization_id),
            source_status_code: None,
        },
        creator_id,
    )
    .expect("销售单")
}

fn stock_adjustment(id: &str, organization_id: &str, creator_id: &str) -> StockAdjustment {
    StockAdjustment::new(
        StockAdjustmentId::new(id),
        StockAdjustmentData {
            adjustment_no: format!("ADJ-{id}"),
            warehouse_id: WarehouseId::new(organization_id),
            reason_type: AdjustmentReasonType::StockGain,
            prepared_by: "warehouse-operator".to_string(),
            note: Some("APP-S06 强事实验收".to_string()),
            occurred_at: Some(Instant::from_unix_secs(100)),
        },
        creator_id,
    )
    .expect("库存调整单")
}

fn customer_receipt(id: &str, organization_id: &str, creator_id: &str) -> CustomerReceipt {
    CustomerReceipt::new(
        CustomerReceiptId::new(id),
        CustomerReceiptData {
            receipt_no: format!("RC-{id}"),
            counterparty_party_id: PartyId::new(organization_id),
            customer_id: Some(CustomerAccountId::new(CUSTOMER_ID)),
            received_at: Instant::from_unix_secs(100),
            amount: amount(),
            bank_reference: Some(format!("BANK-{id}")),
        },
        creator_id,
    )
    .expect("客户回款单")
}

async fn seed_subject_matrix(db: &Database) -> Vec<ExpectedSubjectFacts> {
    let customer = CustomerAccount::new(
        CustomerAccountId::new(CUSTOMER_ID),
        CustomerAccountData {
            party_id: PartyId::new(CUSTOMER_ORG),
            customer_no: "CUSTOMER-UPGRADE".to_string(),
            default_payment_term_id: None,
            status: CustomerAccountStatus::Active,
        },
        "creator-customer-account",
    )
    .expect("客户账户");
    db.customer_accounts()
        .create(&customer, &mut NoTransaction)
        .await
        .expect("写入客户账户");

    let supplier = SupplierAccount::new(
        SupplierAccountId::new(SUPPLIER_ID),
        SupplierAccountData {
            party_id: PartyId::new(SUPPLIER_ORG),
            supplier_no: "SUPPLIER-UPGRADE".to_string(),
            default_payment_term_id: None,
            current_commercial_profile_revision_id: None,
            status: SupplierAccountStatus::Active,
        },
        "creator-supplier-account",
    )
    .expect("供应商账户");
    db.supplier_accounts()
        .create(&supplier, &mut NoTransaction)
        .await
        .expect("写入供应商账户");

    let goods = sales_order(
        GOODS_ORDER_ID,
        "SO-GOODS-UPGRADE",
        BusinessType::GoodsService,
        SALES_ORG,
        "creator-sales-goods",
    );
    db.sales_orders()
        .create(&goods, &mut NoTransaction)
        .await
        .expect("写入实物服务销售单");
    let voucher = sales_order(
        VOUCHER_ORDER_ID,
        "SO-VOUCHER-UPGRADE",
        BusinessType::Voucher,
        VOUCHER_ORG,
        "creator-sales-voucher",
    );
    db.sales_orders()
        .create(&voucher, &mut NoTransaction)
        .await
        .expect("写入卡券销售单");

    let sales_change = SalesChangeOrder::new(
        SalesChangeOrderId::new(SALES_CHANGE_ID),
        SalesChangeOrderData {
            sales_order_id: SalesOrderId::new(GOODS_ORDER_ID),
            base_revision_id: SalesOrderRevisionId::new("sales-revision-base"),
            change_type: SalesChangeType::Quantity,
            reason: "客户追加数量".to_string(),
        },
        "creator-sales-change",
    )
    .expect("销售变更单");
    db.sales_change_orders()
        .create(&sales_change, &mut NoTransaction)
        .await
        .expect("写入销售变更单");

    let purchase = PurchaseOrder::new(
        PurchaseOrderId::new(PURCHASE_ID),
        PurchaseOrderData {
            purchase_no: "PO-UPGRADE".to_string(),
            sales_order_id: SalesOrderId::new(GOODS_ORDER_ID),
            sales_order_revision_id: SalesOrderRevisionId::new("sales-revision-purchase"),
            creation_basis_id: "purchase-basis-upgrade".to_string(),
            supplier_id: SupplierAccountId::new(SUPPLIER_ID),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".to_string(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            owner_user_id: "buyer-upgrade".to_string(),
            target_warehouse_id: Some(WarehouseId::new(WAREHOUSE_ORG)),
        },
        "creator-purchase",
    )
    .expect("采购单");
    db.purchase_orders()
        .create(&purchase, &mut NoTransaction)
        .await
        .expect("写入采购单");

    let purchase_change = PurchaseChangeOrder::new(
        PurchaseChangeOrderId::new(PURCHASE_CHANGE_ID),
        PurchaseChangeOrderData {
            purchase_order_id: PurchaseOrderId::new(PURCHASE_ID),
            base_revision_id: PurchaseOrderRevisionId::new("purchase-revision-base"),
            reason: "采购成本调整".to_string(),
        },
        "creator-purchase-change",
    )
    .expect("采购变更单");
    db.purchase_change_orders()
        .create(&purchase_change, &mut NoTransaction)
        .await
        .expect("写入采购变更单");

    let adjustment = stock_adjustment(STOCK_ID, WAREHOUSE_ORG, "creator-stock");
    db.stock_adjustments()
        .create(&adjustment, &mut NoTransaction)
        .await
        .expect("写入库存调整单");

    let receipt = customer_receipt(RECEIPT_ID, RECEIPT_ORG, "creator-receipt");
    db.customer_receipts()
        .create(&receipt, &mut NoTransaction)
        .await
        .expect("写入客户回款单");

    let customer_refund = CustomerRefund::new(
        CustomerRefundId::new(CUSTOMER_REFUND_ID),
        CustomerRefundData {
            refund_no: "CRF-UPGRADE".to_string(),
            sales_return_case_id: None,
            customer_id: CustomerAccountId::new(CUSTOMER_ID),
            original_receipt_id: Some(CustomerReceiptId::new(RECEIPT_ID)),
            original_receivable_entry_id: None,
            reason_code: Some("QUALITY".to_string()),
            reason_text: "客户退款".to_string(),
            amount: amount(),
            handled_by: "refund-handler".to_string(),
            reviewed_by: "refund-reviewer".to_string(),
            occurred_at: Instant::from_unix_secs(100),
            evidence_attachment_id: None,
        },
        "creator-customer-refund",
    )
    .expect("客户退款单");
    db.customer_refunds()
        .create(&customer_refund, &mut NoTransaction)
        .await
        .expect("写入客户退款单");

    let supplier_payment = SupplierPayment::new(
        SupplierPaymentId::new(SUPPLIER_PAYMENT_ID),
        SupplierPaymentData {
            payment_no: "PAY-UPGRADE".to_string(),
            supplier_id: SupplierAccountId::new(SUPPLIER_ID),
            payee_bank_account_id: PartyBankAccountId::new("supplier-bank-upgrade"),
            paid_at: Instant::from_unix_secs(100),
            amount: amount(),
            bank_reference: Some("BANK-PAY-UPGRADE".to_string()),
            bank_receipt_asset_id: FileAssetId::new("asset-pay-upgrade"),
        },
    )
    .expect("供应商付款单");
    db.supplier_payments()
        .create(&supplier_payment, &mut NoTransaction)
        .await
        .expect("写入供应商付款单");

    let supplier_refund = SupplierRefund::new(
        SupplierRefundId::new(SUPPLIER_REFUND_ID),
        SupplierRefundData {
            refund_no: "SRF-UPGRADE".to_string(),
            purchase_return_order_id: None,
            supplier_id: SupplierAccountId::new(SUPPLIER_ID),
            original_payment_id: Some(SupplierPaymentId::new(SUPPLIER_PAYMENT_ID)),
            original_payable_entry_id: None,
            reason_code: Some("OVERPAY".to_string()),
            reason_text: "供应商退款".to_string(),
            amount: amount(),
            handled_by: "supplier-refund-handler".to_string(),
            reviewed_by: "supplier-refund-reviewer".to_string(),
            occurred_at: Instant::from_unix_secs(100),
            evidence_attachment_id: None,
        },
        "creator-supplier-refund",
    )
    .expect("供应商退款单");
    db.supplier_refunds()
        .create(&supplier_refund, &mut NoTransaction)
        .await
        .expect("写入供应商退款单");

    let receipt_reversal = ReceiptReversal::new(
        ReceiptReversalId::new(RECEIPT_REVERSAL_ID),
        ReceiptReversalData {
            reversal_no: "RR-UPGRADE".to_string(),
            original_customer_receipt_id: CustomerReceiptId::new(RECEIPT_ID),
            reason_code: Some("WRONG_ACCOUNT".to_string()),
            reason_text: "回款冲正".to_string(),
            amount: amount(),
            handled_by: "receipt-reversal-handler".to_string(),
            reviewed_by: "receipt-reversal-reviewer".to_string(),
            occurred_at: Instant::from_unix_secs(100),
            evidence_attachment_id: None,
        },
        "creator-receipt-reversal",
    )
    .expect("回款冲正单");
    db.receipt_reversals()
        .create(&receipt_reversal, &mut NoTransaction)
        .await
        .expect("写入回款冲正单");

    let payment_reversal = PaymentReversal::new(
        PaymentReversalId::new(PAYMENT_REVERSAL_ID),
        PaymentReversalData {
            reversal_no: "PRR-UPGRADE".to_string(),
            original_supplier_payment_id: SupplierPaymentId::new(SUPPLIER_PAYMENT_ID),
            reason_code: Some("WRONG_ACCOUNT".to_string()),
            reason_text: "付款冲正".to_string(),
            amount: amount(),
            handled_by: "payment-reversal-handler".to_string(),
            reviewed_by: "payment-reversal-reviewer".to_string(),
            occurred_at: Instant::from_unix_secs(100),
            evidence_attachment_id: None,
        },
        "creator-payment-reversal",
    )
    .expect("付款冲正单");
    db.payment_reversals()
        .create(&payment_reversal, &mut NoTransaction)
        .await
        .expect("写入付款冲正单");

    vec![
        ExpectedSubjectFacts {
            document_type: DocumentType::SalesOrder,
            document_id: GOODS_ORDER_ID,
            document_no: "SO-GOODS-UPGRADE",
            responsible_org_id: SALES_ORG,
            creator_id: "creator-sales-goods",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::VoucherSalesOrder,
            document_id: VOUCHER_ORDER_ID,
            document_no: "SO-VOUCHER-UPGRADE",
            responsible_org_id: VOUCHER_ORG,
            creator_id: "creator-sales-voucher",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::SalesChangeOrder,
            document_id: SALES_CHANGE_ID,
            document_no: "",
            responsible_org_id: SALES_ORG,
            creator_id: "creator-sales-change",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::PurchaseOrder,
            document_id: PURCHASE_ID,
            document_no: "PO-UPGRADE",
            responsible_org_id: SALES_ORG,
            creator_id: "creator-purchase",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::PurchaseChangeOrder,
            document_id: PURCHASE_CHANGE_ID,
            document_no: "",
            responsible_org_id: SALES_ORG,
            creator_id: "creator-purchase-change",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::StockAdjustment,
            document_id: STOCK_ID,
            document_no: "ADJ-upgrade-stock",
            responsible_org_id: WAREHOUSE_ORG,
            creator_id: "creator-stock",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::CustomerReceipt,
            document_id: RECEIPT_ID,
            document_no: "RC-upgrade-customer-receipt",
            responsible_org_id: RECEIPT_ORG,
            creator_id: "creator-receipt",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::CustomerRefund,
            document_id: CUSTOMER_REFUND_ID,
            document_no: "CRF-UPGRADE",
            responsible_org_id: CUSTOMER_ORG,
            creator_id: "creator-customer-refund",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::SupplierRefund,
            document_id: SUPPLIER_REFUND_ID,
            document_no: "SRF-UPGRADE",
            responsible_org_id: SUPPLIER_ORG,
            creator_id: "creator-supplier-refund",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::ReceiptReversal,
            document_id: RECEIPT_REVERSAL_ID,
            document_no: "RR-UPGRADE",
            responsible_org_id: RECEIPT_ORG,
            creator_id: "creator-receipt-reversal",
        },
        ExpectedSubjectFacts {
            document_type: DocumentType::PaymentReversal,
            document_id: PAYMENT_REVERSAL_ID,
            document_no: "PRR-UPGRADE",
            responsible_org_id: SUPPLIER_ORG,
            creator_id: "creator-payment-reversal",
        },
    ]
}

async fn load_facts(
    db: &Database,
    document_type: DocumentType,
    document_id: &str,
) -> Result<ApprovalUpgradeSubjectFacts, Error> {
    load_approval_upgrade_subject_facts(db, document_type, document_id, &mut NoTransaction).await
}

async fn cleanup(fixture: TestDb) {
    fixture.db().drop().await.expect("清理随机测试数据库");
    std::mem::forget(fixture);
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向启用 enableTestCommands 的 MongoDB 7 副本集"]
async fn all_process_required_subjects_load_exact_strong_facts_and_fail_closed() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_binding_upgrade_subjects")
            .await
            .expect("创建随机测试库");
        ensure_indexes(fixture.db()).await.expect("创建生产索引");
        let expected = seed_subject_matrix(fixture.db()).await;

        assert_eq!(expected.len(), 11, "必须穷尽十一种 PROCESS_REQUIRED 类型");
        for row in expected {
            let facts = load_facts(fixture.db(), row.document_type, row.document_id)
                .await
                .unwrap_or_else(|error| {
                    panic!(
                        "{:?}/{} 强事实加载失败: {error}",
                        row.document_type, row.document_id
                    )
                });
            assert_eq!(facts.document_type, row.document_type);
            assert_eq!(facts.document_id, row.document_id);
            assert_eq!(facts.business_object_version, 1);
            assert_eq!(facts.document_no, row.document_no);
            assert_eq!(facts.responsible_org_id, row.responsible_org_id);
            assert_eq!(facts.creator_id, row.creator_id);
        }

        assert!(
            load_facts(fixture.db(), DocumentType::VoucherSalesOrder, GOODS_ORDER_ID,)
                .await
                .is_err()
        );
        assert!(
            load_facts(fixture.db(), DocumentType::SalesOrder, VOUCHER_ORDER_ID,)
                .await
                .is_err()
        );
        assert!(load_facts(fixture.db(), DocumentType::CustomerReceipt, STOCK_ID)
            .await
            .is_err());
        assert!(load_facts(fixture.db(), DocumentType::Delivery, STOCK_ID)
            .await
            .is_err());

        let orphan = PaymentReversal::new(
            PaymentReversalId::new("upgrade-payment-reversal-orphan"),
            PaymentReversalData {
                reversal_no: "PRR-UPGRADE-ORPHAN".to_string(),
                original_supplier_payment_id: SupplierPaymentId::new("missing-payment"),
                reason_code: None,
                reason_text: "缺失原付款".to_string(),
                amount: amount(),
                handled_by: "orphan-handler".to_string(),
                reviewed_by: "orphan-reviewer".to_string(),
                occurred_at: Instant::from_unix_secs(100),
                evidence_attachment_id: None,
            },
            "creator-orphan",
        )
        .expect("孤儿付款冲正");
        fixture
            .db()
            .payment_reversals()
            .create(&orphan, &mut NoTransaction)
            .await
            .expect("写入孤儿付款冲正");
        assert!(
            load_facts(fixture.db(), DocumentType::PaymentReversal, &orphan.base.id,)
                .await
                .is_err()
        );

        let unset = fixture
            .db()
            .collection::<mongodb::bson::Document>("stock_adjustments")
            .update_one(
                mongodb::bson::doc! { "id": STOCK_ID },
                mongodb::bson::doc! { "$unset": { "created_by": "" } },
            )
            .await
            .expect("模拟旧 BSON 缺少创建人");
        assert_eq!(unset.modified_count, 1, "必须命中库存调整单的 id 字段");
        assert!(load_facts(fixture.db(), DocumentType::StockAdjustment, STOCK_ID)
            .await
            .is_err());

        let mut submitted = stock_adjustment(
            "upgrade-stock-submitted",
            WAREHOUSE_ORG,
            "creator-stock-submitted",
        );
        submitted.start_approval().expect("形成已提交强事实");
        fixture
            .db()
            .stock_adjustments()
            .create(&submitted, &mut NoTransaction)
            .await
            .expect("写入已提交库存调整");
        let submitted_facts = load_facts(fixture.db(), DocumentType::StockAdjustment, &submitted.base.id)
            .await
            .expect("Replay 授权事实不得被生命周期门禁拒绝");
        assert_eq!(submitted_facts.creator_id, "creator-stock-submitted");
        assert!(ensure_initial_unsubmitted_approval_upgrade_subject(
            fixture.db(),
            &submitted_facts,
            &mut NoTransaction,
        )
        .await
        .is_err());

        cleanup(fixture).await;
    });
}

/// 绑定升级跨集合事实；回放和失败路径必须逐文档保持不变。
#[derive(Debug, Clone, PartialEq)]
struct UpgradeFacts {
    documents: Vec<Document>,
    adjustments: Vec<Document>,
    receipts: Vec<Document>,
    actions: Vec<Document>,
    audits: Vec<Document>,
    definitions: Vec<Document>,
}

/// 构造与 Casbin subject 一致的后台账号。
fn account(id: &str) -> AccountCore {
    AccountCore::new(
        id.to_string(),
        AccountCoreData {
            secret: Secret::new(
                LoginAccount::new(format!("login-{id}")).expect("测试登录账号"),
                "test-only-password",
            )
            .expect("测试凭证"),
            name: id.to_string(),
            kind: AccountKind::Admin,
            status: AccountStatus::Active,
            email: None,
            phone: None,
            avatar: None,
        },
    )
    .expect("测试账号")
}

/// 构造认证操作人。
fn actor_for(id: &str) -> AuditActor {
    AuditActor::new(id.to_string(), format!("login-{id}"), AccountKind::Admin)
}

/// 构造 BPM 参与人。
fn participant(id: &str) -> ParticipantId {
    ParticipantId::new(id).expect("测试参与人")
}

/// 构造固定秒时间戳。
fn at(seconds: i64) -> Timestamp {
    Timestamp::from_unix_secs(seconds).expect("测试时间戳")
}

/// 构造绑定升级公开命令。
fn upgrade_command(
    document_type: DocumentType,
    document_id: &str,
    reason: &str,
    expected_document_version: u64,
    expected_approval_binding_version: u64,
    idempotency_key: &str,
) -> UpgradeBindingCommand {
    UpgradeBindingCommand {
        document_type,
        document_id: document_id.to_string(),
        reason: reason.to_string(),
        expected_document_version,
        expected_approval_binding_version,
        idempotency_key: idempotency_key.to_string(),
    }
}

/// 按 `_id` 固定顺序读取集合快照。
async fn documents(db: &Database, collection: &str) -> Vec<Document> {
    let mut cursor = db
        .collection::<Document>(collection)
        .find(doc! {})
        .sort(doc! { "_id": 1_i32 })
        .await
        .expect("读取升级命令事实");
    let mut documents = Vec::new();
    while cursor.advance().await.expect("推进升级命令事实游标") {
        documents.push(cursor.deserialize_current().expect("反序列化升级命令事实"));
    }
    documents
}

/// 读取绑定升级相关持久化快照。
async fn upgrade_facts(db: &Database) -> UpgradeFacts {
    UpgradeFacts {
        documents: documents(db, DOCUMENTS).await,
        adjustments: documents(db, ADJUSTMENTS).await,
        receipts: documents(db, RECEIPTS).await,
        actions: documents(db, ACTIONS).await,
        audits: documents(db, AUDITS).await,
        definitions: documents(db, DEFINITIONS).await,
    }
}

/// 按服务端记录顺序读取当前随机库 profiler。
async fn profiles(db: &Database) -> Vec<Document> {
    let mut cursor = db
        .collection::<Document>("system.profile")
        .find(doc! {})
        .sort(doc! { "$natural": 1_i32 })
        .await
        .expect("读取随机库 profiler");
    let mut profiles = Vec::new();
    while cursor.advance().await.expect("推进 profiler 游标") {
        profiles.push(cursor.deserialize_current().expect("反序列化 profiler"));
    }
    profiles
}

/// 读取 failCommand 累计进入次数。
async fn fail_command_entries(db: &Database) -> i64 {
    db.client()
        .database("admin")
        .run_command(doc! {
            "getParameter": 1_i32,
            "failpoint.failCommand": 1_i32,
        })
        .await
        .expect("读取 failCommand 状态")
        .get_document("failpoint.failCommand")
        .expect("failCommand 参数")
        .get_i64("timesEntered")
        .expect("failCommand timesEntered")
}

/// 在本随机库的指定集合拒绝下一条 insert。
async fn arm_insert_error(db: &Database, collection: &str, error_code: i32) -> i64 {
    let before = fail_command_entries(db).await;
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": { "times": 1_i32 },
            "data": {
                "failCommands": ["insert"],
                "namespace": format!("{}.{}", db.name(), collection),
                "errorCode": error_code,
            },
        })
        .await
        .expect("独立副本集必须启用 enableTestCommands");
    before
}

/// 在本随机库阻塞第一条 receipt insert，使另一会话先提交真实唯一键胜者。
async fn arm_first_receipt_insert_block(db: &Database) -> i64 {
    let before = fail_command_entries(db).await;
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": { "times": 1_i32 },
            "data": {
                "failCommands": ["insert"],
                "namespace": format!("{}.{}", db.name(), RECEIPTS),
                "blockConnection": true,
                "blockTimeMS": 3_000_i32,
            },
        })
        .await
        .expect("挂起首条 receipt insert");
    before
}

/// 等待本轮 failpoint 确实进入。
async fn wait_for_fail_command(db: &Database, before: i64) {
    db.client()
        .database("admin")
        .run_command(doc! {
            "waitForFailPoint": "failCommand",
            "timesEntered": before.checked_add(1).expect("failpoint 计数溢出"),
            "maxTimeMS": 10_000_i32,
        })
        .await
        .expect("目标 insert 必须进入 failpoint");
}

/// 关闭全局 failCommand。
async fn disarm_fail_command(db: &Database) {
    db.client()
        .database("admin")
        .run_command(doc! {
            "configureFailPoint": "failCommand",
            "mode": "off",
        })
        .await
        .expect("关闭 failCommand");
}

/// 证明一次性故障精确作用于本随机库目标集合的 insert。
async fn assert_insert_fail_command_target(db: &Database, collection: &str, error_code: i32) {
    let state = db
        .client()
        .database("admin")
        .run_command(doc! {
            "getParameter": 1_i32,
            "failpoint.failCommand": 1_i32,
        })
        .await
        .expect("回读 failCommand 配置");
    let data = state
        .get_document("failpoint.failCommand")
        .and_then(|failpoint| failpoint.get_document("data"))
        .expect("failCommand data");
    assert_eq!(
        data.get_i32("errorCode").expect("failCommand errorCode"),
        error_code
    );
    assert_eq!(
        data.get_array("failCommands")
            .expect("failCommand command types")
            .as_slice(),
        &[Bson::String("insert".to_string())]
    );
    assert_eq!(
        data.get_str("namespace").expect("failCommand namespace"),
        format!("{}.{}", db.name(), collection)
    );
}

/// 返回 profiler 命令所属服务端会话和事务号。
fn transaction_identity(profile: &Document) -> (Document, Bson) {
    let command = profile.get_document("command").expect("profile command");
    (
        command.get_document("lsid").expect("profile lsid").clone(),
        command.get("txnNumber").expect("profile txnNumber").clone(),
    )
}

/// 证明 receipt insert 已作为本次失败事务的第一笔物理写进入服务端。
async fn assert_receipt_insert_was_transactional(db: &Database) {
    let profiles = profiles(db).await;
    let receipt_namespace = format!("{}.{}", db.name(), RECEIPTS);
    let receipt = profiles
        .iter()
        .position(|profile| {
            profile.get_str("ns").ok() == Some(receipt_namespace.as_str())
                && profile
                    .get_document("command")
                    .and_then(|command| command.get_str("insert"))
                    .ok()
                    == Some(RECEIPTS)
                && profile.get_i32("ninserted").ok() == Some(1)
        })
        .expect("事务必须先成功执行 receipt insert");
    let command = profiles[receipt]
        .get_document("command")
        .expect("receipt profile command");
    assert_eq!(command.get_bool("autocommit").ok(), Some(false));
    let _ = transaction_identity(&profiles[receipt]);
}

/// 读取 profiler 中的收据并发失败码。
///
/// 快照事务下唯一键竞争可能表现为 `DuplicateKey(11000)` 或
/// `WriteConflict(112)`；两者都必须退出失败事务后回读。
fn concurrent_receipt_insert_error(profile: &Document) -> Option<i32> {
    let code = match profile.get("errCode") {
        Some(Bson::Int32(code)) => Some(*code),
        Some(Bson::Int64(code)) => i32::try_from(*code).ok(),
        _ => None,
    };
    if matches!(code, Some(11_000 | 112)) {
        return code;
    }
    match profile
        .get_str("errName")
        .ok()
        .or_else(|| profile.get_str("codeName").ok())
    {
        Some("DuplicateKey") => Some(11_000),
        Some("WriteConflict") => Some(112),
        _ => None,
    }
}

/// 证明真实 receipt 唯一竞争败者退出失败事务后，以不同事务回读胜者。
async fn assert_duplicate_loser_replays_in_new_transaction(db: &Database) {
    let profiles = profiles(db).await;
    let receipt_namespace = format!("{}.{}", db.name(), RECEIPTS);
    let duplicate = profiles.iter().enumerate().rev().find(|(_, profile)| {
        profile.get_str("ns").ok() == Some(receipt_namespace.as_str())
            && profile
                .get_document("command")
                .and_then(|command| command.get_str("insert"))
                .ok()
                == Some(RECEIPTS)
            && concurrent_receipt_insert_error(profile).is_some()
    });
    let Some(duplicate) = duplicate else {
        panic!("并发败者必须收到真实 receipt DuplicateKey 或 WriteConflict");
    };
    let failed_transaction = transaction_identity(duplicate.1);
    let replay = profiles
        .iter()
        .skip(duplicate.0 + 1)
        .find(|profile| {
            profile.get_str("ns").ok() == Some(receipt_namespace.as_str())
                && profile
                    .get_document("command")
                    .and_then(|command| command.get_str("find"))
                    .ok()
                    == Some(RECEIPTS)
                && profile
                    .get_document("command")
                    .and_then(|command| command.get_bool("autocommit"))
                    .ok()
                    == Some(false)
        })
        .expect("败者必须在失败 insert 后事务化回读 receipt");
    assert_ne!(
        transaction_identity(replay),
        failed_transaction,
        "失败事务不得继续用于胜者回读"
    );
}

/// 写入启用角色、账号、Casbin 授权与组织范围。
async fn seed_upgrade_authorization(db: &Database) {
    for id in [
        ADMIN_ID,
        APPROVER_ID,
        RUNTIME_ADMIN_ID,
        WRONG_ORG_ID,
        NO_READ_ID,
        DISABLED_ID,
        CREATOR_ID,
    ] {
        db.accounts()
            .create(&account(id), &mut NoTransaction)
            .await
            .expect("写入升级账号");
    }
    for role_id in [
        ROLE_ADMIN,
        ROLE_APPROVER,
        ROLE_RUNTIME,
        ROLE_WRONG_ORG,
        ROLE_NO_READ,
        ROLE_DISABLED,
    ] {
        let role = Role::new(
            role_id.to_string(),
            RoleData {
                name: role_id.to_string(),
                description: None,
                system: false,
            },
        )
        .expect("升级角色");
        db.roles()
            .create(&role, &mut NoTransaction)
            .await
            .expect("写入升级角色");
    }

    let mut adapter = MongoCasbinAdapter::new(db.clone());
    assert!(adapter
        .add_policies(
            "g",
            "g",
            vec![
                vec![
                    subject(AccountKind::Admin, ADMIN_ID),
                    format!("role:{ROLE_ADMIN}")
                ],
                vec![
                    subject(AccountKind::Admin, APPROVER_ID),
                    format!("role:{ROLE_APPROVER}"),
                ],
                vec![
                    subject(AccountKind::Admin, RUNTIME_ADMIN_ID),
                    format!("role:{ROLE_RUNTIME}"),
                ],
                vec![
                    subject(AccountKind::Admin, WRONG_ORG_ID),
                    format!("role:{ROLE_WRONG_ORG}"),
                ],
                vec![
                    subject(AccountKind::Admin, NO_READ_ID),
                    format!("role:{ROLE_NO_READ}"),
                ],
                vec![
                    subject(AccountKind::Admin, DISABLED_ID),
                    format!("role:{ROLE_DISABLED}"),
                ],
            ],
        )
        .await
        .expect("写入账号角色绑定"));
    assert!(adapter
        .add_policies(
            "p",
            "p",
            vec![
                vec![
                    format!("role:{ROLE_ADMIN}"),
                    "approval_instance".to_string(),
                    "upgrade_binding".to_string(),
                ],
                vec![
                    format!("role:{ROLE_ADMIN}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "approval_definition_admin".to_string(),
                ],
                vec![
                    format!("role:{ROLE_ADMIN}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{ROLE_APPROVER}"),
                    "approval_instance".to_string(),
                    "decide".to_string(),
                ],
                vec![
                    format!("role:{ROLE_APPROVER}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{ROLE_RUNTIME}"),
                    "approval_instance".to_string(),
                    "upgrade_binding".to_string(),
                ],
                vec![
                    format!("role:{ROLE_RUNTIME}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "approval_runtime_admin".to_string(),
                ],
                vec![
                    format!("role:{ROLE_RUNTIME}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{ROLE_WRONG_ORG}"),
                    "approval_instance".to_string(),
                    "upgrade_binding".to_string(),
                ],
                vec![
                    format!("role:{ROLE_WRONG_ORG}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "approval_definition_admin".to_string(),
                ],
                vec![
                    format!("role:{ROLE_WRONG_ORG}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "detail".to_string(),
                ],
                vec![
                    format!("role:{ROLE_NO_READ}"),
                    "approval_instance".to_string(),
                    "upgrade_binding".to_string(),
                ],
                vec![
                    format!("role:{ROLE_NO_READ}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "approval_definition_admin".to_string(),
                ],
                vec![
                    format!("role:{ROLE_DISABLED}"),
                    "approval_instance".to_string(),
                    "upgrade_binding".to_string(),
                ],
                vec![
                    format!("role:{ROLE_DISABLED}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "approval_definition_admin".to_string(),
                ],
                vec![
                    format!("role:{ROLE_DISABLED}"),
                    DocumentType::StockAdjustment.as_str().to_string(),
                    "detail".to_string(),
                ],
            ],
        )
        .await
        .expect("写入升级权限"));

    for (scope_id, role_id, organization_id) in [
        ("scope-upgrade-admin", ROLE_ADMIN, WAREHOUSE_ORG),
        ("scope-upgrade-approver", ROLE_APPROVER, WAREHOUSE_ORG),
        ("scope-upgrade-runtime", ROLE_RUNTIME, WAREHOUSE_ORG),
        ("scope-upgrade-wrong", ROLE_WRONG_ORG, WRONG_WAREHOUSE),
        ("scope-upgrade-no-read", ROLE_NO_READ, WAREHOUSE_ORG),
        ("scope-upgrade-disabled", ROLE_DISABLED, WAREHOUSE_ORG),
    ] {
        let scope = DataScope::new(
            DataScopeId::new(scope_id),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role_id.to_string(),
                scope_type: DataScopeType::Organization,
                scope_targets: vec![organization_id.to_string()],
            },
        )
        .expect("升级组织范围");
        db.data_scopes()
            .create(&scope, &mut NoTransaction)
            .await
            .expect("写入升级组织范围");
    }

    disable_role(db, ROLE_DISABLED).await;
}

/// 停用指定角色，验证残留 Casbin grant 不得授权。
async fn disable_role(db: &Database, role_id: &str) {
    let mut role = db
        .roles()
        .find_by_id(role_id, &mut NoTransaction)
        .await
        .expect("读取升级角色")
        .expect("升级角色必须存在");
    role.update(RoleUpdate {
        disabled: Some(true),
        ..RoleUpdate::default()
    })
    .expect("停用升级角色");
    db.roles()
        .update(&mut role, &mut NoTransaction)
        .await
        .expect("持久化角色停用");
}

/// 写入单节点定义；`retired` 时先发布再退役，供单据绑定旧版本。
async fn seed_definition(db: &Database, definition_id: &str, version: u32, node_key: &str, retired: bool) {
    let mut definition = ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(definition_id),
        ProcessKind::StockAdjustment,
        version,
        format!("库存调整升级验收 v{version}"),
        node_key,
        participant(ADMIN_ID),
        at(1),
    )
    .expect("审批定义");
    definition
        .publish(participant(ADMIN_ID), at(2))
        .expect("发布审批定义");
    if retired {
        definition
            .retire(participant(ADMIN_ID), at(3))
            .expect("退役旧发布定义");
    }
    db.approval_process_definitions()
        .create(&definition, &mut NoTransaction)
        .await
        .expect("写入审批定义");

    let node = ApprovalNodeDefinition::new(NewNodeDefinition {
        id: ApprovalNodeDefinitionId::new(format!("node-{definition_id}")),
        process_definition_id: ApprovalProcessDefinitionId::new(definition_id),
        node_key: node_key.to_string(),
        node_name: format!("仓储复核 {version}"),
        node_purpose: None,
        display_order: 1,
        assignee_participant_id: participant(APPROVER_ID),
        assignee_label_snapshot: "库存复核人".to_string(),
        at: at(1),
    })
    .expect("审批节点");
    db.approval_node_definitions()
        .create(&node, &mut NoTransaction)
        .await
        .expect("写入审批节点");

    for transition in [
        ApprovalTransitionDefinition::to_approved(
            ApprovalTransitionDefinitionId::new(format!("transition-{definition_id}-approve")),
            ApprovalProcessDefinitionId::new(definition_id),
            node_key,
            ApprovalTransitionEvent::Approve,
            at(1),
        )
        .expect("通过终态连线"),
        ApprovalTransitionDefinition::to_node(
            ApprovalTransitionDefinitionId::new(format!("transition-{definition_id}-reject")),
            ApprovalProcessDefinitionId::new(definition_id),
            node_key,
            ApprovalTransitionEvent::Reject,
            node_key,
            at(1),
        )
        .expect("驳回重启连线"),
    ] {
        db.approval_transition_definitions()
            .create(&transition, &mut NoTransaction)
            .await
            .expect("写入审批连线");
    }
}

/// 写入未提交库存调整及其绑定到旧发布版本的注册行。
async fn seed_unsubmitted_adjustment(db: &Database, adjustment_id: &str, started: bool) {
    let adjustment = stock_adjustment(adjustment_id, WAREHOUSE_ORG, CREATOR_ID);
    db.stock_adjustments()
        .create(&adjustment, &mut NoTransaction)
        .await
        .expect("写入库存调整单");

    let mut document = BusinessDocument::new(
        BusinessDocumentId::new(adjustment_id),
        BusinessDocumentData {
            document_type: DocumentType::StockAdjustment,
            document_no: adjustment.adjustment_no.clone(),
        },
    )
    .expect("业务单据注册");
    document
        .bind_approval_definition(
            ApprovalDefinitionBinding::new(
                ApprovalProcessDefinitionId::new(DEF_V1),
                1,
                Instant::from_unix_secs(2),
            )
            .expect("旧发布绑定"),
        )
        .expect("绑定旧发布定义");
    if started {
        document
            .mark_approval_started(Instant::from_unix_secs(4))
            .expect("标记已启动");
    }
    db.business_documents()
        .create(&document, &mut NoTransaction)
        .await
        .expect("写入业务单据注册");
}

/// 预热定义管理、升级动作和对象读取授权。
async fn warmup_upgrade_rbac(db: &Database) {
    let rbac = iam::shared_rbac_service(db.clone());
    for code in [
        "stock_adjustment:approval_definition_admin",
        "approval_instance:upgrade_binding",
        "stock_adjustment:detail",
    ] {
        let permission = Permission::parse(code).expect("升级权限常量");
        assert!(rbac
            .enforce(&subject(AccountKind::Admin, ADMIN_ID), &permission)
            .await
            .expect("预热升级授权"));
    }
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向启用 enableTestCommands 的 MongoDB 7 副本集"]
async fn upgrade_binding_is_authorized_receipt_first_and_concurrent_single_winner() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_binding_upgrade_command")
            .await
            .expect("创建随机测试库");
        ensure_indexes(fixture.db()).await.expect("创建生产索引");
        seed_upgrade_authorization(fixture.db()).await;
        seed_definition(fixture.db(), DEF_V1, 1, NODE_V1, true).await;
        seed_definition(fixture.db(), DEF_V2, 2, NODE_V2, false).await;
        seed_unsubmitted_adjustment(fixture.db(), ADJUSTMENT_ID, false).await;
        seed_unsubmitted_adjustment(fixture.db(), STARTED_ID, true).await;
        warmup_upgrade_rbac(fixture.db()).await;

        let rbac = iam::shared_rbac_service(fixture.db().clone());
        let service = Arc::new(ApprovalRuntimeService::new(fixture.db().clone(), rbac));
        let admin = actor_for(ADMIN_ID);
        let baseline = upgrade_facts(fixture.db()).await;
        assert_eq!(baseline.receipts.len(), 0);
        assert_eq!(baseline.actions.len(), 0);
        assert_eq!(baseline.definitions.len(), 2);

        let denied = [
            service
                .upgrade_binding(
                    &actor_for(RUNTIME_ADMIN_ID),
                    upgrade_command(
                        DocumentType::StockAdjustment,
                        ADJUSTMENT_ID,
                        "运行管理员越权",
                        1,
                        1,
                        "upgrade-runtime-admin",
                    ),
                )
                .await
                .expect_err("运行管理员不得替代定义管理员"),
            service
                .upgrade_binding(
                    &actor_for(WRONG_ORG_ID),
                    upgrade_command(
                        DocumentType::StockAdjustment,
                        ADJUSTMENT_ID,
                        "错误组织",
                        1,
                        1,
                        "upgrade-wrong-org",
                    ),
                )
                .await
                .expect_err("错误组织必须失败关闭"),
            service
                .upgrade_binding(
                    &actor_for(NO_READ_ID),
                    upgrade_command(
                        DocumentType::StockAdjustment,
                        ADJUSTMENT_ID,
                        "缺少对象读取",
                        1,
                        1,
                        "upgrade-no-read",
                    ),
                )
                .await
                .expect_err("缺对象读取必须失败关闭"),
            service
                .upgrade_binding(
                    &actor_for(DISABLED_ID),
                    upgrade_command(
                        DocumentType::StockAdjustment,
                        ADJUSTMENT_ID,
                        "禁用角色残留",
                        1,
                        1,
                        "upgrade-disabled",
                    ),
                )
                .await
                .expect_err("禁用角色残留不得授权"),
        ];
        for error in denied {
            assert!(matches!(error, Error::Forbidden(_)), "{error}");
        }
        assert!(service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::CustomerReceipt,
                    ADJUSTMENT_ID,
                    "类型不一致",
                    1,
                    1,
                    "upgrade-type-mismatch",
                ),
            )
            .await
            .is_err());
        assert!(service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::Delivery,
                    ADJUSTMENT_ID,
                    "无需审批",
                    1,
                    1,
                    "upgrade-no-approval",
                ),
            )
            .await
            .is_err());
        assert!(service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    STARTED_ID,
                    "已启动",
                    1,
                    1,
                    "upgrade-started",
                ),
            )
            .await
            .is_err());
        assert!(service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    ADJUSTMENT_ID,
                    "版本漂移",
                    9,
                    1,
                    "upgrade-object-version",
                ),
            )
            .await
            .is_err());
        assert!(service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    ADJUSTMENT_ID,
                    "绑定版本漂移",
                    1,
                    9,
                    "upgrade-binding-version",
                ),
            )
            .await
            .is_err());
        assert_eq!(
            upgrade_facts(fixture.db()).await,
            baseline,
            "授权、类型、已启动和版本漂移必须零写"
        );

        fixture
            .db()
            .run_command(doc! { "profile": 2_i32, "slowms": 0_i32 })
            .await
            .expect("启用随机库 profiler");
        let fault_before = arm_insert_error(fixture.db(), ACTIONS, 2).await;
        let failed = service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    ADJUSTMENT_ID,
                    "收据后回滚",
                    1,
                    1,
                    "upgrade-rollback",
                ),
            )
            .await
            .expect_err("receipt 后的动作 insert 故障必须整笔失败");
        assert!(matches!(failed, Error::RepositoryError(_)));
        assert_eq!(
            fail_command_entries(fixture.db()).await,
            fault_before + 1,
            "workflow_actions insert 故障必须精确命中一次"
        );
        assert_insert_fail_command_target(fixture.db(), ACTIONS, 2).await;
        disarm_fail_command(fixture.db()).await;
        assert_receipt_insert_was_transactional(fixture.db()).await;
        assert_eq!(
            upgrade_facts(fixture.db()).await,
            baseline,
            "receipt 后任一写失败必须把 receipt、绑定、动作和审计一起回滚"
        );

        let race_before = arm_first_receipt_insert_block(fixture.db()).await;
        let first_service = Arc::clone(&service);
        let first = tokio::spawn(async move {
            first_service
                .upgrade_binding(
                    &actor_for(ADMIN_ID),
                    upgrade_command(
                        DocumentType::StockAdjustment,
                        ADJUSTMENT_ID,
                        "  升级到当前发布定义  ",
                        1,
                        1,
                        "  upgrade-race  ",
                    ),
                )
                .await
        });
        wait_for_fail_command(fixture.db(), race_before).await;
        let winner = service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    ADJUSTMENT_ID,
                    "升级到当前发布定义",
                    1,
                    1,
                    "upgrade-race",
                ),
            )
            .await
            .expect("未阻塞会话必须提交唯一胜者");
        let recovered = first
            .await
            .expect("并发任务不得 panic")
            .expect("败者必须退出失败事务后回读胜者");
        disarm_fail_command(fixture.db()).await;
        assert_eq!(winner.outcome, UpgradeBindingOutcome::Applied);
        assert_eq!(recovered.outcome, UpgradeBindingOutcome::Replay);
        assert_eq!(recovered.document_type, winner.document_type);
        assert_eq!(recovered.document_id, winner.document_id);
        assert_eq!(
            recovered.original_business_object_version,
            winner.original_business_object_version
        );
        assert_eq!(recovered.new_binding, winner.new_binding);
        assert_eq!(recovered.action_id, winner.action_id);
        assert_eq!(winner.document_type, DocumentType::StockAdjustment);
        assert_eq!(winner.document_id, ADJUSTMENT_ID);
        assert_eq!(winner.original_business_object_version, "1");
        assert_eq!(winner.new_binding.approval_process_definition_id, DEF_V2);
        assert_eq!(winner.new_binding.approval_definition_version, 2);
        assert_eq!(winner.new_binding.approval_binding_version, "2");
        assert_duplicate_loser_replays_in_new_transaction(fixture.db()).await;

        let committed = upgrade_facts(fixture.db()).await;
        assert_eq!(committed.adjustments, baseline.adjustments);
        assert_eq!(committed.definitions, baseline.definitions);
        assert_eq!(committed.receipts.len(), 1);
        assert_eq!(committed.actions.len(), 1);
        assert_eq!(committed.audits.len(), 1);
        assert_eq!(
            committed.receipts[0].get_str("result_ref").expect("收据结果引用"),
            winner.action_id
        );
        assert_eq!(
            committed.actions[0].get_str("id").expect("不可变动作 ID"),
            winner.action_id
        );

        let replay = service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    ADJUSTMENT_ID,
                    "升级到当前发布定义",
                    1,
                    1,
                    "\tupgrade-race\n",
                ),
            )
            .await
            .expect("空白等价 key 与同规范载荷必须回放");
        assert_eq!(replay.outcome, UpgradeBindingOutcome::Replay);
        assert_eq!(replay.action_id, winner.action_id);
        assert_eq!(replay.new_binding, winner.new_binding);
        assert_eq!(
            upgrade_facts(fixture.db()).await,
            committed,
            "同载荷 replay 必须逐文档零重写"
        );

        let conflict = service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    ADJUSTMENT_ID,
                    "异载荷升级",
                    1,
                    1,
                    "upgrade-race",
                ),
            )
            .await
            .expect_err("同 scope+key 异载荷必须稳定冲突");
        assert_eq!(
            conflict.code(),
            Some(ErrorCode::ApprovalIdempotencyPayloadConflict)
        );
        assert_eq!(upgrade_facts(fixture.db()).await, committed);

        disable_role(fixture.db(), ROLE_ADMIN).await;
        let revoked = service
            .upgrade_binding(
                &admin,
                upgrade_command(
                    DocumentType::StockAdjustment,
                    ADJUSTMENT_ID,
                    "升级到当前发布定义",
                    1,
                    1,
                    "upgrade-race",
                ),
            )
            .await
            .expect_err("当前失权后不得回放已有 receipt");
        assert!(matches!(revoked, Error::Forbidden(_)));
        assert_eq!(upgrade_facts(fixture.db()).await, committed);

        fixture
            .db()
            .run_command(doc! { "profile": 0_i32 })
            .await
            .expect("关闭随机库 profiler");
        drop(service);
        cleanup(fixture).await;
    });
}
