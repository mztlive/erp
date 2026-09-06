use crate::entity::work_item::{WorkItemPriority, WorkItemStatus, WorkItemType};

use super::query::{parse_priorities, DEFAULT_TIMEZONE};
use super::status::WORK_ITEM_TYPES;
use super::view::{handler_route, role_label};
use super::*;

fn params(scope: WorkItemScope) -> WorkItemListParams {
    WorkItemListParams {
        scope,
        family: None,
        work_item_type: None,
        status: None,
        due: None,
        priorities: None,
        q: None,
        sort: None,
        queue_context_id: None,
        current_work_item_id: None,
        timezone: Some(DEFAULT_TIMEZONE.to_string()),
        page: None,
        page_size: None,
    }
}

#[test]
fn scope_status_combinations_fail_closed() {
    let mut history = params(WorkItemScope::History);
    history.status = Some(WorkItemStatus::Open);
    assert!(history.normalized().is_err());

    let mut mine = params(WorkItemScope::Mine);
    mine.status = Some(WorkItemStatus::Completed);
    assert!(mine.normalized().is_err());
}

#[test]
fn family_and_type_must_match_registered_mapping() {
    let all = params(WorkItemScope::Mine).normalized().unwrap();
    assert_eq!(all.work_item_types, WORK_ITEM_TYPES);

    let mut query = params(WorkItemScope::Mine);
    query.family = Some(WorkItemFamily::Finance);
    query.work_item_type = Some(WorkItemType::BusinessException);
    assert!(query.normalized().is_err());
}

#[test]
fn text_search_and_focus_are_normalized_for_server_filtering() {
    let mut query = params(WorkItemScope::Mine);
    query.q = Some("  SO-1  ".to_string());
    query.current_work_item_id = Some("  wi-1  ".to_string());
    let normalized = query.normalized().unwrap();
    assert_eq!(normalized.query.as_deref(), Some("SO-1"));
    assert_eq!(normalized.current_work_item_id.as_deref(), Some("wi-1"));
}

#[test]
fn priority_codes_are_strict_and_ordered() {
    let priorities = parse_priorities(Some("1,3")).unwrap();
    assert_eq!(
        priorities,
        vec![WorkItemPriority::Urgent, WorkItemPriority::Normal]
    );
    assert!(parse_priorities(Some("urgent")).is_err());
}

#[test]
fn conflict_data_serializes_only_permission_safe_projection() {
    let hidden = WorkItemConflict::new(WorkItemConflictKind::Responsibility, None);
    let value = serde_json::to_value(&hidden).expect("conflict data should serialize");

    assert_eq!(value, serde_json::json!({ "current_work_item": null }));
    assert_eq!(hidden.kind().code(), "WORK_ITEM_RESPONSIBILITY_CONFLICT");
    assert_eq!(WorkItemConflictKind::Version.code(), "WORK_ITEM_VERSION_CONFLICT");
}

#[test]
fn w13_receivable_review_routes_use_fixed_handlers() {
    let opening = handler_route(
        WorkItemType::CardFundsReview,
        "receivable_account",
        "role-finance",
    )
    .unwrap();
    let delta = handler_route(
        WorkItemType::CardFundsDeltaReview,
        "receivable_account",
        "role-finance",
    )
    .unwrap();

    assert_eq!(opening.handler_key, "card_funds");
    assert_eq!(opening.destination_workspace_id, "W13");
    assert_eq!(delta.handler_key, "card_funds_delta");
    assert_eq!(delta.destination_workspace_id, "W13");
    assert!(opening.route_context.is_none());
    assert!(delta.route_context.is_none());
}

#[test]
fn sales_invoice_execution_routes_to_w11() {
    let route = handler_route(
        WorkItemType::SalesInvoiceExecution,
        "receivable_account",
        "role-finance",
    )
    .unwrap();

    assert_eq!(route.handler_key, "sales_invoice_execution");
    assert_eq!(route.destination_workspace_id, "W11");
    assert!(route.route_context.is_none());
}

#[test]
fn w18_route_context_uses_only_fixed_owner_role_registry() {
    let cases = [
        ("role-sales", "SALES"),
        ("role-procurement", "PROCUREMENT"),
        ("role-operations", "OPERATIONS"),
        ("role-warehouse", "WAREHOUSE"),
        ("role-finance", "FINANCE"),
    ];
    for (role, scope) in cases {
        let route = handler_route(
            WorkItemType::ImportBusinessConfirmation,
            "LEGACY_IMPORT_BATCH",
            role,
        )
        .unwrap();
        assert_eq!(
            route
                .route_context
                .and_then(|context| context.confirmation_scope)
                .as_deref(),
            Some(scope)
        );
    }
    let unknown = handler_route(
        WorkItemType::ImportBusinessConfirmation,
        "LEGACY_IMPORT_BATCH",
        "role-unregistered",
    );
    match unknown {
        Err(error) => assert!(error.to_string().contains("IMPORT_CONFIRMATION_SCOPE_UNMAPPED")),
        Ok(_) => panic!("未登记的导入确认责任角色必须失败关闭"),
    }
}

#[test]
fn retired_and_unmapped_work_items_fail_closed() {
    let mut retired_query = params(WorkItemScope::Mine);
    retired_query.work_item_type = Some(WorkItemType::PurchaseOrderReview);
    assert!(retired_query.normalized().is_err());

    let retired = handler_route(
        WorkItemType::PurchaseOrderReview,
        "purchase_order",
        "role-finance",
    );
    match retired {
        Err(error) => assert!(error.to_string().contains("WORK_ITEM_TYPE_RETIRED")),
        Ok(_) => panic!("已退役任务类型必须失败关闭"),
    }

    let wrong_card_subject = handler_route(WorkItemType::CardFundsReview, "sales_order", "role-finance");
    match wrong_card_subject {
        Err(error) => assert!(error.to_string().contains("WORK_ITEM_HANDLER_UNMAPPED")),
        Ok(_) => panic!("票款任务绑定非应收对象时必须失败关闭"),
    }

    let unknown_exception = handler_route(
        WorkItemType::BusinessException,
        "UNREGISTERED_SUBJECT",
        "role-operations",
    );
    match unknown_exception {
        Err(error) => assert!(error.to_string().contains("WORK_ITEM_HANDLER_UNMAPPED")),
        Ok(_) => panic!("异常任务缺少处理器时必须失败关闭"),
    }

    let non_approval_document = handler_route(WorkItemType::DocumentApproval, "supplier_payment", "approver");
    match non_approval_document {
        Err(error) => assert!(error.to_string().contains("APPROVAL_DOCUMENT_ROUTE_UNMAPPED")),
        Ok(_) => panic!("非审批单据不得进入单据审批任务"),
    }
}

#[test]
fn procurement_creation_maps_to_purchase_workspace_and_family() {
    let route = handler_route(
        WorkItemType::ProcurementOrderCreation,
        "sales_order",
        "role-procurement",
    )
    .unwrap();
    assert_eq!(route.handler_key, "procurement_order_creation");
    assert_eq!(route.destination_workspace_id, "W08");
    assert!(route.route_context.is_none());
    assert_eq!(
        family_of(WorkItemType::ProcurementOrderCreation),
        WorkItemFamily::Procurement
    );
    assert!(WORK_ITEM_TYPES.contains(&WorkItemType::ProcurementOrderCreation));
    assert!(handler_route(
        WorkItemType::ProcurementOrderCreation,
        "purchase_order",
        "role-procurement"
    )
    .is_err());
}

#[test]
fn customer_acceptance_maps_to_w06_fulfillment_family() {
    let route = handler_route(
        WorkItemType::CustomerAcceptanceRegistration,
        "sales_order",
        "sales_order_owner",
    )
    .unwrap();

    assert_eq!(route.handler_key, "customer_acceptance_registration");
    assert_eq!(route.destination_workspace_id, "W06");
    assert!(route.route_context.is_none());
    assert_eq!(
        family_of(WorkItemType::CustomerAcceptanceRegistration),
        WorkItemFamily::Fulfillment
    );
    assert_eq!(role_label("sales_order_owner"), "负责销售");
    assert!(WORK_ITEM_TYPES.contains(&WorkItemType::CustomerAcceptanceRegistration));
    assert!(handler_route(
        WorkItemType::CustomerAcceptanceRegistration,
        "delivery",
        "sales_order_owner"
    )
    .is_err());
}

#[test]
fn supplier_payment_execution_maps_to_w12_finance_family() {
    let route = handler_route(
        WorkItemType::SupplierPaymentExecution,
        "payable_account",
        "role-finance",
    )
    .unwrap();

    assert_eq!(route.handler_key, "supplier_payment_execution");
    assert_eq!(route.destination_workspace_id, "W12");
    assert!(route.route_context.is_none());
    assert_eq!(
        family_of(WorkItemType::SupplierPaymentExecution),
        WorkItemFamily::Finance
    );
    assert!(WORK_ITEM_TYPES.contains(&WorkItemType::SupplierPaymentExecution));
    assert!(handler_route(
        WorkItemType::SupplierPaymentExecution,
        "supplier_payment",
        "role-finance"
    )
    .is_err());
}

#[test]
fn document_approval_maps_to_signed_workspace_and_approval_family() {
    let stock = handler_route(
        WorkItemType::DocumentApproval,
        "stock_adjustment",
        "stock_adjustment_approver",
    )
    .unwrap();
    assert_eq!(stock.handler_key, "document_approval");
    assert_eq!(stock.destination_workspace_id, "W10");
    assert_eq!(
        stock
            .route_context
            .and_then(|context| context.document_type)
            .as_deref(),
        Some("stock_adjustment")
    );
    let missing = handler_route(WorkItemType::DocumentApproval, "unknown_type", "approver");
    match missing {
        Err(error) => assert!(error.to_string().contains("APPROVAL_DOCUMENT_ROUTE_UNMAPPED")),
        Ok(_) => panic!("缺少映射必须失败关闭"),
    }
    assert_eq!(
        family_of(WorkItemType::DocumentApproval),
        WorkItemFamily::Approval
    );
    assert!(WORK_ITEM_TYPES.contains(&WorkItemType::DocumentApproval));
}
