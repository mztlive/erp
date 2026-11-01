//! 已加载的工作副本行和冻结提交行按相同展示合同生成视图。

use super::SalesOrderWorkingCopyLineView;
use crate::entity::sales_order::{SalesOrderSubmissionLine, SalesOrderWorkingCopyLine};

// 两种存储形状保持独立，展示字段赋值仅维护一份，避免新增字段时漏改其中一个来源。
macro_rules! stored_line_view {
    ($($method:ident($source:ty)),+ $(,)?) => {
        impl SalesOrderWorkingCopyLineView {
            $(
                /// 从已加载的销售行保留原身份、金额和快照，不读取当前商品资料。
                ///
                /// # 参数
                /// * `line` - 对应版本的工作副本行或不可变提交行
                ///
                /// # 返回
                /// 返回展示视图；参考价由有权限的跨域读取另行补齐。
                ///
                /// # 错误
                /// 不返回错误；不重新校验或计算历史金额。
                pub fn $method(line: $source) -> Self {
                    Self {
                        pricing_mode: line.pricing_mode,
                        delivery_allocations: line.delivery_allocations,
                        reference_prices: None,
                        id: line.base.id,
                        sales_order_line_id: line.sales_order_line_id.to_string(),
                        line_no: line.line_no,
                        line_type: line.line_type,
                        gross_amount: line.gross_amount,
                        net_amount: line.net_amount,
                        tax_amount: line.tax_amount,
                        sales_tax_rate: line.sales_tax_rate,
                        item_name_snapshot: line.item_name_snapshot,
                        spec_snapshot: line.spec_snapshot,
                        unit_snapshot: line.unit_snapshot,
                        sku_id: line.sku_id,
                        sku_revision_id: line.sku_revision_id,
                        welfare_scenario: line.welfare_scenario,
                        procurement_supply_price_gross: line.procurement_supply_price_gross,
                        procurement_price_basis: line.procurement_price_basis,
                        fulfillment_due_at: line.fulfillment_due_at.map(|instant| instant.unix_secs() as u64),
                        quantity: line.quantity,
                        base_unit_code: line.base_unit_code,
                        unit_price_gross: line.unit_price_gross,
                        face_value: line.face_value,
                        card_count: line.card_count,
                        transaction_amount: line.transaction_amount,
                        card_form: line.card_form,
                    }
                }
            )+
        }
    };
}

stored_line_view!(
    from_working_copy_line(SalesOrderWorkingCopyLine),
    from_submission_line(SalesOrderSubmissionLine),
);

#[cfg(test)]
mod tests {
    use erp_core::ids::{
        SalesOrderSubmissionId, SalesOrderSubmissionLineId, SalesOrderWorkingCopyId,
        SalesOrderWorkingCopyLineId,
    };
    use serde_json::json;

    use super::*;
    use crate::entity::sales_order::working_copy_test_support::line_data;
    use crate::entity::sales_order::{
        CardForm, LineType, SalesOrderSubmissionLineData, SalesOrderWorkingCopyLineData, VoucherLineDraft,
    };

    fn views(
        data: SalesOrderWorkingCopyLineData,
    ) -> (SalesOrderWorkingCopyLineView, SalesOrderWorkingCopyLineView) {
        let working = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("working-line"),
            SalesOrderWorkingCopyId::new("working"),
            data,
        )
        .unwrap();
        let submitted = SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("submitted-line"),
            SalesOrderSubmissionId::new("submitted"),
            SalesOrderSubmissionLineData::from_working_copy_line(&working).unwrap(),
        )
        .unwrap();
        (
            SalesOrderWorkingCopyLineView::from_working_copy_line(working),
            SalesOrderWorkingCopyLineView::from_submission_line(submitted),
        )
    }

    #[test]
    fn goods_sources_share_wire_fields_and_preserve_their_own_line_identity() {
        let (working, mut submitted) = views(line_data(2));
        assert_eq!(working.id, "working-line");
        assert_eq!(submitted.id, "submitted-line");
        submitted.id = working.id.clone();
        assert_eq!(submitted, working);
        assert_eq!(
            serde_json::to_value(working).unwrap(),
            json!({
                "id": "working-line",
                "sales_order_line_id": "line-2",
                "line_no": 2,
                "line_type": "GOODS_SERVICE",
                "gross_amount": "29.97",
                "net_amount": "26.07",
                "tax_amount": "3.90",
                "sales_tax_rate": "0.130000",
                "item_name_snapshot": "年货礼盒",
                "spec_snapshot": "10kg",
                "unit_snapshot": "箱",
                "sku_id": "sku-1",
                "sku_revision_id": "skurev-1",
                "welfare_scenario": "ANNUAL_GIFT_BAG",
                "procurement_supply_price_gross": "8.5000",
                "fulfillment_due_at": 1_800_000_000,
                "quantity": "3.000000",
                "base_unit_code": "箱",
                "unit_price_gross": "9.9900",
                "pricing_mode": "MANUAL",
                "face_value": null,
                "card_count": null,
                "transaction_amount": null,
                "card_form": null
            })
        );
    }

    #[test]
    fn submitted_amounts_are_read_from_the_frozen_record_without_recalculation() {
        let working = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("working-line"),
            SalesOrderWorkingCopyId::new("working"),
            line_data(2),
        )
        .unwrap();
        let mut submitted = SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("submitted-line"),
            SalesOrderSubmissionId::new("submitted"),
            SalesOrderSubmissionLineData::from_working_copy_line(&working).unwrap(),
        )
        .unwrap();
        // 历史冻结金额与当前构造规则的结果不同，展示仍须原样读取。
        submitted.net_amount = "27.00".parse().unwrap();
        submitted.tax_amount = "2.97".parse().unwrap();
        let view = SalesOrderWorkingCopyLineView::from_submission_line(submitted);
        assert_eq!(view.gross_amount.to_string(), "29.97");
        assert_eq!(view.net_amount.to_string(), "27.00");
        assert_eq!(view.tax_amount.to_string(), "2.97");
    }

    #[test]
    fn voucher_sources_preserve_amounts_without_goods_or_reference_price_fields() {
        let mut data = line_data(3);
        data.line_type = LineType::Voucher;
        data.goods = None;
        data.voucher = Some(VoucherLineDraft {
            face_value: "100.00".parse().unwrap(),
            card_count: 3,
            unit_price_gross: "90.0000".parse().unwrap(),
            face_value_total: "300.00".parse().unwrap(),
            transaction_amount: "270.00".parse().unwrap(),
            gift_amount: "30.00".parse().unwrap(),
            gift_rate: None,
            card_form: CardForm::Electronic,
        });
        let (working, mut submitted) = views(data);
        submitted.id = working.id.clone();
        assert_eq!(submitted, working);
        let value = serde_json::to_value(working).unwrap();
        assert_eq!(value["line_type"], "VOUCHER");
        assert_eq!(value["face_value"], "100.00");
        assert_eq!(value["card_count"], 3);
        assert_eq!(value["transaction_amount"], "270.00");
        assert_eq!(value["gross_amount"], "270.00");
        assert!(value["quantity"].is_null());
        assert!(value["sku_id"].is_null());
        assert!(value.get("reference_prices").is_none());
        assert!(value.get("delivery_allocations").is_none());
    }
}
