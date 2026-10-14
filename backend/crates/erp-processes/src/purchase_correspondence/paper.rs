//! 供应商采购单：仅输出采购内容，绝不包含来源销售报价或内部审批资料。
use std::str::FromStr;

use erp_procurement::dto::purchase_order::PurchaseOrderLineView;
use erp_procurement::entity::purchase_order::{FulfillmentResponsibility, PurchaseLineType};
use erp_read_models::purchase_center::dto::PurchaseOrderCenterView;
use erp_supplier::entity::supplier::SupplierPaymentTerm;
use rust_decimal::Decimal;
use serde::Serialize;

/// 可保存并打印的独立 HTML 文件。
#[derive(Serialize)]
pub struct PurchasePaper {
    pub filename: String,
    pub html: String,
}

pub(super) fn render(order: &PurchaseOrderCenterView) -> PurchasePaper {
    let mut html = format!(
        r#"<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>采购单 {}</title><style>body{{font:14px/1.6 sans-serif;color:#17212b;max-width:1000px;margin:32px auto;padding:24px}}h1{{text-align:center}}table{{width:100%;border-collapse:collapse;margin:24px 0}}th,td{{padding:9px;border:1px solid #aaa;text-align:left}}.num{{text-align:right}}@page{{size:A4 landscape;margin:15mm}}tr{{break-inside:avoid}}thead{{display:table-header-group}}</style></head><body><h1>采购单</h1><p>单号：{}　版本：{}　状态：{}</p><p>供应商：{}<br>采购负责人：{}<br>付款条件：{}</p><table><thead><tr><th>商品 / 规格</th><th>数量</th><th>单位</th><th>含税单价</th><th>税率</th><th>含税金额</th><th>预计交期</th></tr></thead><tbody>"#,
        escape(&order.purchase_no),
        escape(&order.purchase_no),
        order.revision_no.map(|v| v.to_string()).unwrap_or_else(|| "草稿".into()),
        order.status.label(),
        escape(&order.supplier_name),
        escape(&order.owner_name),
        escape(
            &SupplierPaymentTerm::parse(&order.payment_term_code)
                .map(|term| term.label())
                .unwrap_or_else(|_| order.payment_term_code.clone())
        )
    );
    for line in &order.lines {
        html.push_str(&render_line(line));
    }
    html.push_str("</tbody></table>");
    html.push_str(&render_delivery(order));
    html.push_str(&format!("<p>不含税金额：{}　税额：{}　<strong>含税合计：{}</strong></p><p>供应商确认：________________　日期：________________</p></body></html>", escape(&order.totals.net), escape(&order.totals.tax), escape(&order.totals.gross)));
    let number: String = order.purchase_no.chars().filter(|c| c.is_alphanumeric() || *c == '-').collect();
    PurchasePaper { filename: format!("采购单-{number}.html"), html }
}
fn render_delivery(order: &PurchaseOrderCenterView) -> String {
    if order.fulfillment_responsibility == FulfillmentResponsibility::Warehouse {
        return format!(
            "<h2>交付安排</h2><p>采购入库目标仓：{}。客户配送由仓库另行安排。</p>",
            escape(order.target_warehouse_name.as_deref().unwrap_or("仓库名称未提供"))
        );
    }
    render_delivery_lines(&order.lines)
}
fn render_delivery_lines(lines: &[PurchaseOrderLineView]) -> String {
    let mut html = "<h2>交付安排</h2><table><thead><tr><th>商品 / 规格</th><th>收货人 / 电话</th><th>完整地址</th><th>数量</th></tr></thead><tbody>".to_string();
    for line in lines.iter().filter(|line| line.quantity.is_some() && !is_removed(line)) {
        if line.delivery_allocations.is_empty() {
            html.push_str(&format!(
                "<tr><td>{}</td><td colspan=3>未指定交付地址，履约前补齐</td></tr>",
                escape(line.product_name.as_deref().unwrap_or("—"))
            ));
        }
        for row in &line.delivery_allocations {
            html.push_str(&format!(
                "<tr><td>{}<br>{}</td><td>{}<br>{}</td><td>{}</td><td>{} {}</td></tr>",
                escape(line.product_name.as_deref().unwrap_or("—")),
                escape(line.specification.as_deref().unwrap_or("")),
                escape(&row.recipient),
                escape(&row.phone),
                escape(&row.address),
                row.quantity,
                escape(line.base_unit_code.as_deref().unwrap_or(""))
            ));
        }
    }
    html.push_str("</tbody></table>");
    html
}
/// 采购改单移除的商品行数量为零，保留在采购单上并标注已取消。
fn is_removed(line: &PurchaseOrderLineView) -> bool {
    line.line_type == PurchaseLineType::ItemService
        && line
            .quantity
            .as_deref()
            .and_then(|value| Decimal::from_str(value).ok())
            .is_some_and(|value| value.is_zero())
}
fn render_line(line: &PurchaseOrderLineView) -> String {
    let fee_name =
        line.expense.as_ref().map(|expense| format!("{}：{}", expense.category.label(), expense.description));
    let name = escape(line.product_name.as_deref().or(fee_name.as_deref()).unwrap_or("费用"));
    let name = if is_removed(line) { format!("{name}（已取消）") } else { name };
    format!(
        "<tr><td>{}<br>{}</td><td class=\"num\">{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td></tr>",
        name,
        escape(line.specification.as_deref().unwrap_or("")),
        escape(line.quantity.as_deref().unwrap_or("—")),
        escape(line.base_unit_code.as_deref().unwrap_or("—")),
        escape(line.unit_cost_gross.as_deref().unwrap_or("—")),
        tax_percent(line.input_tax_rate.as_deref()),
        escape(&line.gross_amount),
        escape(line.expected_delivery_date.as_deref().unwrap_or("—"))
    )
}
fn tax_percent(value: Option<&str>) -> String {
    value
        .and_then(|value| Decimal::from_str(value).ok())
        .and_then(|rate| rate.checked_mul(Decimal::ONE_HUNDRED))
        .map(|rate| format!("{}%", rate.normalize()))
        .unwrap_or_else(|| "—".into())
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
#[cfg(test)]
mod tests {
    use erp_procurement::entity::purchase_order::PurchaseDeliveryAllocation;

    use super::*;
    #[test]
    fn escapes_document_data_without_executable_markup() {
        assert_eq!(escape("<script>\"a&b\"</script>"), "&lt;script&gt;&quot;a&amp;b&quot;&lt;/script&gt;");
    }
    #[test]
    fn printed_line_uses_purchase_amounts_and_excludes_internal_references() {
        let mut line = PurchaseOrderLineView {
            delivery_allocations: Vec::new(),

            line_id: "internal-line".into(),
            line_no: 1,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some("internal-confirmation".into()),
            sku_id: None,
            sku_revision_id: None,
            product_name: Some("茶礼 <限定>".into()),
            specification: Some("A&B".into()),
            quantity: Some("2".into()),
            base_unit_code: Some("盒".into()),
            unit_cost_gross: Some("56.50".into()),
            input_tax_rate: Some("0.13".into()),
            gross_amount: "113.00".into(),
            net_amount: "100.00".into(),
            tax_amount: "13.00".into(),
            expected_delivery_date: Some("2026-10-20".into()),
            sales_order_line_id: Some("internal-sales".into()),
            sales_order_revision_line_id: None,
            sales_order_submission_line_id: None,
            allocated_quantity: Some("2".into()),
            expense: None,
            expense_occurrence: None,
        };
        let html = render_line(&line);
        for expected in ["茶礼 &lt;限定&gt;", "A&amp;B", "56.50", "113.00", "13%", "2026-10-20"] {
            assert!(html.contains(expected), "missing {expected}");
        }
        assert!(!html.contains("internal-"));
        assert!(!html.contains("<限定>"));
        assert_eq!(tax_percent(Some("0.095")), "9.5%");
        assert_eq!(tax_percent(None), "—");
        let empty = render_delivery_lines(std::slice::from_ref(&line));
        assert!(empty.contains("未指定交付地址"));
        line.delivery_allocations = vec![PurchaseDeliveryAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            address: "A区<1号>".into(),
            quantity: "2".parse().unwrap(),
        }];
        let delivery = render_delivery_lines(std::slice::from_ref(&line));
        assert!(delivery.contains("A区&lt;1号&gt;"));
        assert!(delivery.contains("13800000000"));
        assert!(!delivery.contains("internal-"));

        line.quantity = Some("0.000000".into());
        line.delivery_allocations = Vec::new();
        assert!(render_line(&line).contains("（已取消）"));
        assert!(!render_delivery_lines(&[line]).contains("未指定交付地址"));
    }
}
