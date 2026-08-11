//! 采购提交行、版本行与汇总视图映射。

use entities::purchase_order::PurchaseOrderRevision;

use super::dto::{PurchaseOrderLineView, TotalsView};

/// 从实体构造采购版本行的视图。
///
/// # 参数
/// * `line` - 采购版本行实体
///
/// # 返回
/// 返回响应视图。
pub(super) fn revision_line_to_view(
    line: &entities::purchase_order::PurchaseOrderRevisionLine,
) -> PurchaseOrderLineView {
    PurchaseOrderLineView {
        line_id: line.base.id.clone(),
        line_no: line.line_no,
        line_type: line.line_type,
        procurement_confirmation_line_id: line
            .procurement_confirmation_line_id
            .as_ref()
            .map(ToString::to_string),
        sku_id: line.sku_id.as_ref().map(ToString::to_string),
        sku_revision_id: line.sku_revision_id.as_ref().map(ToString::to_string),
        product_name: line.product_name_snapshot.clone(),
        specification: line.specification_snapshot.clone(),
        quantity: line.quantity.map(|q| q.to_string()),
        base_unit_code: line.base_unit_code.clone(),
        unit_cost_gross: line.unit_cost_gross.map(|v| v.to_string()),
        input_tax_rate: line.input_tax_rate.map(|v| v.to_string()),
        gross_amount: line.gross_amount.to_string(),
        net_amount: line.net_amount.to_string(),
        tax_amount: line.tax_amount.to_string(),
        expected_delivery_date: line.expected_delivery_date.map(|d| d.to_string()),
        sales_order_submission_line_id: None,
        allocated_quantity: None,
    }
}

/// 从实体构造提交行视图。
///
/// # 参数
/// * `line` - 采购提交行实体
///
/// # 返回
/// 返回响应视图。
pub(super) fn submission_line_to_view(
    line: &entities::purchase_order::PurchaseOrderSubmissionLine,
) -> PurchaseOrderLineView {
    PurchaseOrderLineView {
        line_id: line.base.id.clone(),
        line_no: line.line_no,
        line_type: line.line_type,
        procurement_confirmation_line_id: line
            .procurement_confirmation_line_id
            .as_ref()
            .map(ToString::to_string),
        sku_id: line.sku_id.as_ref().map(ToString::to_string),
        sku_revision_id: line.sku_revision_id.as_ref().map(ToString::to_string),
        product_name: line.product_name_snapshot.clone(),
        specification: line.specification_snapshot.clone(),
        quantity: line.quantity.map(|q| q.to_string()),
        base_unit_code: line.base_unit_code.clone(),
        unit_cost_gross: line.unit_cost_gross.map(|v| v.to_string()),
        input_tax_rate: line.input_tax_rate.map(|v| v.to_string()),
        gross_amount: line.gross_amount.to_string(),
        net_amount: line.net_amount.to_string(),
        tax_amount: line.tax_amount.to_string(),
        expected_delivery_date: line.expected_delivery_date.map(|d| d.to_string()),
        sales_order_submission_line_id: line
            .sales_order_submission_line_id
            .as_ref()
            .map(ToString::to_string),
        allocated_quantity: line.allocated_quantity.map(|q| q.to_string()),
    }
}

/// 从实体构造采购版本汇总。
///
/// # 参数
/// * `revision` - 采购版本实体
///
/// # 返回
/// 返回汇总视图。
pub(super) fn revision_totals(revision: &PurchaseOrderRevision) -> TotalsView {
    TotalsView {
        gross: revision.gross_amount.to_string(),
        net: revision.net_amount.to_string(),
        tax: revision.tax_amount.to_string(),
    }
}
