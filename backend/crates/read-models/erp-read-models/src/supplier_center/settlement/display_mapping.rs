//! 名称投影的纯映射；缺失或伪装成名称的身份不进入展示字段。

use std::collections::HashMap;

use erp_supply::dto::supplier_settlement::{
    SettlementDifferenceEvidenceView, SupplierSettlementDifferenceView, SupplierSettlementItemView,
    SupplierSettlementStatementView,
};

use super::display_dto::{
    SettlementEvidenceDisplayView, SettlementItemDisplayView, SettlementStatementDisplayView,
};

#[derive(Default)]
pub(super) struct StatementDisplayFacts {
    pub supplier_names: HashMap<String, String>,
    pub user_names: HashMap<String, String>,
}

impl StatementDisplayFacts {
    /// 将结算单身份解析为当前名称。
    ///
    /// # 参数
    /// * `statement` - 已授权结算单事实。
    /// # 返回
    /// 返回保留原身份的名称视图。
    /// # 错误
    /// 无；缺失名称保持为空。
    pub(super) fn statement(
        &self,
        statement: SupplierSettlementStatementView,
    ) -> SettlementStatementDisplayView {
        let supplier_name = readable_name(&self.supplier_names, &statement.supplier_id);
        let prepared_by_name = readable_name(&self.user_names, &statement.prepared_by);
        let difference_handler_name = readable_name(&self.user_names, &statement.difference_handler_user_id);
        let reviewed_by_name =
            statement.reviewed_by.as_deref().and_then(|id| readable_name(&self.user_names, id));
        let payable_no =
            statement.payable_account_id.as_ref().and_then(|id| readable_value(&statement.statement_no, id));
        SettlementStatementDisplayView {
            statement,
            supplier_name,
            prepared_by_name,
            difference_handler_name,
            reviewed_by_name,
            payable_no,
        }
    }

    /// 补齐记录人姓名并按引用顺序标注材料。
    ///
    /// # 参数
    /// * `evidence` - 原不可变补证事实。
    /// # 返回
    /// 返回补证名称视图。
    /// # 错误
    /// 无；缺失记录人名称保持为空。
    pub(super) fn evidence(
        &self,
        evidence: SettlementDifferenceEvidenceView,
    ) -> SettlementEvidenceDisplayView {
        let provided_by_name = readable_name(&self.user_names, &evidence.provided_by);
        let evidence_reference_labels =
            (1..=evidence.evidence_reference_ids.len()).map(|index| format!("补证材料 {index}")).collect();
        SettlementEvidenceDisplayView { evidence, provided_by_name, evidence_reference_labels }
    }

    /// 为差异所属补证补齐名称。
    ///
    /// # 参数
    /// * `difference` - 原正式差异投影。
    /// # 返回
    /// 返回原差异和补证名称视图。
    /// # 错误
    /// 无。
    pub(super) fn difference(
        &self,
        difference: SupplierSettlementDifferenceView,
    ) -> SupplierSettlementDifferenceView<SettlementEvidenceDisplayView> {
        SupplierSettlementDifferenceView {
            id: difference.id,
            statement_item_id: difference.statement_item_id,
            difference_type: difference.difference_type,
            difference_amount: difference.difference_amount,
            status: difference.status,
            resolution: difference.resolution,
            resolved_by: difference.resolved_by,
            resolved_at: difference.resolved_at,
            version: difference.version,
            created_at: difference.created_at,
            evidence: difference.evidence.into_iter().map(|value| self.evidence(value)).collect(),
        }
    }
}

/// 从精确身份映射中取可读名称。
///
/// # 参数
/// * `names` - 当前授权结果关联的稀疏名称映射。
/// * `id` - 关联身份。
/// # 返回
/// 返回非空且不同于内部身份的名称。
/// # 错误
/// 无。
pub(super) fn readable_name(names: &HashMap<String, String>, id: &str) -> Option<String> {
    names.get(id).and_then(|value| readable_value(value, id))
}

/// 拒绝空白或与内部身份相同的展示值。
///
/// # 参数
/// * `value` - 拥有领域返回的名称或业务单号。
/// * `id` - 该对象内部身份。
/// # 返回
/// 返回规范化的可读值，缺失时为空。
/// # 错误
/// 无。
pub(super) fn readable_value(value: &str, id: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty() && value != id.trim()).then(|| value.to_string())
}

#[derive(Default)]
pub(super) struct OrderDisplayLabels {
    pub supplier_order_no: Option<String>,
    pub external_order_no: Option<String>,
}

#[derive(Default)]
pub(super) struct ItemDisplayFacts {
    pub orders: HashMap<String, OrderDisplayLabels>,
    pub item_names: HashMap<String, (String, String)>,
}

impl ItemDisplayFacts {
    /// 为结算明细补齐业务单号与所属商品名称。
    ///
    /// # 参数
    /// * `item` - 原结算明细事实。
    /// # 返回
    /// 返回保留内部关联身份的名称视图。
    /// # 错误
    /// 无；缺失或错链商品名称保持为空。
    pub(super) fn item(&self, item: SupplierSettlementItemView) -> SettlementItemDisplayView {
        let order = self.orders.get(&item.supplier_fulfillment_order_id);
        let supplier_order_no = order.and_then(|labels| labels.supplier_order_no.clone());
        let external_order_no = order.and_then(|labels| labels.external_order_no.clone());
        let product_name = self
            .item_names
            .get(&item.supplier_fulfillment_item_id)
            .filter(|(order_id, _)| order_id == &item.supplier_fulfillment_order_id)
            .and_then(|(_, name)| readable_value(name, &item.supplier_fulfillment_item_id));
        SettlementItemDisplayView {
            item,
            supplier_order_no,
            external_order_no,
            product_name,
            purchase_order_id: None,
            purchase_order_no: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::common::time::{BusinessDate, Instant};
    use erp_core::ids::{SupplierAccountId, SupplierSettlementStatementId};
    use erp_core::money::{Amount, Quantity};
    use erp_supply::dto::supplier_settlement::SettlementDraftCommandResult;
    use erp_supply::entity::supplier_settlement::{
        SupplierSettlementStatement, SupplierSettlementStatementData,
    };

    use super::*;
    use crate::supplier_center::settlement::SettlementStatementResult;

    fn statement() -> SupplierSettlementStatementView {
        SupplierSettlementStatement::new(
            SupplierSettlementStatementId::new("statement-1"),
            SupplierSettlementStatementData {
                statement_no: "ST-2026-001".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
                period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
                period_policy_id: "calendar-month".to_string(),
                period_policy_version: "1".to_string(),
                period_timezone: "Asia/Shanghai".to_string(),
                external_bill_no: None,
                external_bill_version: None,
                erp_amount: Amount::from_str("100.00").unwrap(),
                supplier_amount: Amount::from_str("100.00").unwrap(),
                subject_hash: "a".repeat(64),
                source_as_of: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_at: Instant::from_unix_secs(1_700_000_000),
                source_snapshot_hash: "b".repeat(64),
                refresh_cutoff_policy_id: "policy-1".to_string(),
                refresh_cutoff_policy_version: "1".to_string(),
                prepared_by: "preparer-1".to_string(),
                business_org_unit_id: "org-1".to_string(),
                difference_handler_user_id: "handler-1".to_string(),
            },
        )
        .unwrap()
        .into()
    }

    fn item() -> SupplierSettlementItemView {
        let zero = Amount::from_str("0.00").unwrap();
        SupplierSettlementItemView {
            id: "item-1".to_string(),
            statement_id: "statement-1".to_string(),
            supplier_fulfillment_order_id: "supplier-order-1".to_string(),
            supplier_fulfillment_item_id: "fulfillment-item-1".to_string(),
            quantity: Quantity::from_str("1").unwrap(),
            order_amount: zero,
            freight_amount: zero,
            service_fee_amount: zero,
            refund_amount: zero,
            erp_calculated_amount: zero,
            erp_calculated_net_amount: zero,
            erp_calculated_tax_amount: zero,
            supplier_billed_amount: zero,
            supplier_billed_net_amount: zero,
            supplier_billed_tax_amount: zero,
            created_at: 1_700_000_000,
        }
    }

    #[test]
    fn settlement_item_display_preserves_order_links_and_readable_product() {
        let facts = ItemDisplayFacts {
            orders: HashMap::from([(
                "supplier-order-1".to_string(),
                OrderDisplayLabels {
                    supplier_order_no: Some("SFO-2026-001".to_string()),
                    external_order_no: Some("EXT-2026-001".to_string()),
                },
            )]),
            item_names: HashMap::from([(
                "fulfillment-item-1".to_string(),
                ("supplier-order-1".to_string(), "员工福利礼盒".to_string()),
            )]),
        };
        let original = item();
        let view = facts.item(original.clone());
        assert_eq!(view.supplier_order_no.as_deref(), Some("SFO-2026-001"));
        assert_eq!(view.external_order_no.as_deref(), Some("EXT-2026-001"));
        assert_eq!(view.product_name.as_deref(), Some("员工福利礼盒"));
        assert_eq!(view.item, original);
        assert!(view.purchase_order_id.is_none());
        assert!(view.purchase_order_no.is_none());
    }

    #[test]
    fn settlement_item_display_does_not_use_missing_or_other_order_product() {
        let facts = ItemDisplayFacts {
            item_names: HashMap::from([(
                "fulfillment-item-1".to_string(),
                ("other-order".to_string(), "其它订单商品".to_string()),
            )]),
            ..Default::default()
        };
        let view = facts.item(item());
        assert!(view.supplier_order_no.is_none());
        assert!(view.external_order_no.is_none());
        assert!(view.product_name.is_none());
        let facts = ItemDisplayFacts {
            item_names: HashMap::from([(
                "fulfillment-item-1".to_string(),
                ("supplier-order-1".to_string(), "fulfillment-item-1".to_string()),
            )]),
            ..Default::default()
        };
        assert!(facts.item(item()).product_name.is_none());
    }

    #[test]
    fn settlement_statement_display_uses_names_and_preserves_identities() {
        let mut statement = statement();
        statement.reviewed_by = Some("reviewer-1".to_string());
        statement.payable_account_id = Some("payable-1".to_string());
        let facts = StatementDisplayFacts {
            supplier_names: HashMap::from([("supplier-1".to_string(), "供应商公司".to_string())]),
            user_names: HashMap::from([
                ("preparer-1".to_string(), "对账人员".to_string()),
                ("handler-1".to_string(), "差异人员".to_string()),
                ("reviewer-1".to_string(), "复核人员".to_string()),
            ]),
        };
        let view = facts.statement(statement.clone());
        assert_eq!(view.supplier_name.as_deref(), Some("供应商公司"));
        assert_eq!(view.prepared_by_name.as_deref(), Some("对账人员"));
        assert_eq!(view.difference_handler_name.as_deref(), Some("差异人员"));
        assert_eq!(view.reviewed_by_name.as_deref(), Some("复核人员"));
        assert_eq!(view.payable_no.as_deref(), Some("ST-2026-001"));
        assert_eq!(view.statement, statement);
        let json = serde_json::to_value(view).unwrap();
        assert_eq!(json["supplier_id"], "supplier-1");
        assert_eq!(json["supplier_name"], "供应商公司");
        assert!(json.get("statement").is_none());
    }

    #[test]
    fn settlement_display_missing_or_identity_names_remain_absent() {
        let facts = StatementDisplayFacts {
            supplier_names: HashMap::from([("supplier-1".to_string(), "supplier-1".to_string())]),
            user_names: HashMap::from([("preparer-1".to_string(), "   ".to_string())]),
        };
        let view = facts.statement(statement());
        assert!(view.supplier_name.is_none());
        assert!(view.prepared_by_name.is_none());
        assert!(view.difference_handler_name.is_none());
        assert!(view.reviewed_by_name.is_none());
        assert!(view.payable_no.is_none());
        assert_eq!(view.statement.supplier_id, "supplier-1");
    }

    #[test]
    fn settlement_evidence_display_labels_opaque_references_without_exposing_ids() {
        let evidence = SettlementDifferenceEvidenceView {
            evidence_id: "evidence-1".to_string(),
            evidence_reference_ids: vec!["opaque-ref-1".to_string(), "ticket://T-1".to_string()],
            opinion_code: None,
            comment: None,
            provided_by: "preparer-1".to_string(),
            provided_at: 1_700_000_000,
        };
        let facts = StatementDisplayFacts {
            user_names: HashMap::from([("preparer-1".to_string(), "记录人员".to_string())]),
            ..Default::default()
        };
        let view = facts.evidence(evidence.clone());
        assert_eq!(view.provided_by_name.as_deref(), Some("记录人员"));
        assert_eq!(view.evidence_reference_labels, vec!["补证材料 1", "补证材料 2"]);
        assert_eq!(view.evidence, evidence);
        let mut empty = evidence;
        empty.evidence_reference_ids.clear();
        empty.provided_by = "missing".to_string();
        let view = facts.evidence(empty);
        assert!(view.provided_by_name.is_none());
        assert!(view.evidence_reference_labels.is_empty());
    }

    #[test]
    fn settlement_command_display_preserves_formal_result_and_http_shape() {
        let result = SettlementDraftCommandResult {
            result_status: "CREATED".to_string(),
            message: "已创建".to_string(),
            request_id: "request-1".to_string(),
            statement: statement(),
            item_count: 4,
            difference_count: 2,
        };
        let display = StatementDisplayFacts::default().statement(result.statement.clone());
        let view = result.with_statement(display);
        assert_eq!(view.result_status, "CREATED");
        assert_eq!(view.request_id, "request-1");
        assert_eq!(view.item_count, 4);
        assert_eq!(view.difference_count, 2);
        let json = serde_json::to_value(view).unwrap();
        assert_eq!(json["statement"]["statement_no"], "ST-2026-001");
        assert!(json["statement"].get("statement").is_none());
        assert!(json["statement"]["supplier_name"].is_null());
    }
}
