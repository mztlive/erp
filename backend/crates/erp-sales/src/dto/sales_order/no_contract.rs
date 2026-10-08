//! 无合同销售可编辑 DTO 与服务器读取资料的确定性组装。
use erp_core::ids::PartyId;
use validator::Validate;

use super::{SalesOrderDraftRequest, SalesOrderEditableDraftRequest};
use crate::{Error, Result};
impl SalesOrderEditableDraftRequest {
    /// 编辑旧销售单时，省略结算主体表示保留原选择。
    ///
    /// # 参数
    /// `existing` 为原销售单结算主体，`has_contract` 表示本次是否关联合同。
    /// # 返回
    /// 仅补齐对应请求字段中的缺省值，显式选择保持不变。
    /// # 错误
    /// 无；关联和主体资格由命令流程另行校验。
    pub fn retain_settlement(&mut self, existing: &PartyId, has_contract: bool) {
        if has_contract {
            self.settlement_party_id.get_or_insert_with(|| existing.clone());
        } else if let Some(terms) = self.no_contract_terms.as_mut() {
            terms.settlement_party_id.get_or_insert_with(|| existing.clone());
        }
    }

    /// 规范化并组装无合同草稿；主体名称来自服务器。
    ///
    /// # 参数
    /// * `name` / `settlement_name` - 服务器读取的客户及结算主体当前法定名称
    /// # 返回
    /// 返回规范化后的完整无合同销售草稿。
    /// # 错误
    /// 缺少或存在非法付款与开票条款、行为空时拒绝。
    pub fn into_no_contract_draft(
        self,
        name: String,
        settlement_name: String,
    ) -> Result<SalesOrderDraftRequest> {
        let editable = self;
        let terms = editable
            .no_contract_terms
            .ok_or_else(|| Error::ValidationError("无合同销售单必须填写付款与开票条款".into()))?;
        terms.validate()?;
        let draft = SalesOrderDraftRequest {
            editor_user_id: editable.editor_user_id,
            customer_name: name,
            contract_no: None,
            requested_contract_revision_id: None,
            settlement_party_name: Some(settlement_name),
            payment_term_code: terms.payment_term_code,
            payment_term_name: terms.payment_term_name,
            invoice_type: terms.invoice_type,
            tax_point: terms.tax_point,
            project_name: editable.project_name,
            business_remark: editable.business_remark,
            voucher_category_sku_id: editable.voucher_category_sku_id,
            voucher_expiry_at: editable.voucher_expiry_at,
            receivable_due_date: editable.receivable_due_date,
            lines: editable.lines,
        };
        draft.validate()?;
        Ok(draft)
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn editable() -> SalesOrderEditableDraftRequest {
        serde_json::from_value(json!({
            "editor_user_id": "actor-1", "requested_contract_revision_id": null,
            "no_contract_terms": {"payment_term_code": "NET_30", "payment_term_name": "月结30天", "invoice_type": "增值税专用发票", "tax_point": "13"},
            "project_name": null, "business_remark": null, "voucher_category_sku_id": null,
            "voucher_expiry_at": null, "receivable_due_date": null,
            "lines": [{"line_no": 1, "line_type": "GOODS_SERVICE", "sales_tax_rate": "0.13", "item_name_snapshot": "测试商品", "spec_snapshot": null, "unit_snapshot": "件", "goods": {"sku_id": "sku-1", "sku_revision_id": "sku-revision-1", "welfare_scenario": null, "service_region": null, "fulfillment_due_at": 1800000000, "quantity": "1", "base_unit_code": "件", "unit_price_gross": "100"}, "voucher": null}]
        })).unwrap()
    }

    #[test]
    fn freezes_authoritative_customer_name_and_selected_commercial_terms() {
        let draft = editable().into_no_contract_draft("客户有限公司".into(), "结算有限公司".into()).unwrap();
        assert_eq!(draft.customer_name, "客户有限公司");
        assert_eq!(draft.settlement_party_name.as_deref(), Some("结算有限公司"));
        assert!(draft.contract_no.is_none());
        assert!(draft.requested_contract_revision_id.is_none());
        assert_eq!(draft.payment_term_code, "NET_30");
        assert_eq!(draft.invoice_type, "增值税专用发票");
        assert_eq!(draft.tax_point, "13");
        assert_eq!(draft.lines.len(), 1);
    }

    #[test]
    fn preserves_confirmed_invoice_and_tax_instead_of_defaulting() {
        let mut input = editable();
        let terms = input.no_contract_terms.as_mut().unwrap();
        terms.invoice_type = "增值税普通发票".into();
        terms.tax_point = "6".into();
        let draft = input.into_no_contract_draft("客户".into(), "第三方结算公司".into()).unwrap();
        assert_eq!(draft.invoice_type, "增值税普通发票");
        assert_eq!(draft.tax_point, "6");
        assert_eq!(draft.settlement_party_name.as_deref(), Some("第三方结算公司"));
    }

    #[test]
    fn requires_nonblank_commercial_terms() {
        let mut draft = editable();
        draft.no_contract_terms = None;
        assert!(draft.into_no_contract_draft("客户".into(), "结算".into()).is_err());
        let mut draft = editable();
        draft.no_contract_terms.as_mut().unwrap().tax_point = " ".into();
        assert!(draft.into_no_contract_draft("客户".into(), "结算".into()).is_err());
    }

    #[test]
    fn editing_keeps_saved_settlement_when_omitted_and_honors_explicit_choice() {
        let original = PartyId::new("our-company");
        let selected = PartyId::new("other-company");
        for has_contract in [true, false] {
            let mut input = editable();
            input.retain_settlement(&original, has_contract);
            if has_contract {
                assert_eq!(input.settlement_party_id, Some(original.clone()));
                input.settlement_party_id = Some(selected.clone());
            } else {
                assert_eq!(
                    input.no_contract_terms.as_ref().unwrap().settlement_party_id,
                    Some(original.clone())
                );
                input.no_contract_terms.as_mut().unwrap().settlement_party_id = Some(selected.clone());
            }
            input.retain_settlement(&original, has_contract);
            let actual = if has_contract {
                input.settlement_party_id
            } else {
                input.no_contract_terms.unwrap().settlement_party_id
            };
            assert_eq!(actual, Some(selected.clone()));
        }
    }

    #[test]
    fn legacy_commands_omit_new_settlement_field_when_serialized() {
        let input = editable();
        assert!(input.settlement_party_id.is_none());
        let wire = serde_json::to_value(input).unwrap();
        assert!(wire.get("settlement_party_id").is_none());
    }
}
