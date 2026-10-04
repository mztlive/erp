//! 采购提交行、版本行与汇总视图映射。

use super::{PurchaseOrderLineView, TotalsView};
use crate::entity::purchase_order::{
    PurchaseLineType, PurchaseOrderRevision, PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine,
};

/// 采购行视图的统一数据源（版本行与提交行共用字段）。
///
/// 两类行实体除提交行多 `sales_order_submission_line_id` 外字段同形；
/// 本 trait 收敛公共字段读取，单个泛型函数承载视图组装。
trait PurchaseLineViewSource {
    /// 行实体主键。
    fn view_line_id(&self) -> String;
    /// 版本内行号。
    fn view_line_no(&self) -> u32;
    /// 行类型。
    fn view_line_type(&self) -> PurchaseLineType;
    /// 采购二次确认分行。
    fn view_procurement_confirmation_line_id(&self) -> Option<String>;
    /// 商品行引用的 SKU。
    fn view_sku_id(&self) -> Option<String>;
    /// 商品行引用的 SKU 版本。
    fn view_sku_revision_id(&self) -> Option<String>;
    /// 商品名称快照。
    fn view_product_name(&self) -> Option<String>;
    /// 规格快照。
    fn view_specification(&self) -> Option<String>;
    /// 基础单位数量。
    fn view_quantity(&self) -> Option<String>;
    /// 单位代码。
    fn view_base_unit_code(&self) -> Option<String>;
    /// 含税采购单价。
    fn view_unit_cost_gross(&self) -> Option<String>;
    /// 进项税率。
    fn view_input_tax_rate(&self) -> Option<String>;
    /// 含税行金额。
    fn view_gross_amount(&self) -> String;
    /// 不含税行金额。
    fn view_net_amount(&self) -> String;
    /// 税额。
    fn view_tax_amount(&self) -> String;
    /// 预计交期。
    fn view_expected_delivery_date(&self) -> Option<String>;
    /// 商品行对应的销售稳定行。
    fn view_sales_order_line_id(&self) -> Option<String>;
    /// 商品行对应的销售当前版本行。
    fn view_sales_order_revision_line_id(&self) -> Option<String>;
    /// 商品行正式分配数量。
    fn view_allocated_quantity(&self) -> Option<String>;
    /// 商品行对应的历史销售提交行；版本行恒为空。
    fn view_sales_order_submission_line_id(&self) -> Option<String> {
        None
    }
}

/// 为同形行实体生成视图字段读取。
macro_rules! common_view_methods {
    () => {
        fn view_line_id(&self) -> String {
            self.base.id.clone()
        }

        fn view_line_no(&self) -> u32 {
            self.line_no
        }

        fn view_line_type(&self) -> PurchaseLineType {
            self.line_type
        }

        fn view_procurement_confirmation_line_id(&self) -> Option<String> {
            self.procurement_confirmation_line_id.as_ref().map(ToString::to_string)
        }

        fn view_sku_id(&self) -> Option<String> {
            self.sku_id.as_ref().map(ToString::to_string)
        }

        fn view_sku_revision_id(&self) -> Option<String> {
            self.sku_revision_id.as_ref().map(ToString::to_string)
        }

        fn view_product_name(&self) -> Option<String> {
            self.product_name_snapshot.clone()
        }

        fn view_specification(&self) -> Option<String> {
            self.specification_snapshot.clone()
        }

        fn view_quantity(&self) -> Option<String> {
            self.quantity.map(|q| q.to_string())
        }

        fn view_base_unit_code(&self) -> Option<String> {
            self.base_unit_code.clone()
        }

        fn view_unit_cost_gross(&self) -> Option<String> {
            self.unit_cost_gross.map(|v| v.to_string())
        }

        fn view_input_tax_rate(&self) -> Option<String> {
            self.input_tax_rate.map(|v| v.to_string())
        }

        fn view_gross_amount(&self) -> String {
            self.gross_amount.to_string()
        }

        fn view_net_amount(&self) -> String {
            self.net_amount.to_string()
        }

        fn view_tax_amount(&self) -> String {
            self.tax_amount.to_string()
        }

        fn view_expected_delivery_date(&self) -> Option<String> {
            self.expected_delivery_date.map(|d| d.to_string())
        }

        fn view_sales_order_line_id(&self) -> Option<String> {
            self.sales_order_line_id.as_ref().map(ToString::to_string)
        }

        fn view_sales_order_revision_line_id(&self) -> Option<String> {
            self.sales_order_revision_line_id.as_ref().map(ToString::to_string)
        }

        fn view_allocated_quantity(&self) -> Option<String> {
            self.allocated_quantity.map(|q| q.to_string())
        }
    };
}

impl PurchaseLineViewSource for PurchaseOrderRevisionLine {
    common_view_methods!();
}

impl PurchaseLineViewSource for PurchaseOrderSubmissionLine {
    common_view_methods!();

    fn view_sales_order_submission_line_id(&self) -> Option<String> {
        self.sales_order_submission_line_id.as_ref().map(ToString::to_string)
    }
}

/// 由统一行数据源组装采购行视图。
fn line_to_view(line: &impl PurchaseLineViewSource) -> PurchaseOrderLineView {
    PurchaseOrderLineView {
        line_id: line.view_line_id(),
        line_no: line.view_line_no(),
        line_type: line.view_line_type(),
        procurement_confirmation_line_id: line.view_procurement_confirmation_line_id(),
        sku_id: line.view_sku_id(),
        sku_revision_id: line.view_sku_revision_id(),
        product_name: line.view_product_name(),
        specification: line.view_specification(),
        quantity: line.view_quantity(),
        base_unit_code: line.view_base_unit_code(),
        unit_cost_gross: line.view_unit_cost_gross(),
        input_tax_rate: line.view_input_tax_rate(),
        gross_amount: line.view_gross_amount(),
        net_amount: line.view_net_amount(),
        tax_amount: line.view_tax_amount(),
        expected_delivery_date: line.view_expected_delivery_date(),
        sales_order_line_id: line.view_sales_order_line_id(),
        sales_order_revision_line_id: line.view_sales_order_revision_line_id(),
        sales_order_submission_line_id: line.view_sales_order_submission_line_id(),
        allocated_quantity: line.view_allocated_quantity(),
    }
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
        line_to_view(line)
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
        line_to_view(line)
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
        PaymentTermSnapshot, PurchaseLineType, PurchaseOrderRevision, PurchaseOrderRevisionData,
        PurchaseOrderRevisionLine, PurchaseOrderSubmissionLine, SupplierSnapshot,
    };
    use crate::entity::test_support::payment_term;

    fn submission_line(line_type: PurchaseLineType) -> PurchaseOrderSubmissionLine {
        let is_item = line_type == PurchaseLineType::ItemService;
        let item = |value: &str| is_item.then(|| value.to_string());
        let request = SavePurchaseOrderLine {
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
                "line_id": "subl-1", "line_no": 2, "line_type": "ITEM_SERVICE",
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
