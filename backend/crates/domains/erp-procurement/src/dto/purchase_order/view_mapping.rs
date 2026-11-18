//! 采购提交行、版本行与汇总视图映射。

use super::{InquiryQuoteSourceView, PurchaseOrderLineView, TotalsView};
use crate::entity::purchase_order::{
    PurchaseOrderRevision, PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine,
};

// 提交行与版本行共用字段映射；历史销售提交行只存在于提交行。
macro_rules! purchase_line_view {
    ($line:ident, $sales_submission_line_id:expr) => {
        PurchaseOrderLineView {
            inquiry_quote_source: $line
                .supplier_offering_source
                .as_ref()
                .and_then(|source| source.inquiry_quote.as_ref())
                .map(|source| InquiryQuoteSourceView::from_source(source, $line.quantity)),
            delivery_allocations: $line.delivery_allocations.clone(),
            line_id: $line.base.id.clone(),
            line_no: $line.line_no,
            line_type: $line.line_type,
            procurement_confirmation_line_id: $line
                .procurement_confirmation_line_id
                .as_ref()
                .map(ToString::to_string),
            sku_id: $line.sku_id.as_ref().map(ToString::to_string),
            sku_revision_id: $line.sku_revision_id.as_ref().map(ToString::to_string),
            product_name: $line.product_name_snapshot.clone(),
            specification: $line.specification_snapshot.clone(),
            quantity: $line.quantity.map(|q| q.to_string()),
            base_unit_code: $line.base_unit_code.clone(),
            unit_cost_gross: $line.unit_cost_gross.map(|v| v.to_string()),
            input_tax_rate: $line.input_tax_rate.map(|v| v.to_string()),
            gross_amount: $line.gross_amount.to_string(),
            net_amount: $line.net_amount.to_string(),
            tax_amount: $line.tax_amount.to_string(),
            expected_delivery_date: $line.expected_delivery_date.map(|d| d.to_string()),
            sales_order_line_id: $line.sales_order_line_id.as_ref().map(ToString::to_string),
            sales_order_revision_line_id: $line
                .sales_order_revision_line_id
                .as_ref()
                .map(ToString::to_string),
            sales_order_submission_line_id: $sales_submission_line_id,
            allocated_quantity: $line.allocated_quantity.map(|q| q.to_string()),
            expense: $line.expense.clone(),
            expense_occurrence: None,
        }
    };
}

impl PurchaseOrderLineView {
    /// 从不可变采购版本行生成展示视图。
    ///
    /// # 参数
    /// * `line` - 已保存的采购版本行
    ///
    /// # 返回
    /// 保留版本行身份、精确数值字符串与销售版本关联的视图；历史销售提交行为空。
    ///
    /// # 错误
    /// 无；只投影已类型化实体，不读取当前主数据。
    pub fn from_revision(line: &PurchaseOrderRevisionLine) -> Self {
        purchase_line_view!(line, None)
    }

    /// 从采购冻结提交行生成展示视图。
    ///
    /// # 参数
    /// * `line` - 已保存的采购提交行
    ///
    /// # 返回
    /// 保留提交行身份、精确数值字符串与冻结销售关联的视图。
    ///
    /// # 错误
    /// 无；只投影已类型化实体，不读取当前主数据。
    pub fn from_submission(line: &PurchaseOrderSubmissionLine) -> Self {
        purchase_line_view!(line, line.sales_order_submission_line_id.as_ref().map(ToString::to_string))
    }
}

impl TotalsView {
    /// 从不可变采购版本生成表头汇总视图。
    ///
    /// # 参数
    /// * `revision` - 已保存的采购版本
    ///
    /// # 返回
    /// 返回版本内保存的含税、未税和税额字符串，不重新聚合行金额。
    ///
    /// # 错误
    /// 无。
    pub fn from_revision(revision: &PurchaseOrderRevision) -> Self {
        Self {
            gross: revision.gross_amount.to_string(),
            net: revision.net_amount.to_string(),
            tax: revision.tax_amount.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        PurchaseOrderId, PurchaseOrderRevisionId, PurchaseOrderRevisionLineId, PurchaseOrderSubmissionId,
        PurchaseOrderSubmissionLineId, SupplierCommercialProfileRevisionId,
    };
    use erp_core::money::Amount;
    use serde_json::json;

    use super::{PurchaseOrderLineView, TotalsView};
    use crate::dto::purchase_order::SavePurchaseOrderLine;
    use crate::entity::purchase_order::{
        PaymentSubjectSnapshot, PaymentTermSnapshot, PurchaseLineType, PurchaseOrderRevision,
        PurchaseOrderRevisionData, PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine, SupplierSnapshot,
    };
    use crate::entity::test_support::payment_term;

    fn submission_line(line_type: PurchaseLineType) -> PurchaseOrderSubmissionLine {
        let is_item = line_type == PurchaseLineType::ItemService;
        let item = |value: &str| is_item.then(|| value.to_string());
        let request = SavePurchaseOrderLine {
            delivery_allocations: None,
            line_type,
            procurement_confirmation_line_id: item("pcl-1"),
            sku_id: item("sku-1"),
            sku_revision_id: item("skur-1"),
            product_name: item("慰问礼包"),
            specification: item("500g×2"),
            quantity: item("3.000000"),
            base_unit_code: item("箱"),
            unit_cost_gross: item("9.9900"),
            input_tax_rate: Some("0.130000".to_string()),
            expected_delivery_date: is_item.then(|| "2026-08-06".to_string()),
            sales_order_line_id: item("sol-1"),
            sales_order_revision_line_id: item("sorl-1"),
            sales_order_submission_line_id: item("ssl-1"),
            allocated_quantity: item("3.000000"),
            gross_amount: (!is_item).then(|| "100.00".to_string()),
            expense: None,
        };
        let data = request
            .to_line_input()
            .unwrap()
            .into_submission_line_data(PurchaseOrderSubmissionId::new("sub-1"), 2)
            .unwrap();
        let mut line =
            PurchaseOrderSubmissionLine::new(PurchaseOrderSubmissionLineId::new("subl-1"), data).unwrap();
        line.base = BaseModel::fake();
        line.base.id = "subl-1".to_string();
        line
    }

    #[test]
    fn submission_and_revision_views_keep_snapshot_fields_and_distinct_line_identity() {
        let line = submission_line(PurchaseLineType::ItemService);
        let view = PurchaseOrderLineView::from_submission(&line);
        assert_eq!(
            serde_json::to_value(&view).unwrap(),
            json!({
                "line_id": "subl-1", "delivery_allocations": [], "line_no": 2, "line_type": "ITEM_SERVICE",
                "procurement_confirmation_line_id": "pcl-1", "sku_id": "sku-1",
                "sku_revision_id": "skur-1", "product_name": "慰问礼包", "specification": "500g×2",
                "quantity": "3.000000", "base_unit_code": "箱", "unit_cost_gross": "9.9900",
                "input_tax_rate": "0.130000", "gross_amount": "29.97", "net_amount": "26.07",
                "tax_amount": "3.90", "expected_delivery_date": "2026-08-06",
                "sales_order_line_id": "sol-1", "sales_order_revision_line_id": "sorl-1",
                "sales_order_submission_line_id": "ssl-1", "allocated_quantity": "3.000000"
            })
        );

        let revision = PurchaseOrderRevisionLine::from_submission_line(
            PurchaseOrderRevisionLineId::new("revl-1"),
            PurchaseOrderRevisionId::new("rev-1"),
            &line,
        )
        .unwrap();
        let mut expected = view;
        expected.line_id = "revl-1".to_string();
        expected.sales_order_submission_line_id = None;
        assert_eq!(PurchaseOrderLineView::from_revision(&revision), expected);
    }

    #[test]
    fn logistics_view_preserves_absent_item_and_sales_fields() {
        let line = submission_line(PurchaseLineType::LogisticsFee);
        let view = PurchaseOrderLineView::from_submission(&line);
        assert_eq!(view.line_type, PurchaseLineType::LogisticsFee);
        assert_eq!(view.gross_amount, "100.00");
        assert!(view.sku_id.is_none());
        assert!(view.quantity.is_none());
        assert!(view.unit_cost_gross.is_none());
        assert!(view.product_name.is_none());
        assert!(view.sales_order_line_id.is_none());
        assert!(view.sales_order_revision_line_id.is_none());
        assert!(view.sales_order_submission_line_id.is_none());
        assert!(view.allocated_quantity.is_none());
    }

    #[test]
    fn revision_totals_preserve_frozen_values_including_zero() {
        for (gross, net, tax) in [("0.00", "0.00", "0.00"), ("29.97", "26.07", "3.90")] {
            let mut revision = PurchaseOrderRevision::new(
                PurchaseOrderRevisionId::new("rev-1"),
                PurchaseOrderRevisionData {
                    payment_subject: PaymentSubjectSnapshot::new(
                        "party-1".to_string().into(),
                        "我方公司".into(),
                        "so-1".to_string().into(),
                        "sales-rev-1".to_string().into(),
                    )
                    .unwrap(),
                    purchase_order_id: PurchaseOrderId::new("po-1"),
                    revision_no: 1,
                    supplier_revision_id: SupplierCommercialProfileRevisionId::new("supplier-rev-1"),
                    supplier_snapshot: SupplierSnapshot::new("供应商".to_string()).unwrap(),
                    payment_term_snapshot: PaymentTermSnapshot::new(
                        "CASH_ON_APPROVAL".to_string(),
                        false,
                        None,
                        None,
                        payment_term,
                    )
                    .unwrap(),
                    gross_amount: Amount::from_str(gross).unwrap(),
                    net_amount: Amount::from_str(net).unwrap(),
                    tax_amount: Amount::from_str(tax).unwrap(),
                    effective_at: Instant::from_unix_secs(1_800_000_000),
                },
            )
            .unwrap();
            revision.base = BaseModel::fake();
            assert_eq!(
                TotalsView::from_revision(&revision),
                TotalsView { gross: gross.to_string(), net: net.to_string(), tax: tax.to_string() }
            );
        }
    }
}
