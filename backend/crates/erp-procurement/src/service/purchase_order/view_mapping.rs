//! 采购提交行、版本行与汇总视图映射。

use crate::dto::purchase_order::{PurchaseOrderLineView, TotalsView};
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

impl PurchaseLineViewSource for PurchaseOrderRevisionLine {
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
}

impl PurchaseLineViewSource for PurchaseOrderSubmissionLine {
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

/// 从实体构造采购版本行的视图。
///
/// # 参数
/// * `line` - 采购版本行实体
///
/// # 返回
/// 返回响应视图。
pub fn revision_line_to_view(line: &PurchaseOrderRevisionLine) -> PurchaseOrderLineView {
    line_to_view(line)
}

/// 从实体构造提交行视图.
///
/// # 参数
/// * `line` - 采购提交行实体
///
/// # 返回
/// 返回响应视图。
pub fn submission_line_to_view(line: &PurchaseOrderSubmissionLine) -> PurchaseOrderLineView {
    line_to_view(line)
}

/// 从实体构造采购版本汇总。
///
/// # 参数
/// * `revision` - 采购版本实体
///
/// # 返回
/// 返回汇总视图。
pub fn revision_totals(revision: &PurchaseOrderRevision) -> TotalsView {
    TotalsView {
        gross: revision.gross_amount.to_string(),
        net: revision.net_amount.to_string(),
        tax: revision.tax_amount.to_string(),
    }
}
