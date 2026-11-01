use std::str::FromStr;

use entity_core::BaseModel;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{
    FileAssetId, PurchaseOrderId, PurchaseOrderRevisionId, SalesOrderId, SalesOrderLineId,
    SalesOrderRevisionId, SupplierAccountId,
};
use erp_core::money::{Amount, Rate};
use erp_procurement::entity::facts::PaymentTermFact;
use erp_procurement::entity::purchase_order::{
    FulfillmentResponsibility, PurchaseExpenseDetail, PurchaseExpenseOccurrenceData, PurchaseOrderData,
    PurchaseType,
};

use super::*;

fn order(id: &str, sales_order: &str) -> PurchaseOrder {
    let mut order = PurchaseOrder::new(
        PurchaseOrderId::new(id),
        PurchaseOrderData {
            business_org_unit_id: "org".into(),
            purchase_no: format!("PO-{id}"),
            sales_order_id: SalesOrderId::new(sales_order),
            sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
            creation_basis_id: format!("expense:{id}"),
            supplier_id: SupplierAccountId::new("sup-1"),
            purchase_type: PurchaseType::Expense,
            payment_term_code: "POSTPAY_NET30".into(),
            fulfillment_responsibility: FulfillmentResponsibility::NotApplicable,
            owner_user_id: "buyer".into(),
            target_warehouse_id: None,
        },
        "buyer",
        |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
    )
    .unwrap();
    order.stable.current_revision_id = Some(format!("{id}-rev"));
    order
}

fn expense(id: &str) -> PurchaseExpenseDetail {
    PurchaseExpenseDetail::new(
        id.into(),
        PurchaseExpenseCategory::Logistics,
        "第三方物流".into(),
        Some(SalesOrderLineId::new("sol-1")),
    )
    .unwrap()
}

fn fee_line(order_id: &str, id: &str, gross: &str) -> PurchaseOrderRevisionLine {
    PurchaseOrderRevisionLine {
        base: BaseModel::fake(),
        purchase_order_revision_id: PurchaseOrderRevisionId::new(format!("{order_id}-rev")),
        line_no: 1,
        line_type: PurchaseLineType::LogisticsFee,
        procurement_confirmation_line_id: None,
        sku_id: None,
        sku_revision_id: None,
        supplier_offering_source: None,
        delivery_allocations: Vec::new(),
        product_name_snapshot: None,
        specification_snapshot: None,
        quantity: None,
        base_unit_code: None,
        unit_cost_gross: None,
        gross_amount: Amount::from_str(gross).unwrap(),
        net_amount: Amount::from_str(gross).unwrap(),
        tax_amount: Amount::zero(),
        input_tax_rate: Some(Rate::from_str("0").unwrap()),
        expected_delivery_date: None,
        sales_order_line_id: None,
        sales_order_revision_line_id: None,
        allocated_quantity: None,
        expense: Some(expense(id)),
    }
}

fn occurrence(order_id: &str, id: &str, gross: &str) -> PurchaseExpenseOccurrence {
    PurchaseExpenseOccurrence::new(
        format!("occ-{id}"),
        PurchaseExpenseOccurrenceData {
            purchase_order_id: PurchaseOrderId::new(order_id),
            purchase_order_revision_id: PurchaseOrderRevisionId::new(format!("{order_id}-rev")),
            sales_order_id: SalesOrderId::new("so-1"),
            supplier_id: SupplierAccountId::new("sup-1"),
            expense: expense(id),
            gross_amount: Amount::from_str(gross).unwrap(),
            net_amount: Amount::from_str(gross).unwrap(),
            tax_amount: Amount::zero(),
            input_tax_rate: Rate::from_str("0").unwrap(),
            occurred_on: BusinessDate::from_ymd(2026, 10, 1).unwrap(),
            evidence_attachment_id: FileAssetId::new("file-1"),
            confirmed_by: "buyer".into(),
        },
        BusinessDate::from_ymd(2026, 10, 14).unwrap(),
    )
    .unwrap()
}

#[test]
fn fee_lines_map_to_pending_or_occurred_and_skip_removed() {
    let facts = PurchaseExpenseFacts {
        orders: vec![order("po-1", "so-1")],
        lines: vec![
            fee_line("po-1", "a", "100.00"),
            fee_line("po-1", "b", "30.00"),
            fee_line("po-1", "c", "0.00"),
        ],
        occurrences: vec![occurrence("po-1", "b", "30.00")],
    };
    let list = SalesExtraCostListView::from_items(purchase_expense_views(&facts));
    assert_eq!(list.items.len(), 2);
    assert_eq!((list.items[0].id.as_str(), list.items[0].status), ("b", ExtraCostStatus::Occurred));
    assert_eq!(list.items[0].evidence_attachment_id.as_deref(), Some("file-1"));
    assert_eq!((list.items[1].id.as_str(), list.items[1].status), ("a", ExtraCostStatus::Pending));
    assert_eq!(list.items[1].purchase_no.as_deref(), Some("PO-po-1"));
    assert_eq!(list.items[1].sales_order_line_id.as_deref(), Some("sol-1"));
    assert_eq!(list.pending_gross_total, Amount::from_str("100.00").unwrap());
    assert_eq!(list.occurred_gross_total, Amount::from_str("30.00").unwrap());
}

#[test]
fn pending_detection_ignores_zero_and_occurred_lines() {
    let facts = PurchaseExpenseFacts {
        orders: vec![order("po-1", "so-1"), order("po-2", "so-2")],
        lines: vec![
            fee_line("po-1", "a", "10.00"),
            fee_line("po-2", "b", "0.00"),
            fee_line("po-2", "c", "5.00"),
        ],
        occurrences: vec![occurrence("po-2", "c", "5.00")],
    };
    assert_eq!(facts.sales_orders_with_pending(), HashSet::from(["so-1".to_string()]));
    assert!(PurchaseExpenseFacts::default().sales_orders_with_pending().is_empty());
}
