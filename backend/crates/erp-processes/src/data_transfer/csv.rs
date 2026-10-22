use erp_contract::ContractStatus;
use erp_customer::CustomerAccountStatus;
use erp_inventory::{MovementType, ReservationStatus, StockAdjustmentState};
use erp_procurement::entity::purchase_order::{ProgressStatus, PurchaseOrderStatus, PurchaseType};
use erp_supply::entity::supplier_fulfillment::status::{CancelStatus, FulfillmentStatus, RefundStatus};
use serde::de::DeserializeOwned;
use serde_json::Value;

use super::TransferKind;
use super::quotes::date;
use crate::{Error, Result};

pub(super) fn render(kind: TransferKind, input: &Value, rows: &[Value]) -> Result<(String, String)> {
    let (title, fields) = columns(kind, input)?;
    let mut lines = vec![fields.iter().map(|(_, label)| cell(label)).collect::<Vec<_>>().join(",")];
    for row in rows {
        lines.push(
            fields
                .iter()
                .map(|(key, _)| {
                    cell(&if *key == "$scope" {
                        value(input, "scope_version")
                    } else {
                        display(kind, row, key)
                    })
                })
                .collect::<Vec<_>>()
                .join(","),
        );
    }
    Ok((format!("{title}.csv"), format!("\u{feff}{}", lines.join("\r\n"))))
}
fn value(row: &Value, key: &str) -> String {
    let value = key.split('.').fold(row, |v, part| &v[part]);
    match value {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        _ => value.to_string(),
    }
}
fn display(kind: TransferKind, row: &Value, key: &str) -> String {
    if kind == TransferKind::Customers
        && key == "owner_user_name"
        && let Some(owners) = row["owners"].as_array().filter(|v| !v.is_empty())
    {
        return owners
            .iter()
            .map(|o| o["user_name"].as_str().unwrap_or("未命名销售"))
            .collect::<Vec<_>>()
            .join("、");
    }
    if ["created_at", "current_revision.valid_from", "current_revision.valid_to"].contains(&key) {
        let raw = key.split('.').fold(row, |v, p| &v[p]);
        return raw.as_str().map(str::to_owned).unwrap_or_else(|| date(raw));
    }
    label(kind, row, key).unwrap_or_else(|| value(row, key))
}
fn label(kind: TransferKind, row: &Value, key: &str) -> Option<String> {
    use TransferKind::*;
    let value = &row[key];
    match (kind, key) {
        (Contracts, "status") => enum_label(value, ContractStatus::label),
        (Customers, "status") => enum_label(value, CustomerAccountStatus::label),
        (InventoryReservation, "status") => enum_label(value, ReservationStatus::label),
        (InventoryAdjustment, "status") => enum_label(value, StockAdjustmentState::label),
        (InventoryMovement, "movement_type") => enum_label(value, MovementType::label),
        (PurchaseOrders, "status") => enum_label(value, PurchaseOrderStatus::label),
        (PurchaseOrders, "purchase_type") => enum_label(value, PurchaseType::label),
        (PurchaseOrders, "payment_progress" | "fulfillment_progress") => {
            enum_label(value, ProgressStatus::label)
        },
        (SupplierOrders, "fulfillment_status") => enum_label(value, FulfillmentStatus::label),
        (SupplierOrders, "cancel_status") => enum_label(value, CancelStatus::label),
        (SupplierOrders, "refund_status") => enum_label(value, RefundStatus::label),
        (SalesOrders, "business_type") => Some(if value == "VOUCHER" { "卡券" } else { "实物及服务" }.into()),
        (SalesOrders, "origin_system") => Some(if value == "ERP" { "ERP 手工" } else { "商城" }.into()),
        _ => None,
    }
}
fn enum_label<T: DeserializeOwned>(value: &Value, label: fn(&T) -> &'static str) -> Option<String> {
    serde_json::from_value(value.clone()).ok().map(|v| label(&v).to_owned())
}
fn cell(value: &str) -> String {
    let prefix = if value.trim_start().starts_with(['=', '+', '-', '@']) || value.starts_with(['\t', '\r']) {
        "'"
    } else {
        ""
    };
    format!("\"{prefix}{}\"", value.replace('"', "\"\""))
}
type Columns = (&'static str, &'static [(&'static str, &'static str)]);
fn columns(kind: TransferKind, input: &Value) -> Result<Columns> {
    use TransferKind::*;
    Ok(match kind {
        SalesOrders => COLUMNS_1,
        PurchaseOrders => COLUMNS_2,
        Contracts => COLUMNS_3,
        Customers => COLUMNS_4,
        InventoryBalance => COLUMNS_5,
        InventoryMovement => COLUMNS_6,
        InventoryAdjustment => COLUMNS_7,
        InventoryReservation => COLUMNS_8,
        SupplierOrders => COLUMNS_9,
        InvoiceRequests => COLUMNS_10,
        Receivables => match input["view"].as_str() {
            Some("receipt") => COLUMNS_11,
            Some("sales_invoice") => COLUMNS_12,
            Some("receivable") => COLUMNS_13,
            _ => return Err(Error::ValidationError("未知的客户往来视图".into())),
        },
        SupplierPayables => match input["view"].as_str() {
            Some("payment") => COLUMNS_14,
            Some("allocation") => COLUMNS_15,
            Some("payable") => COLUMNS_16,
            _ => return Err(Error::ValidationError("未知的供应商往来视图".into())),
        },
        MasterData => COLUMNS_17,
        _ => return Err(Error::ValidationError("任务不属于列表导出".into())),
    })
}
#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn exports_business_fields_including_multiple_owners_and_reservation_balance() {
        let (_, csv) = render(TransferKind::Customers, &json!({}), &[json!({"customer_no":"C1","legal_name":"公司","status":"active","owners":[{"user_name":"销售甲"},{"user_name":"销售乙"}]})]).unwrap();
        assert!(csv.contains("销售甲、销售乙"));
        let (_, csv) = render(TransferKind::InventoryReservation, &json!({}), &[json!({"id":"R1","reserved_quantity":"12.300","consumed_quantity":"2.000","released_quantity":"1.000","status":"active"})]).unwrap();
        assert!(csv.contains("\"12.300\",\"2.000\",\"1.000\""));
        let (_, csv) =
            render(TransferKind::SupplierOrders, &json!({}), &[json!({"fulfillment_order_no":"SF1"})])
                .unwrap();
        assert!(csv.contains("SF1"));
        assert_eq!(
            display(
                TransferKind::Contracts,
                &json!({"current_revision":{"valid_from":"2026-10-22"}}),
                "current_revision.valid_from"
            ),
            "2026-10-22"
        );
        assert!(cell("  =1+1").starts_with("\"'"));
    }
    #[test]
    fn csv_escapes_text_and_preserves_decimal_strings() {
        let (_, text) = render(TransferKind::InventoryBalance, &json!({}), &[json!({"warehouse_name":"=bad()", "sku_code":"a,\"b", "on_hand_quantity":"123456789.123456", "reserved_quantity":null, "available_quantity":"0"})]).unwrap();
        assert!(text.contains("\"'=bad()\""));
        assert!(text.contains("\"a,\"\"b\""));
        assert!(text.contains("123456789.123456"));
        assert!(text.ends_with("\"\",\"0\""));
    }
}

const COLUMNS_1: Columns = (
    "销售单列表",
    &[
        ("order_no", "销售单号"),
        ("customer_name", "客户"),
        ("contract_no", "合同"),
        ("business_type", "业务性质"),
        ("stage.label", "状态"),
        ("origin_system", "创建来源"),
        ("gross_amount", "成交金额（含税）"),
        ("owner_user_name", "负责人"),
        ("created_at", "提交时间"),
    ],
);
const COLUMNS_2: Columns = (
    "采购单列表",
    &[
        ("purchase_no", "采购单号"),
        ("status", "状态"),
        ("supplier_name", "供应商"),
        ("sales_order_no", "来源销售单"),
        ("purchase_type", "类型"),
        ("gross_amount", "含税金额"),
        ("payment_progress", "付款"),
        ("fulfillment_progress", "履约"),
        ("owner_name", "负责人"),
    ],
);
const COLUMNS_3: Columns = (
    "合同列表",
    &[
        ("contract_no", "合同编号"),
        ("current_revision.customer_name", "客户"),
        ("current_revision.settlement_party_name", "结算主体"),
        ("status", "状态"),
        ("owner_user_name", "当前跟进负责人"),
        ("current_revision.valid_from", "有效期起"),
        ("current_revision.valid_to", "有效期止"),
    ],
);
const COLUMNS_4: Columns = (
    "客户目录",
    &[
        ("customer_no", "客户编号"),
        ("legal_name", "客户名称"),
        ("status", "状态"),
        ("owner_user_name", "负责销售"),
    ],
);
const COLUMNS_5: Columns = (
    "库存余额",
    &[
        ("warehouse_name", "仓库"),
        ("sku_code", "SKU"),
        ("on_hand_quantity", "现存"),
        ("reserved_quantity", "预占"),
        ("available_quantity", "可用"),
    ],
);
const COLUMNS_6: Columns = (
    "库存流水",
    &[
        ("warehouse_id", "仓库"),
        ("sku_id", "SKU"),
        ("movement_type", "类型"),
        ("quantity", "数量"),
        ("recorded_by", "经办人"),
    ],
);
const COLUMNS_7: Columns = (
    "库存调整",
    &[
        ("adjustment_no", "调整单"),
        ("warehouse_id", "仓库"),
        ("prepared_by", "经办人"),
        ("submitted_by", "申请人"),
        ("current_assignee", "当前审批人"),
        ("status", "状态"),
    ],
);
const COLUMNS_8: Columns = (
    "销售预占",
    &[
        ("id", "预占编号"),
        ("sales_order_line_id", "销售明细"),
        ("warehouse_id", "仓库"),
        ("sku_id", "SKU"),
        ("reserved_quantity", "有效预占数量"),
        ("consumed_quantity", "已消耗数量"),
        ("released_quantity", "已释放数量"),
        ("status", "状态"),
    ],
);
const COLUMNS_9: Columns = (
    "供应商订单",
    &[
        ("fulfillment_order_no", "订单号"),
        ("supplier_id", "供应商"),
        ("external_order_no", "供应商订单号"),
        ("fulfillment_status", "履约状态"),
        ("cancel_status", "取消状态"),
        ("refund_status", "退款状态"),
    ],
);
const COLUMNS_10: Columns = (
    "开票申请",
    &[
        ("request_no", "申请单号"),
        ("sales_order_no", "销售单"),
        ("amount", "申请金额"),
        ("applicant_user_id", "申请人"),
        ("handler_user_id", "当前处理人"),
        ("status", "状态"),
        ("$scope", "范围版本"),
    ],
);
const COLUMNS_11: Columns = (
    "客户回款",
    &[
        ("receipt_no", "回款单号"),
        ("visible_allocated_share", "获授权已分配"),
        ("amount", "整单金额"),
        ("allocated_total", "整单已分配"),
        ("unallocated_amount", "未分配"),
        ("status", "状态"),
        ("$scope", "范围版本"),
    ],
);
const COLUMNS_12: Columns = (
    "销项发票",
    &[
        ("invoice_no", "发票号码"),
        ("visible_allocated_share", "获授权已分配"),
        ("gross_amount", "整单金额"),
        ("allocated_total", "整单已分配"),
        ("unallocated_amount", "未分配"),
        ("status", "状态"),
        ("$scope", "范围版本"),
    ],
);
const COLUMNS_13: Columns = (
    "客户应收",
    &[
        ("account_seq", "子账"),
        ("sales_order_id", "销售单"),
        ("visible_settled_share", "获授权已核销"),
        ("gross_total", "整单金额"),
        ("settled_total", "整单已核销"),
        ("open_total", "未分配"),
        ("status", "状态"),
        ("$scope", "范围版本"),
    ],
);
const COLUMNS_14: Columns = (
    "供应商付款",
    &[
        ("payment_no", "付款单号"),
        ("visible_allocated_share", "获授权已分配"),
        ("amount", "整单金额"),
        ("allocated_total", "整单已分配"),
        ("unallocated_amount", "未分配"),
        ("status", "状态"),
        ("$scope", "范围版本"),
    ],
);
const COLUMNS_15: Columns = (
    "进项发票分配",
    &[
        ("id", "分配"),
        ("invoice_no", "进项发票"),
        ("payable_account_id", "应付子账"),
        ("visible_allocated_amount", "获授权分配"),
        ("allocated_gross_amount", "整单分配"),
        ("$scope", "范围版本"),
    ],
);
const COLUMNS_16: Columns = (
    "供应商应付",
    &[
        ("id", "子账"),
        ("source_document_id", "采购单"),
        ("visible_settled_share", "获授权已核销"),
        ("gross_total", "整单金额"),
        ("settled_total", "整单已核销"),
        ("open_total", "未分配"),
        ("status", "状态"),
        ("$scope", "范围版本"),
    ],
);
const COLUMNS_17: Columns = (
    "基础资料",
    &[
        ("number", "资料编号"),
        ("name", "名称"),
        ("revision", "版本"),
        ("lifecycle", "启用状态"),
        ("timing", "版本状态"),
        ("from", "生效开始"),
        ("to", "生效结束"),
        ("blocker", "不可用原因"),
        ("facts", "业务事实"),
    ],
);

#[cfg(test)]
mod scope_tests {
    use serde_json::json;

    use super::*;
    #[test]
    fn partial_financial_scope_does_not_replace_redacted_total_with_zero() {
        let (_,csv)=render(TransferKind::Receivables,&json!({"view":"receipt","scope_version":"scope-1"}),&[json!({"receipt_no":"R","visible_allocated_share":"12.3400","amount":null,"allocated_total":null,"unallocated_amount":null,"status":"active"})]).unwrap();
        assert!(csv.contains("\"R\",\"12.3400\",\"\",\"\",\"\""));
        assert!(csv.ends_with("\"scope-1\""));
        assert!(!csv.contains("\"0\""));
    }
}
