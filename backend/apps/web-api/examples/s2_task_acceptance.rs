//! S2 任务层验收（隔离库）：A23 改派候选双向对照、A24 采购级联原子性、
//! A25 非审批阻塞视图与统计排除。
//! 仅允许显式 ERP_TEST_MONGO_URI（隔离副本集），随机库用后删除；不用 hint，不用内存解释器做通过依据。
use std::error::Error;
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{
    DataScopeId, ElectronicDeliveryId, FileAssetId, PurchaseLineSalesAllocationId, PurchaseOrderId,
    SalesOrderId, SalesOrderLineId, SalesOrderRevisionId, ServiceFulfillmentId, SupplierAccountId,
    WarehouseId,
};
use erp_core::money::Quantity;
use erp_identity::access_control::{
    DataScope, DataScopeData, DataScopeSubjectType, DataScopeType, ScopeBinding, ScopeDimension,
    ScopeTargetMode,
};
use erp_identity::entity::organization::OrgUnitKind;
use erp_identity::entity::organization_change::{OrganizationChangeRequest, OrganizationOperation};
use erp_processes::adapters::identity::shared_rbac_service;
use erp_processes::adapters::organization_service;
use erp_processes::adapters::workflow::{work_item_service, workflow_auth};
use erp_workflow::dto::work_item::{
    ProcessingState, ReassignWorkItemRequest, WorkItemAllowedAction, WorkItemMutationOutcome, WorkItemScope,
    WorkItemSort,
};
use mongodb::bson::{Document, doc};
use mongodb::{Client, Database};
use persistence_core::NoTransaction;
use test_support::seed_admin_account;

type Outcome<T = ()> = Result<T, Box<dyn Error>>;

#[tokio::main(flavor = "current_thread")]
async fn main() -> Outcome {
    let uri = std::env::var("ERP_TEST_MONGO_URI")?;
    let client = Client::with_uri_str(uri).await?;
    let suffix = SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos();
    let db = client.database(&format!("erp_s2_task_{suffix}"));
    let outcome = verify(&db).await;
    db.drop().await?;
    outcome
}

async fn verify(db: &Database) -> Outcome {
    erp_identity::indexes::ensure(db).await?;
    erp_workflow::indexes::ensure(db).await?;

    let manager_id = seed_admin_account(db).await?;
    let good_id = seed_admin_account(db).await?;
    let partial_id = seed_admin_account(db).await?;
    let manager = AuditActor::new(manager_id.clone(), "s2t-manager".into(), AccountKind::Admin);
    let good = AuditActor::new(good_id.clone(), "s2t-good".into(), AccountKind::Admin);

    // manager：完整管理＋读写；good（跨部门）：完整执行；partial（同部门）：缺 customer_acceptance:post 与 service 确认。
    let manager_role = grant(
        db,
        &manager_id,
        &[
            ("org_unit", "list"),
            ("org_unit", "manage"),
            ("work_item", "list"),
            ("work_item", "manage"),
            ("work_item", "reassign"),
            ("sales_order", "detail"),
            ("purchase_order", "detail"),
            ("electronic_delivery", "list"),
            ("electronic_delivery", "confirm"),
            ("service_fulfillment", "list"),
            ("service_fulfillment", "confirm"),
            ("customer_acceptance", "list"),
            ("customer_acceptance", "detail"),
            ("customer_acceptance", "create"),
            ("customer_acceptance", "post"),
        ],
    )
    .await?;
    let good_role = grant(
        db,
        &good_id,
        &[
            ("sales_order", "detail"),
            ("purchase_order", "detail"),
            ("electronic_delivery", "list"),
            ("electronic_delivery", "confirm"),
            ("service_fulfillment", "list"),
            ("service_fulfillment", "confirm"),
            ("customer_acceptance", "list"),
            ("customer_acceptance", "detail"),
            ("customer_acceptance", "create"),
            ("customer_acceptance", "post"),
        ],
    )
    .await?;
    let partial_role = grant(
        db,
        &partial_id,
        &[
            ("sales_order", "detail"),
            ("purchase_order", "detail"),
            ("electronic_delivery", "list"),
            ("electronic_delivery", "confirm"),
            ("customer_acceptance", "list"),
            ("customer_acceptance", "detail"),
            ("customer_acceptance", "create"),
        ],
    )
    .await?;
    seed_company(db, &manager_role, "org_unit", &["list", "manage"]).await?;

    let org = organization_service(db.clone(), shared_rbac_service(db.clone()));
    let dept_a = create_unit(&org, &manager, "s2t-dept-a").await?;
    let dept_b = create_unit(&org, &manager, "s2t-dept-b").await?;
    transfer(&org, &manager, &manager_id, &dept_a).await?;
    sleep().await?;
    // partial 与 manager 同部门，good 跨部门：部门相同与否不得代替资格校验。
    transfer(&org, &manager, &partial_id, &dept_a).await?;
    sleep().await?;
    transfer(&org, &manager, &good_id, &dept_b).await?;
    sleep().await?;

    // 范围：Company 覆盖订单读取；work_item:manage 用 Company 以便 manager 可管理两部门任务。
    for (role, resource, actions) in [
        (manager_role.as_str(), "sales_order", &["detail"][..]),
        (manager_role.as_str(), "purchase_order", &["detail"][..]),
        (manager_role.as_str(), "work_item", &["manage"][..]),
        (good_role.as_str(), "sales_order", &["detail"][..]),
        (good_role.as_str(), "purchase_order", &["detail"][..]),
        (partial_role.as_str(), "sales_order", &["detail"][..]),
        (partial_role.as_str(), "purchase_order", &["detail"][..]),
    ] {
        seed_company(db, role, resource, actions).await?;
    }

    // T1/A23 用销售验收任务（owner=manager）。
    let sales_order_id = "s2t-order-1".to_string();
    seed_sales_order(db, &sales_order_id, &manager_id, &dept_a).await?;
    let accept_task_id = "s2t-accept-1".to_string();
    seed_acceptance_task(db, &accept_task_id, &sales_order_id, &manager_id).await?;

    let svc = work_item_service(db.clone(), shared_rbac_service(db.clone()));
    // T1 正向：跨部门 good 在候选内，同部门 partial（缺 post）不在候选内，当前 owner 不在候选内。
    let candidates = svc.clone().reassign_candidates(accept_task_id.clone(), manager.clone()).await?;
    assert!(
        candidates.iter().any(|c| c.user_id == good_id),
        "cross-dept qualified must be candidate, got {:?}",
        candidates.iter().map(|c| &c.user_id).collect::<Vec<_>>()
    );
    assert!(
        !candidates.iter().any(|c| c.user_id == partial_id),
        "same-dept unqualified must not be candidate"
    );
    assert!(!candidates.iter().any(|c| c.user_id == manager_id), "current owner must be skipped");

    // T1 反向：向 partial 改派必须整体拒绝且零写入。
    let before = find_task(db, &accept_task_id).await?;
    let before_version = before.get_i64("version").unwrap_or(1);
    let err = svc
        .clone()
        .reassign(
            accept_task_id.clone(),
            ReassignWorkItemRequest {
                expected_task_version: before_version.to_string(),
                target_user_id: partial_id.clone(),
                reason: "t1 negative".into(),
                idempotency_key: "s2t-t1-neg-1".into(),
            },
            manager.clone(),
        )
        .await
        .expect_err("unqualified reassign must fail");
    assert!(matches!(err, erp_workflow::Error::Forbidden(_)), "unqualified must be Forbidden, got {err:?}");
    let after_neg = find_task(db, &accept_task_id).await?;
    assert_eq!(after_neg.get_str("owner_user_id")?, manager_id);
    assert_eq!(after_neg.get_i64("version").unwrap_or(1), before_version);
    println!("PASS t1_candidate_bidirectional");

    // T1 正向提交：good 可接收。
    let out = svc
        .clone()
        .reassign(
            accept_task_id.clone(),
            ReassignWorkItemRequest {
                expected_task_version: before_version.to_string(),
                target_user_id: good_id.clone(),
                reason: "t1 positive".into(),
                idempotency_key: "s2t-t1-pos-1".into(),
            },
            manager.clone(),
        )
        .await?;
    assert!(matches!(out, WorkItemMutationOutcome::Applied { .. }), "qualified must apply, got {out:?}");
    let after_pos = find_task(db, &accept_task_id).await?;
    assert_eq!(after_pos.get_str("owner_user_id")?, good_id);
    assert!(after_pos.get_i64("version").unwrap_or(0) > before_version);

    // T2/A24：同一 purchase_order 下 2 开放履约＋1 已完成＋1 仓库键。
    let po_id = "s2t-po-1".to_string();
    seed_purchase_order(db, &po_id, &manager_id, &dept_a, &sales_order_id).await?;
    seed_electronic_delivery(db, "s2t-ed-1", &po_id).await?;
    seed_electronic_delivery(db, "s2t-ed-3", &po_id).await?;
    seed_service_fulfillment(db, "s2t-sf-1", &po_id).await?;
    let t1 = "s2t-po-task-1".to_string();
    let t2 = "s2t-po-task-2".to_string();
    seed_fulfillment_task(db, &t1, "electronic_delivery", "s2t-ed-1", &manager_id, &dept_a, &po_id).await?;
    seed_fulfillment_task(db, &t2, "service_fulfillment", "s2t-sf-1", &manager_id, &dept_a, &po_id).await?;
    // 已完成同键任务：完成后直接写入终态，不参与级联（用独立对象避免开放唯一键冲突）。
    let t_done = "s2t-po-task-done".to_string();
    seed_fulfillment_task(db, &t_done, "electronic_delivery", "s2t-ed-3", &manager_id, &dept_a, &po_id)
        .await?;
    db.collection::<Document>("work_items")
        .update_one(doc! {"id": &t_done}, doc! {"$set": {"status": "COMPLETED"}})
        .await?;
    // 仓库键任务：不得纳入采购单责任级联。
    let t_wh = "s2t-po-task-wh".to_string();
    seed_warehouse_task(db, &t_wh, &manager_id).await?;

    // 预检：同一责任键开放集合恰为 2 条。
    let selected: erp_workflow::entity::work_item::WorkItem =
        db.collection("work_items").find_one(doc! {"id": &t1}).await?.ok_or("selected task missing")?;
    let (owner, tasks) =
        svc.facts.purchase_order_fulfillment_scope(&selected, &po_id, &mut NoTransaction).await?;
    assert_eq!(owner, manager_id);
    assert_eq!(tasks.len(), 2, "only two open po tasks expected, got {}", tasks.len());
    assert!(tasks.iter().any(|t| t.base.id == t1) && tasks.iter().any(|t| t.base.id == t2));

    // 候选对照：good 可执行全部，partial 缺 service 确认故排除。
    let cands = svc.clone().reassign_candidates(t1.clone(), manager.clone()).await?;
    assert!(cands.iter().any(|c| c.user_id == good_id), "good must be cascade candidate");
    assert!(!cands.iter().any(|c| c.user_id == partial_id), "partial must be excluded from cascade");

    // 反向：partial 提交整体拒绝，采购单与全部任务零部分变更。
    let v1 = find_task(db, &t1).await?.get_i64("version").unwrap_or(1);
    let v2 = find_task(db, &t2).await?.get_i64("version").unwrap_or(1);
    let po_before = find_doc(db, "purchase_orders", &po_id).await?;
    let err = svc
        .clone()
        .reassign(
            t1.clone(),
            ReassignWorkItemRequest {
                expected_task_version: v1.to_string(),
                target_user_id: partial_id.clone(),
                reason: "t2 negative".into(),
                idempotency_key: "s2t-t2-neg-1".into(),
            },
            manager.clone(),
        )
        .await
        .expect_err("partial cascade must fail");
    assert!(matches!(err, erp_workflow::Error::Forbidden(_)), "partial must be Forbidden, got {err:?}");
    assert_eq!(find_task(db, &t1).await?.get_str("owner_user_id")?, manager_id);
    assert_eq!(find_task(db, &t2).await?.get_str("owner_user_id")?, manager_id);
    assert_eq!(find_task(db, &t1).await?.get_i64("version").unwrap_or(0), v1);
    assert_eq!(find_task(db, &t2).await?.get_i64("version").unwrap_or(0), v2);
    assert_eq!(
        find_doc(db, "purchase_orders", &po_id).await?.get_str("owner_user_id")?,
        po_before.get_str("owner_user_id")?
    );

    // 正向：good 同一事务原子级联采购单＋全部开放任务，历史与仓库键不变。
    let out = svc
        .clone()
        .reassign(
            t1.clone(),
            ReassignWorkItemRequest {
                expected_task_version: v1.to_string(),
                target_user_id: good_id.clone(),
                reason: "t2 positive".into(),
                idempotency_key: "s2t-t2-pos-1".into(),
            },
            manager.clone(),
        )
        .await?;
    assert!(matches!(out, WorkItemMutationOutcome::Applied { .. }), "good cascade must apply");
    assert_eq!(find_task(db, &t1).await?.get_str("owner_user_id")?, good_id);
    assert_eq!(find_task(db, &t2).await?.get_str("owner_user_id")?, good_id);
    assert!(find_task(db, &t1).await?.get_i64("version").unwrap_or(0) > v1);
    assert!(find_task(db, &t2).await?.get_i64("version").unwrap_or(0) > v2);
    assert_eq!(find_doc(db, "purchase_orders", &po_id).await?.get_str("owner_user_id")?, good_id);
    assert_eq!(find_task(db, &t_done).await?.get_str("status")?, "COMPLETED");
    assert_eq!(find_task(db, &t_done).await?.get_str("owner_user_id")?, manager_id);
    assert_eq!(find_task(db, &t_wh).await?.get_str("owner_user_id")?, manager_id);
    println!("PASS t2_po_cascade_atomic");

    // T3/A25：撤销 good 的销售读取后失格不自动改派，管理视图阻塞＋统计排除。
    // accept 任务当前 owner=good；软删其 sales_order 范围模拟失读。
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "subject_id": &good_role, "resource": "sales_order"},
            doc! {"$set": {"deleted_at": 1_i64}},
        )
        .await?;
    let read = erp_read_models::workbench::WorkbenchReadService::new(
        db.clone(),
        workflow_auth(db.clone(), shared_rbac_service(db.clone())),
    );
    use erp_read_models::workbench::{WorkItemListParams, WorkItemStatsParams};
    let snap_before = find_task(db, &accept_task_id).await?;
    let page = read
        .work_item_list(
            WorkItemListParams {
                scope: WorkItemScope::Managed,
                family: None,
                work_item_type: None,
                status: None,
                due: None,
                priorities: None,
                q: None,
                sort: Some(WorkItemSort::CreatedDesc),
                queue_context_id: None,
                scope_version: None,
                current_work_item_id: None,
                timezone: Some("Asia/Shanghai".into()),
                page: Some(1),
                page_size: Some(20),
            },
            manager.clone(),
        )
        .await?;
    let view = page.items.iter().find(|i| i.id == accept_task_id).ok_or("blocked task must stay visible")?;
    assert_eq!(view.processing_state, ProcessingState::ExecutionBlocked);
    assert_eq!(view.owner_user_id.as_deref(), Some(good_id.as_str()));
    assert!(
        view.processing_blocker.as_ref().is_some_and(|b| b.code == "WORK_ITEM_OWNER_INELIGIBLE"),
        "blocker must be ineligible, got {:?}",
        view.processing_blocker
    );
    assert!(!view.allowed_actions.contains(&WorkItemAllowedAction::Process));
    assert!(!view.allowed_actions.contains(&WorkItemAllowedAction::Approve));
    assert!(view.allowed_actions.contains(&WorkItemAllowedAction::Reassign));
    let stats = read
        .work_item_stats(
            WorkItemStatsParams {
                scope: WorkItemScope::Mine,
                family: None,
                work_item_type: Some(
                    erp_workflow::entity::work_item::WorkItemType::CustomerAcceptanceRegistration,
                ),
                due: None,
                timezone: None,
            },
            good.clone(),
        )
        .await?;
    assert_eq!(stats.assigned, 0, "blocked task must be excluded from processable stats");
    // 零变更：列表＋统计前后责任与版本不变。
    let snap_after = find_task(db, &accept_task_id).await?;
    assert_eq!(snap_after.get_str("owner_user_id")?, snap_before.get_str("owner_user_id")?);
    assert_eq!(snap_after.get_i64("version").unwrap_or(0), snap_before.get_i64("version").unwrap_or(0));
    // 恢复对照：撤销软删后同一任务回到 Ready 且统计重新计入。
    db.collection::<Document>("data_scopes")
        .update_many(
            doc! {"subject_type": "role", "subject_id": &good_role, "resource": "sales_order"},
            doc! {"$set": {"deleted_at": 0_i64}},
        )
        .await?;
    let page2 = read
        .work_item_list(
            WorkItemListParams {
                scope: WorkItemScope::Managed,
                family: None,
                work_item_type: None,
                status: None,
                due: None,
                priorities: None,
                q: None,
                sort: Some(WorkItemSort::CreatedDesc),
                queue_context_id: None,
                scope_version: None,
                current_work_item_id: None,
                timezone: Some("Asia/Shanghai".into()),
                page: Some(1),
                page_size: Some(20),
            },
            manager.clone(),
        )
        .await?;
    let view2 = page2.items.iter().find(|i| i.id == accept_task_id).ok_or("restored task missing")?;
    assert_eq!(view2.processing_state, ProcessingState::Ready);
    let stats2 = read
        .work_item_stats(
            WorkItemStatsParams {
                scope: WorkItemScope::Mine,
                family: None,
                work_item_type: Some(
                    erp_workflow::entity::work_item::WorkItemType::CustomerAcceptanceRegistration,
                ),
                due: None,
                timezone: None,
            },
            good.clone(),
        )
        .await?;
    assert_eq!(stats2.assigned, 1, "restored task must be countable");
    println!("PASS t3_blocked_view_and_stats_excluded");

    println!(
        "LIMIT synthetic accounts and scopes in an isolated random database; existing business accounts and browser acceptance remain separate"
    );
    Ok(())
}

async fn find_task(db: &Database, id: &str) -> Outcome<Document> {
    db.collection::<Document>("work_items")
        .find_one(doc! {"id": id})
        .await?
        .ok_or_else(|| format!("work item {id} missing").into())
}

async fn find_doc(db: &Database, collection: &str, id: &str) -> Outcome<Document> {
    db.collection::<Document>(collection)
        .find_one(doc! {"id": id})
        .await?
        .ok_or_else(|| format!("{collection} {id} missing").into())
}

async fn seed_sales_order(db: &Database, id: &str, owner: &str, org: &str) -> Outcome {
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use erp_sales::entity::sales_order::{BusinessType, OriginSystem, SalesOrder, SalesOrderData};
    let order = SalesOrder::new(
        SalesOrderId::new(id),
        SalesOrderData {
            sales_owner_user_id: owner.into(),
            business_org_unit_id: org.into(),
            order_no: format!("S2T-{id}"),
            business_type: BusinessType::GoodsService,
            origin_system: OriginSystem::Erp,
            source_identity_id: None,
            customer_id: CustomerAccountId::new("s2t-customer"),
            contract_id: None,
            settlement_party_id: PartyId::new("s2t-party"),
            source_status_code: None,
        },
        owner,
    )?;
    db.collection::<SalesOrder>("sales_orders").insert_one(order).await?;
    Ok(())
}

async fn seed_acceptance_task(db: &Database, task_id: &str, order_id: &str, owner: &str) -> Outcome {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };
    let task = WorkItem::new_with_responsibility_key(
        WorkItemId::new(task_id),
        WorkItemData {
            work_item_type: WorkItemType::CustomerAcceptanceRegistration,
            business_object_type: "sales_order".into(),
            business_object_id: order_id.into(),
            subject_version: "1".into(),
            owner_role: "sales_order_owner".into(),
            owner_organization_id: "s2t-party".into(),
            owner_user_id: owner.into(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".into()),
            impact_summary: None,
        },
        format!("sales_order:{order_id}:customer_acceptance"),
    )?;
    db.collection::<WorkItem>("work_items").insert_one(task).await?;
    Ok(())
}

async fn seed_purchase_order(
    db: &Database,
    id: &str,
    owner: &str,
    org: &str,
    sales_order_id: &str,
) -> Outcome {
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseType,
    };
    let order = PurchaseOrder::new(
        PurchaseOrderId::new(id),
        PurchaseOrderData {
            business_org_unit_id: org.into(),
            purchase_no: format!("S2T-{id}"),
            sales_order_id: SalesOrderId::new(sales_order_id),
            sales_order_revision_id: SalesOrderRevisionId::new("s2t-sor-1"),
            creation_basis_id: "s2t-basis-1".into(),
            supplier_id: SupplierAccountId::new("s2t-supplier"),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "POSTPAY_NET30".into(),
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            owner_user_id: owner.into(),
            target_warehouse_id: Some(WarehouseId::new("s2t-wh-1")),
        },
        owner,
        |code| {
            Ok(PaymentTermFact {
                canonical_code: code.into(),
                prepay_gate: false,
                prepay_minimum_ratio: None,
                days_after_delivery: Some(30),
                calendar_due: None,
            })
        },
    )?;
    db.collection::<PurchaseOrder>("purchase_orders").insert_one(order).await?;
    Ok(())
}

async fn seed_electronic_delivery(db: &Database, id: &str, po_id: &str) -> Outcome {
    use erp_fulfillment::entity::fulfillment::{
        ElectronicDelivery, ElectronicDeliveryData, FulfillmentResult,
    };
    let plaintext = "收货人 李四 13812345678 电子邮箱 lisi@example.com";
    let delivery = ElectronicDelivery::new(
        ElectronicDeliveryId::new(id),
        ElectronicDeliveryData {
            fulfillment_no: format!("S2T-{id}"),
            sales_order_line_id: SalesOrderLineId::new("s2t-so-line-1"),
            purchase_order_id: PurchaseOrderId::new(po_id),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("s2t-pla-1"),
            recipient_snapshot: "ciphertext-recipient...".into(),
            recipient_snapshot_fingerprint: ElectronicDelivery::recipient_snapshot_fingerprint(
                plaintext,
                b"test-fingerprint-key",
            ),
            quantity: Quantity::from_str("2")?,
            result: FulfillmentResult::Success,
            evidence_attachment_id: Some(FileAssetId::new("s2t-file-1")),
            fact_no: format!("S2T-F-{id}"),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: "s2t-operator".into(),
            source_type: SourceType::Erp,
            source_reference: Some("s2t-msg-1".into()),
            reason_code: None,
            reason_text: None,
        },
    )?;
    db.collection::<ElectronicDelivery>("electronic_deliveries").insert_one(delivery).await?;
    Ok(())
}

async fn seed_service_fulfillment(db: &Database, id: &str, po_id: &str) -> Outcome {
    use erp_fulfillment::entity::fulfillment::{
        FulfillmentResult, ServiceFulfillment, ServiceFulfillmentData,
    };
    let fulfillment = ServiceFulfillment::new(
        ServiceFulfillmentId::new(id),
        ServiceFulfillmentData {
            fulfillment_no: format!("S2T-{id}"),
            sales_order_line_id: SalesOrderLineId::new("s2t-so-line-1"),
            purchase_order_id: PurchaseOrderId::new(po_id),
            purchase_line_sales_allocation_id: PurchaseLineSalesAllocationId::new("s2t-pla-1"),
            recipient_snapshot: "ciphertext-recipient...".into(),
            recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                "收货人 王五 13912345678",
                b"test-fingerprint-key",
            ),
            quantity: Quantity::from_str("1")?,
            result: FulfillmentResult::Success,
            evidence_attachment_id: Some(FileAssetId::new("s2t-file-1")),
            service_location_encrypted: "ciphertext-location...".into(),
            service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                "上海市徐汇区漕河泾开发区xx大厦 3F 会议室A",
                b"test-fingerprint-key",
            ),
            service_started_at: Some(Instant::from_unix_secs(1_700_000_000)),
            service_ended_at: Some(Instant::from_unix_secs(1_700_003_600)),
            completion_note: Some("上门安装调试完成".into()),
            fact_no: format!("S2T-F-{id}"),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: "s2t-operator".into(),
            source_type: SourceType::ManualImport,
            source_reference: None,
            reason_code: None,
            reason_text: None,
        },
    )?;
    db.collection::<ServiceFulfillment>("service_fulfillments").insert_one(fulfillment).await?;
    Ok(())
}

async fn seed_fulfillment_task(
    db: &Database,
    task_id: &str,
    object_type: &str,
    object_id: &str,
    owner: &str,
    org: &str,
    po_id: &str,
) -> Outcome {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };
    let (owner_role, reason) = match object_type {
        "electronic_delivery" => ("purchase_order_owner", "ELECTRONIC_DELIVERY_READY"),
        "service_fulfillment" => ("purchase_order_owner", "SERVICE_FULFILLMENT_READY"),
        _ => return Err("unsupported fulfillment object".into()),
    };
    let task = WorkItem::new_with_responsibility_key(
        WorkItemId::new(task_id),
        WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: object_type.into(),
            business_object_id: object_id.into(),
            subject_version: "1".into(),
            owner_role: owner_role.into(),
            owner_organization_id: org.into(),
            owner_user_id: owner.into(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some(reason.into()),
            impact_summary: None,
        },
        format!("purchase_order:{po_id}"),
    )?;
    db.collection::<WorkItem>("work_items").insert_one(task).await?;
    Ok(())
}

async fn seed_warehouse_task(db: &Database, task_id: &str, owner: &str) -> Outcome {
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{
        AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
    };
    let task = WorkItem::new_with_responsibility_key(
        WorkItemId::new(task_id),
        WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: "purchase_receipt".into(),
            business_object_id: "s2t-receipt-1".into(),
            subject_version: "1".into(),
            owner_role: "warehouse_inbound_handler".into(),
            owner_organization_id: "s2t-wh-1".into(),
            owner_user_id: owner.into(),
            assignment_source: AssignmentSource::SystemRule,
            priority: WorkItemPriority::Normal,
            due_at: None,
            reason_code: Some("PURCHASE_RECEIPT_READY".into()),
            impact_summary: None,
        },
        "warehouse:s2t-wh-1:receipt".to_string(),
    )?;
    db.collection::<WorkItem>("work_items").insert_one(task).await?;
    Ok(())
}

async fn grant(db: &Database, user: &str, grants: &[(&str, &str)]) -> Outcome<String> {
    let rule = db
        .collection::<Document>("casbin_rules")
        .find_one(doc! {"ptype": "g", "values.0": format!("user:admin:{user}")})
        .await?
        .ok_or("role missing")?;
    let key = rule.get_array("values")?[1].as_str().ok_or("invalid role")?.to_string();
    for (resource, action) in grants {
        db.collection::<Document>("casbin_rules")
            .insert_one(doc! {"_id": format!("p\u{1f}p\u{1f}{key}\u{1f}{resource}\u{1f}{action}"), "sec": "p", "ptype": "p", "values": [key.clone(), resource, action]})
            .await?;
    }
    Ok(key.strip_prefix("role:").ok_or("invalid role prefix")?.to_string())
}

async fn seed_company(db: &Database, role: &str, resource: &str, actions: &[&str]) -> Outcome {
    shared_rbac_service(db.clone())
        .seed_data_scope_manifest(
            role,
            resource,
            vec![DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Company,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: actions.iter().map(|action| (*action).to_string()).collect(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    enabled: true,
                },
            }],
        )
        .await?;
    if db
        .collection::<Document>("data_scopes")
        .count_documents(doc! {"subject_id": role, "resource": resource, "deleted_at": 0_i64})
        .await?
        == 0
    {
        let scope = DataScope::new(
            DataScopeId::new(format!("s2t:{role}:{resource}:company")),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role.into(),
                scope_type: DataScopeType::Company,
                scope_targets: vec![],
                binding: ScopeBinding {
                    schema_version: 2,
                    resource: resource.into(),
                    actions: actions.iter().map(|action| (*action).to_string()).collect(),
                    target_dimension: ScopeDimension::InternalOrg,
                    target_mode: None,
                    include_descendants: None,
                    enabled: true,
                },
            },
        )?;
        db.collection::<DataScope>("data_scopes").insert_one(scope).await?;
    }
    Ok(())
}

async fn create_unit(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
    name: &str,
) -> Outcome<String> {
    let version = org.state(actor).await?.version;
    let receipt = org
        .change(
            actor,
            OrganizationChangeRequest {
                expected_version: version,
                idempotency_key: format!("s2t-{name}"),
                reason: "S2 task isolated acceptance".into(),
                change: OrganizationOperation::CreateUnit {
                    name: name.into(),
                    parent_id: None,
                    kind: OrgUnitKind::Department,
                },
            },
        )
        .await?;
    receipt
        .after
        .units
        .into_iter()
        .find(|unit| unit.name == name)
        .map(|unit| unit.base.id.to_string())
        .ok_or_else(|| "created unit missing".into())
}

async fn transfer(
    org: &erp_identity::service::organization::OrganizationService,
    actor: &AuditActor,
    user_id: &str,
    org_unit_id: &str,
) -> Outcome {
    let version = org.state(actor).await?.version;
    org.change(
        actor,
        OrganizationChangeRequest {
            expected_version: version,
            idempotency_key: format!("s2t-transfer-{user_id}-{org_unit_id}-{version}"),
            reason: "S2 task isolated acceptance".into(),
            change: OrganizationOperation::TransferMember {
                user_id: user_id.into(),
                org_unit_id: org_unit_id.into(),
            },
        },
    )
    .await?;
    Ok(())
}

async fn sleep() -> Outcome {
    tokio::time::sleep(std::time::Duration::from_millis(1200)).await;
    Ok(())
}

#[allow(dead_code)]
fn _use_scope_target_mode() {
    let _ = ScopeTargetMode::Explicit;
}
