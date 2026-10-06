//! 原变更撤回编辑时的内容、来源、稳定行键及失败关闭规则。

use std::str::FromStr;

use erp_core::common::time::Instant;
use erp_core::ids::{
    ProcurementConfirmationLineId, PurchaseChangeOrderId, PurchaseChangeSubmissionId,
    PurchaseChangeSubmissionLineId, PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId,
    SalesOrderLineId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId, SkuRevisionId,
    SupplierAccountId, SupplierCommercialProfileRevisionId,
};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice};

use super::*;
use crate::entity::purchase_order::{
    FulfillmentResponsibility, PaymentTermSnapshot, PurchaseChangeOrderData, PurchaseChangeSubmissionData,
    PurchaseChangeSubmissionLineData, PurchaseOrderRevisionData, PurchaseType, SupplierSnapshot,
};

/// 构造原采购变更草稿。
fn change() -> PurchaseChangeOrder {
    PurchaseChangeOrder::new(
        PurchaseChangeOrderId::new("change-1"),
        PurchaseChangeOrderData {
            purchase_order_id: PurchaseOrderId::new("order-1"),
            base_revision_id: PurchaseOrderRevisionId::new("revision-1"),
            reason: "成本调整".into(),
        },
        "buyer-1",
    )
    .unwrap()
}

/// 构造目标付款条件与采购变更提交。
fn submission() -> PurchaseChangeSubmission {
    PurchaseChangeSubmission::new(
        PurchaseChangeSubmissionId::new("submission-1"),
        PurchaseChangeSubmissionData {
            purchase_change_order_id: PurchaseChangeOrderId::new("change-1"),
            submission_no: "CS-000001".into(),
            base_revision_id: PurchaseOrderRevisionId::new("revision-1"),
            supplier_id: SupplierAccountId::new("supplier-1"),
            purchase_type: PurchaseType::Physical,
            fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
            supplier_revision_id: SupplierCommercialProfileRevisionId::new("supplier-rev-1"),
            supplier_snapshot: SupplierSnapshot::new("供应商".into()).unwrap(),
            payment_term_snapshot: PaymentTermSnapshot {
                payment_term_code: "NET-60".into(),
                prepay_gate: false,
                prepay_minimum_amount: None,
                prepay_minimum_ratio: None,
            },
            gross_amount: Amount::from_str("27").unwrap(),
            net_amount: Amount::from_str("27").unwrap(),
            tax_amount: Amount::zero(),
        },
    )
    .unwrap()
}

/// 构造保留全部当前和历史来源身份的目标商品行。
fn item() -> PurchaseChangeSubmissionLine {
    PurchaseChangeSubmissionLine::new(
        PurchaseChangeSubmissionLineId::new("item-1"),
        PurchaseChangeSubmissionLineData {
            purchase_change_submission_id: PurchaseChangeSubmissionId::new("submission-1"),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("confirm-1")),
            sku_id: Some(SkuId::new("sku-1")),
            sku_revision_id: Some(SkuRevisionId::new("sku-rev-1")),
            supplier_offering_source: None,
            product_name_snapshot: Some("商品".into()),
            specification_snapshot: Some("规格".into()),
            quantity: Some(Quantity::from_str("4").unwrap()),
            base_unit_code: Some("piece".into()),
            unit_cost_gross: Some(UnitPrice::from_str("5").unwrap()),
            gross_amount: Amount::from_str("20").unwrap(),
            net_amount: Amount::from_str("20").unwrap(),
            tax_amount: Amount::zero(),
            input_tax_rate: Some(Rate::from_str("0").unwrap()),
            expected_delivery_date: None,
            sales_order_line_id: Some(SalesOrderLineId::new("sales-line-1")),
            sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("sales-rev-line-1")),
            sales_order_submission_line_id: Some(SalesOrderSubmissionLineId::new("historical-line-1")),
            allocated_quantity: Some(Quantity::from_str("4").unwrap()),
        },
    )
    .unwrap()
}

/// 构造无关联ID但拥有自身稳定身份的物流费用行。
fn logistics() -> PurchaseChangeSubmissionLine {
    PurchaseChangeSubmissionLine::new(
        PurchaseChangeSubmissionLineId::new("logistics-1"),
        PurchaseChangeSubmissionLineData {
            purchase_change_submission_id: PurchaseChangeSubmissionId::new("submission-1"),
            line_no: 2,
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            supplier_offering_source: None,
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount: Amount::from_str("7").unwrap(),
            net_amount: Amount::from_str("7").unwrap(),
            tax_amount: Amount::zero(),
            input_tax_rate: Some(Rate::from_str("0").unwrap()),
            expected_delivery_date: None,
            sales_order_line_id: None,
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
        },
    )
    .unwrap()
}

/// 构造付款条件与目标提交不同的基准版本。
fn revision() -> PurchaseOrderRevision {
    let header = submission();
    let mut payment_term = header.payment_term_snapshot;
    payment_term.payment_term_code = "NET-30".into();
    PurchaseOrderRevision::new(
        PurchaseOrderRevisionId::new("revision-1"),
        PurchaseOrderRevisionData {
            purchase_order_id: PurchaseOrderId::new("order-1"),
            revision_no: 1,
            supplier_revision_id: header.supplier_revision_id,
            supplier_snapshot: header.supplier_snapshot,
            payment_term_snapshot: payment_term,
            gross_amount: header.gross_amount,
            net_amount: header.net_amount,
            tax_amount: header.tax_amount,
            effective_at: Instant::from_unix_secs(1_700_000_000),
        },
    )
    .unwrap()
}

/// 撤回保留当前冻结目标、历史关联、多行与付款条件供再次提交。
#[test]
fn withdrawn_draft_restores_last_submission_with_all_lines_and_keys() {
    let mut change = change();
    change.start_approval(PurchaseChangeSubmissionId::new("submission-1"), "target", "buyer-1").unwrap();
    change.cancel_approval("buyer-1").unwrap();
    let view =
        PurchaseChangeDraftView::from_submission(&change, &submission(), &[item(), logistics()]).unwrap();
    assert_eq!(view.version, change.base.version);
    assert_eq!(view.reason, "成本调整");
    assert_eq!(view.payment_term_code, "NET-60");
    assert_eq!(view.line_keys, ["item-1", "logistics-1"]);
    assert_eq!(view.lines.len(), 2);
    assert_eq!(view.lines[0].quantity.as_deref(), Some("4"));
    assert_eq!(view.lines[0].allocated_quantity.as_deref(), Some("4"));
    assert_eq!(view.lines[0].sku_revision_id.as_deref(), Some("sku-rev-1"));
    assert_eq!(view.lines[0].sales_order_submission_line_id.as_deref(), Some("historical-line-1"));
    assert_eq!(view.lines[0].gross_amount, None);
    assert_eq!(view.lines[1].gross_amount.as_deref(), Some("7"));
    assert_eq!(view.lines[1].sales_order_line_id, None);
    change.start_approval(PurchaseChangeSubmissionId::new("submission-2"), "edited", "buyer-1").unwrap();
    assert_eq!(change.approval_subject_version, 2);
}

/// 首次变更使用完整基准版本且物流行键仍独立稳定。
#[test]
fn initial_draft_restores_complete_base_and_original_payment_term() {
    let lines = [item(), logistics()]
        .iter()
        .map(|line| {
            PurchaseOrderRevisionLine::from_change_submission_line(
                PurchaseOrderRevisionLineId::new(format!("base-{}", line.base.id)),
                PurchaseOrderRevisionId::new("revision-1"),
                line,
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let view = PurchaseChangeDraftView::from_base(&change(), &revision(), &lines).unwrap();
    assert_eq!(view.payment_term_code, "NET-30");
    assert_eq!(view.line_keys, ["base-item-1", "base-logistics-1"]);
    assert_eq!(view.lines[0].sales_order_revision_line_id.as_deref(), Some("sales-rev-line-1"));
    assert_eq!(view.lines[0].sales_order_submission_line_id, None);
    assert_eq!(view.lines[1].gross_amount.as_deref(), Some("7"));
}

/// 冻结提交缺失、误用历史或跨来源时失败关闭，不能回退基准内容。
#[test]
fn draft_target_rejects_missing_stale_or_cross_source_facts() {
    let mut change = change();
    assert!(PurchaseChangeDraftView::from_submission(&change, &submission(), &[item()]).is_err());
    change.start_approval(PurchaseChangeSubmissionId::new("submission-1"), "target", "buyer-1").unwrap();
    assert!(PurchaseChangeDraftView::from_submission(&change, &submission(), &[item()]).is_err());
    change.cancel_approval("buyer-1").unwrap();
    assert!(PurchaseChangeDraftView::from_submission(&change, &submission(), &[]).is_err());
    let mut wrong_header = submission();
    wrong_header.purchase_change_order_id = PurchaseChangeOrderId::new("other-change");
    assert!(PurchaseChangeDraftView::from_submission(&change, &wrong_header, &[item()]).is_err());
    let mut wrong_line = item();
    wrong_line.purchase_change_submission_id = PurchaseChangeSubmissionId::new("old-submission");
    assert!(PurchaseChangeDraftView::from_submission(&change, &submission(), &[wrong_line]).is_err());
    let mut wrong_base = revision();
    wrong_base.purchase_order_id = PurchaseOrderId::new("other-order");
    assert!(PurchaseChangeDraftView::from_base(&change, &wrong_base, &[]).is_err());
}
