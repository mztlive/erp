//! 集成简报类型、证据类别与已知外部业务编号的可读投影。

use erp_integration::entity::integration_ops::CanonicalEvidenceReference;

pub(super) fn business_type_label(kind: &str) -> &'static str {
    match kind.trim().to_ascii_lowercase().as_str() {
        "mall_order" | "商城订单" => "商城订单",
        "supplier_fulfillment_order" | "supplier_order" | "供应商订单" => "供应商订单",
        "sales_order" | "销售单" => "销售单",
        "purchase_order" | "采购单" => "采购单",
        "supplier_offering" => "供应商供给",
        "legacy_import_batch" => "历史导入批次",
        "customer_refund" => "客户退款单",
        "supplier_refund" => "供应商退款单",
        _ => "业务对象",
    }
}

pub(super) fn difference_type_label(kind: &str) -> &'static str {
    match kind.trim().to_ascii_lowercase().as_str() {
        "mall_missing" | "missing_mall_fact" => "商城无对应记录",
        "erp_missing" | "missing_erp_fact" => "ERP 无对应记录",
        "status_difference" | "status_mismatch" => "状态不一致",
        "content_fingerprint_difference" => "内容不一致",
        "duplicate_identity" => "重复业务记录",
        "amount_mismatch" | "amount_and_line_count" | "金额不一致" => "金额不一致",
        "refund_mismatch" => "退款不一致",
        "balance_mismatch" => "余额不一致",
        "settlement_mismatch" => "结算不一致",
        "cost_mismatch" => "成本不一致",
        "supplier_supply_mismatch" => "供应商供给不一致",
        "supplier_order_mismatch" => "供应商订单不一致",
        "supply_mismatch" => "供给不一致",
        "result_unknown" | "integration_result_unknown" => "集成结果未知",
        _ => "业务差异",
    }
}

/// 仅解释已存引用的类别，不证明引用存在或已通过权威核验。
pub(super) fn evidence_category(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    let value = completion_evidence(value).unwrap_or(value);
    Some(
        value
            .split(';')
            .map(|part| {
                let Ok(reference) = CanonicalEvidenceReference::parse_stored(part) else {
                    return "证据类别未维护";
                };
                match reference.kind() {
                    "inbox_message" => "入站消息证据",
                    "supplier_order_action" => "供应商订单操作证据",
                    "supplier_refund_fact" => "供应商退款结果证据",
                    "customer_refund" => "客户退款单证据",
                    "supplier_refund" => "供应商退款单证据",
                    "work_item" => "处理任务证据",
                    "command_receipt" => "命令执行收据证据",
                    "reconciliation_difference_resolution" => "差异处理记录证据",
                    "mall_order_fact" | "mall-snapshot" => "商城订单事实证据",
                    "erp-revision" => "ERP 单据版本证据",
                    "sales_order" => "销售单证据",
                    "purchase_order" => "采购单证据",
                    _ => "证据类别未维护",
                }
            })
            .collect::<Vec<_>>()
            .join("、"),
    )
}

/// 已完成错误任务的说明只投影终态证据，操作与账号身份不进入显示值。
fn completion_evidence(value: &str) -> Option<&str> {
    let value = value.strip_prefix("operation=")?;
    let (_, value) = value.split_once(";reason_code=")?;
    let (_, value) = value.split_once(";terminal_evidence=")?;
    let (evidence, _) = value.rsplit_once(";actor=")?;
    Some(evidence)
}

/// 商城尚无仓储可查；只保留该类型中明显有业务结构的外部单号。
pub(super) fn external_reference(kind: &str, id: &str) -> Option<String> {
    let id = id.trim();
    let opaque = matches!(id.len(), 24 | 32) && id.chars().all(|ch| ch.is_ascii_hexdigit());
    (matches!(kind.trim().to_ascii_lowercase().as_str(), "mall_order" | "商城订单")
        && !id.is_empty()
        && !opaque
        && !id.contains([':', '/', ';']))
    .then(|| id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_only_explains_category_and_never_shows_identity() {
        assert_eq!(
            evidence_category(Some("inbox_message:opaque:v2:processed")).as_deref(),
            Some("入站消息证据")
        );
        assert_eq!(
            evidence_category(Some("supplier_refund_fact://missing")).as_deref(),
            Some("供应商退款结果证据")
        );
        assert_eq!(evidence_category(Some("unknown:opaque")).as_deref(), Some("证据类别未维护"));
        assert_eq!(
            evidence_category(Some("malformed:ref:with:extra:parts")).as_deref(),
            Some("证据类别未维护")
        );
    }

    #[test]
    fn closed_task_evidence_preserves_registered_categories() {
        assert_eq!(
            evidence_category(Some("work_item:task-1;command_receipt:receipt-1")).as_deref(),
            Some("处理任务证据、命令执行收据证据")
        );
    }

    #[test]
    fn completed_task_evidence_omits_operation_and_actor_metadata() {
        let resolution = "operation=operation-1;reason_code=RESULT_CONFIRMED;terminal_evidence=inbox_message:message-1:v2:processed;supplier_order_action:action-1:v3:succeeded;actor=operator-1";
        assert_eq!(evidence_category(Some(resolution)).as_deref(), Some("入站消息证据、供应商订单操作证据"));
        assert_eq!(
            evidence_category(Some("operation=operation-1;reason_code=RESULT_CONFIRMED;terminal_evidence=inbox_message:message-1:v2:processed")).as_deref(),
            Some("证据类别未维护、证据类别未维护、证据类别未维护")
        );
    }

    #[test]
    fn external_order_number_is_not_used_for_internal_typed_identity() {
        assert_eq!(external_reference("mall_order", "MALL-1001").as_deref(), Some("MALL-1001"));
        assert!(external_reference("mall_order", "507f1f77bcf86cd799439011").is_none());
        assert!(external_reference("sales_order", "SO-1001").is_none());
    }

    #[test]
    fn difference_registry_uses_readable_labels_and_safe_unknown() {
        assert_eq!(difference_type_label(" Amount_Mismatch "), "金额不一致");
        assert_eq!(difference_type_label("integration_result_unknown"), "集成结果未知");
        assert_eq!(difference_type_label("opaque-custom-code"), "业务差异");
    }
}
