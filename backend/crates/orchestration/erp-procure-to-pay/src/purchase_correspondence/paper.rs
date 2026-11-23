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
        concat!(
            r#"<!doctype html><html lang="zh-CN"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1"><title>采购单 {}</title>"#,
            r#"<style>body{{font:14px/1.6 sans-serif;color:#17212b;max-width:1000px;margin:32px auto;padding:24px}}h1{{text-align:center}}h2{{font-size:16px;margin:0 0 12px}}table{{width:100%;border-collapse:collapse;margin:20px 0}}th,td{{padding:9px;border:1px solid #aaa;text-align:left;vertical-align:top}}.num{{text-align:right;font-variant-numeric:tabular-nums}}.muted{{color:#57606a;font-size:12px}}.delivery-section{{margin-top:28px;padding-top:16px;border-top:1px solid #d0d7de}}.delivery-group{{margin:14px 0;border:1px solid #d0d7de;border-radius:4px;overflow:hidden}}.delivery-group-head{{display:flex;justify-content:space-between;align-items:baseline;gap:16px;padding:8px 12px;background:#f6f8fa;border-bottom:1px solid #d0d7de;font-size:13px}}.delivery-group table{{margin:0;font-size:13px}}.delivery-group th,.delivery-group td{{border:0;border-right:1px solid #d0d7de;border-bottom:1px solid #d0d7de;padding:7px 9px}}.delivery-group th:last-child,.delivery-group td:last-child{{border-right:0}}.delivery-group tbody tr:last-child td{{border-bottom:0}}.delivery-note{{margin-top:12px;padding:10px 12px;border:1px solid #d0d7de;background:#f6f8fa;border-radius:4px;font-size:13px}}@page{{size:A4 landscape;margin:15mm}}tr{{break-inside:avoid}}thead{{display:table-header-group}}</style>"#,
            r#"</head><body data-purchase-no="{}" data-supplier-name="{}"><h1>采购单</h1><p>单号：{}　版本：{}　状态：{}</p><p>供应商：{}<br>采购负责人：{}<br>付款条件：{}</p><table><thead><tr><th>商品 / 规格</th><th class="num">数量</th><th>单位</th><th class="num">含税单价</th><th class="num">税率</th><th class="num">含税金额</th><th>预计交期</th></tr></thead><tbody>"#
        ),
        escape(&order.purchase_no),
        escape(&order.purchase_no),
        escape(&order.supplier_name),
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
    html.push_str(&format!(
        "<p>不含税金额：{}　税额：{}　<strong>含税合计：{}</strong></p><p>供应商确认：________________　日期：________________</p>",
        escape(&order.totals.net),
        escape(&order.totals.tax),
        escape(&order.totals.gross)
    ));
    html.push_str(&render_delivery(order));
    html.push_str("</body></html>");

    let number: String = order.purchase_no.chars().filter(|c| c.is_alphanumeric() || *c == '-').collect();
    PurchasePaper { filename: format!("采购单-{number}.html"), html }
}

fn render_delivery(order: &PurchaseOrderCenterView) -> String {
    if order.fulfillment_responsibility == FulfillmentResponsibility::Warehouse {
        return format!(
            r#"<section class="delivery-section" data-purchase-delivery="true" data-purchase-delivery-mode="warehouse"><h2>交付安排</h2><p>采购入库目标仓：{}。客户配送由仓库另行安排。</p></section>"#,
            escape(order.target_warehouse_name.as_deref().unwrap_or("仓库名称未提供"))
        );
    }
    render_delivery_lines(&order.lines)
}

fn render_delivery_lines(lines: &[PurchaseOrderLineView]) -> String {
    let mut html = r#"<section class="delivery-section" data-purchase-delivery="true" data-purchase-delivery-mode="direct"><h2>交付安排</h2>"#.to_string();
    let mut has_active_line = false;
    for line in lines.iter().filter(|line| line.quantity.is_some() && !is_removed(line)) {
        has_active_line = true;
        let item_name = line.product_name.as_deref().unwrap_or("—");
        let spec = line.specification.as_deref().unwrap_or("");
        let unit = line.base_unit_code.as_deref().unwrap_or("");
        let line_quantity = line.quantity.as_deref().unwrap_or("");
        let expected_date = line.expected_delivery_date.as_deref().unwrap_or("");

        let allocated_total = line
            .delivery_allocations
            .iter()
            .fold(Decimal::ZERO, |acc, row| acc.checked_add(row.quantity.to_decimal()).unwrap_or(acc))
            .normalize();

        let group_summary = if line.delivery_allocations.is_empty() {
            "未指定交付地址，履约前补齐".to_string()
        } else if unit.is_empty() {
            format!("{} 处地址 · 合计 {}", line.delivery_allocations.len(), allocated_total)
        } else {
            format!("{} 处地址 · 合计 {} {}", line.delivery_allocations.len(), allocated_total, escape(unit))
        };

        let spec_html = if spec.trim().is_empty() {
            String::new()
        } else {
            format!(r#" <span class="muted">{}</span>"#, escape(spec))
        };

        html.push_str(&format!(
            r#"<div class="delivery-group" data-purchase-delivery-group="true" data-item-name="{}" data-item-sku="{}" data-unit="{}" data-quantity="{}" data-expected-date="{}"><div class="delivery-group-head"><div><strong>{}</strong>{}</div><div class="muted">{}</div></div>"#,
            escape(item_name),
            escape(spec),
            escape(unit),
            escape(line_quantity),
            escape(expected_date),
            escape(item_name),
            spec_html,
            group_summary
        ));

        if !line.delivery_allocations.is_empty() {
            html.push_str(
                r#"<table><thead><tr><th style="width:8%">序号</th><th style="width:26%">收货人 / 电话</th><th>完整地址</th><th class="num" style="width:14%">数量</th></tr></thead><tbody>"#,
            );
            for (index, row) in line.delivery_allocations.iter().enumerate() {
                let qty_text = row.quantity.to_decimal().normalize().to_string();
                let qty_display = if unit.is_empty() {
                    escape(&qty_text)
                } else {
                    format!("{} {}", escape(&qty_text), escape(unit))
                };
                html.push_str(&format!(
                    r#"<tr data-purchase-delivery-row="true" data-recipient="{}" data-phone="{}" data-region-name="{}" data-address="{}" data-quantity="{}"><td class="num">{}</td><td>{}<br><span class="muted">{}</span></td><td>{}</td><td class="num">{}</td></tr>"#,
                    escape(&row.recipient),
                    escape(&row.phone),
                    escape(&row.region_name),
                    escape(&row.address),
                    escape(&qty_text),
                    index + 1,
                    escape(&row.recipient),
                    escape(&row.phone),
                    escape(&row.full_address()),
                    qty_display
                ));
            }
            html.push_str("</tbody></table>");
        }
        html.push_str("</div>");
    }
    if !has_active_line {
        html.push_str(r#"<p class="muted">暂无商品交付安排</p>"#);
    }
    html.push_str("</section>");
    html
}

// 采购改单移除的商品行数量为零，保留在采购单上并标注已取消。
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
    use erp_procurement::dto::purchase_order::TotalsView;
    use erp_procurement::entity::purchase_order::{
        ProgressStatus, PurchaseDeliveryAllocation, PurchaseOrderStatus, PurchaseReviewStatus, PurchaseType,
    };
    use erp_read_models::purchase_center::dto::{DocumentApprovalHistoryPageView, DocumentApprovalView};

    use super::*;

    #[test]
    fn escapes_document_data_without_executable_markup() {
        assert_eq!(escape("<script>\"a&b\"</script>"), "&lt;script&gt;&quot;a&amp;b&quot;&lt;/script&gt;");
    }

    #[test]
    fn printed_line_uses_purchase_amounts_and_excludes_internal_references() {
        let mut line = PurchaseOrderLineView {
            inquiry_quote_source: None,
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
        assert!(empty.contains(r#"data-purchase-delivery="true""#));

        line.delivery_allocations = vec![PurchaseDeliveryAllocation {
            recipient: "张三".into(),
            phone: "13800000000".into(),
            region_code: erp_core::region::RegionCode::parse("310115").unwrap(),
            region_name: "上海市浦东新区".into(),
            address: "A区<1号>".into(),
            quantity: "2".parse().unwrap(),
        }];
        let delivery = render_delivery_lines(std::slice::from_ref(&line));
        assert!(delivery.contains("A区&lt;1号&gt;"));
        assert!(delivery.contains("13800000000"));
        assert!(delivery.contains(r#"data-purchase-delivery-group="true""#));
        assert!(delivery.contains(r#"data-purchase-delivery-row="true""#));
        assert!(delivery.contains("1 处地址 · 合计 2 盒"));
        assert!(!delivery.contains("internal-"));

        line.quantity = Some("0.000000".into());
        line.delivery_allocations = Vec::new();
        assert!(render_line(&line).contains("（已取消）"));
        assert!(!render_delivery_lines(&[line]).contains("未指定交付地址"));
    }

    #[test]
    fn totals_and_confirmation_precede_delivery_appendix() {
        let order = PurchaseOrderCenterView {
            id: "internal-po".into(),
            purchase_no: "PO-2026-001".into(),
            status: PurchaseOrderStatus::Effective,
            review_status: PurchaseReviewStatus::Approved,
            version: 1,
            sales_order_id: "internal-so".into(),
            sales_order_no: "SO-INTERNAL".into(),
            supplier_id: "internal-supplier".into(),
            supplier_name: "示例供应商".into(),
            purchase_type: PurchaseType::Physical,
            payment_term_code: "NET-30".into(),
            fulfillment_responsibility: FulfillmentResponsibility::SupplierDirect,
            owner_user_id: "internal-user".into(),
            owner_name: "采购员".into(),
            target_warehouse_id: None,
            target_warehouse_name: None,
            payment_progress: ProgressStatus::None,
            invoice_progress: ProgressStatus::None,
            fulfillment_progress: ProgressStatus::None,
            current_submission_id: None,
            current_revision_id: Some("rev-1".into()),
            revision_no: Some(1),
            content_source: "REVISION".into(),
            lines: Vec::new(),
            source_sales_order: None,
            source_update: None,
            delivery: None,
            totals: TotalsView { gross: "113.00".into(), net: "100.00".into(), tax: "13.00".into() },
            allocations: Vec::new(),
            api_executions: Vec::new(),
            fulfillment_quantities: Vec::new(),
            api_execution_funds: None,
            changes: Vec::new(),
            payable_summary: None,
            approval: DocumentApprovalView {
                requirement: "NO_APPROVAL".into(),
                definition: None,
                instance: None,
                recent_history: Vec::new(),
                history_page: DocumentApprovalHistoryPageView::default(),
                allowed_actions: Vec::new(),
            },
            created_at: 1,
        };

        let paper = render(&order);
        let totals_pos = paper.html.find("含税合计：113.00").expect("totals missing");
        let confirm_pos = paper.html.find("供应商确认").expect("confirmation missing");
        let delivery_pos = paper.html.find("交付安排").expect("delivery missing");
        assert!(totals_pos < confirm_pos);
        assert!(confirm_pos < delivery_pos);
        assert!(!paper.html.contains("SO-INTERNAL"));
        assert!(!paper.html.contains("internal-"));
    }
}
