//! 采购创建依据查询参数与行视图。

use erp_core::delivery_policy::DeliveryTerm;
use erp_procurement::entity::purchase_order::{PurchaseDeliveryAllocation, PurchaseInquiryQuoteSource};

use super::*;

/// 某收货地区的配送判定。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DeliveryRegionTermView {
    /// 收货地区代码。
    pub region_code: String,
    /// 判定。
    pub term: DeliveryTerm,
}

/// 某目标仓库的集采配送判定。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct WarehouseDeliveryTermView {
    /// 仓库主键。
    pub warehouse_id: String,
    /// 仓库所在地区完整名称。
    pub region_name: String,
    /// 判定。
    pub term: DeliveryTerm,
}

/// 依据行的配送判定视图。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct CreationBasisDeliveryView {
    /// 供给生效配送模板名称；未设置模板时为空。
    pub delivery_template_name: Option<String>,
    /// 直发方案按销售收货地区的判定。
    pub delivery_terms: Vec<DeliveryRegionTermView>,
    /// 集采入仓方案按启用仓库的判定。
    pub warehouse_delivery_terms: Vec<WarehouseDeliveryTermView>,
    /// 集采直发要求的供应商集采起订量。
    pub bulk_minimum_order_quantity: Option<String>,
    /// 本行供给主键，用于按供给汇总起订量。
    pub supplier_offering_id: Option<String>,
}

/// 采购创建依据查询参数。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CreationBasisListParams {
    /// 可选来源销售单；从销售详情或工作台进入时用于收窄范围。
    pub sales_order_id: Option<String>,
    /// 可选供给分配任务；提供时必须是当前账号拥有的开放任务。
    pub work_item_id: Option<String>,
}

/// 采购创建依据行视图（销售当前版本行 + 当前采购剩余量）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisLineView {
    /// 本方案的配送判定。
    #[serde(flatten)]
    pub delivery: CreationBasisDeliveryView,
    /// 当前销售冻结的客户交付要求。
    pub delivery_allocations: Vec<PurchaseDeliveryAllocation>,
    /// 当前采购已承担的地址数量。
    pub assigned_delivery_allocations: Vec<PurchaseDeliveryAllocation>,
    /// 基础单位允许数量小数位；缺失时客户端不得推断可拆分精度。
    pub quantity_scale: Option<u8>,
    /// 销售稳定行身份。
    pub sales_order_line_id: String,
    /// 销售当前版本行身份。
    pub sales_order_revision_line_id: String,
    /// 销售当前版本内的业务行号。
    pub sales_line_no: u32,
    /// 确认供应商。
    pub supplier_id: String,
    /// 销售当前版本目标数量。
    pub sales_quantity: String,
    /// 当前采购覆盖数量。
    pub covered_quantity: String,
    /// 当前采购剩余数量。
    pub remaining_quantity: String,
    /// 本供应商当前最大可创建数量，等于 `min(remaining, available)`。
    pub max_create_quantity: String,
    /// 兼容展示字段，值等于 `max_create_quantity`。
    pub confirmed_quantity: String,
    /// 最新含税成本。
    pub latest_cost_gross: String,
    /// 进项税率。
    pub input_tax_rate: String,
    /// 采购预计交付日预填值（`YYYY-MM-DD`）。
    pub expected_delivery_date: String,
    /// 销售对客户承诺的最晚交付日（`YYYY-MM-DD`）。
    pub sales_delivery_deadline: String,
    /// 商品名称快照（销售提交行侧联查，缺失时为空）。
    pub product_name: Option<String>,
    /// 规格快照。
    pub specification: Option<String>,
    /// 销售单位快照。
    pub unit: Option<String>,
    /// 含税行金额（按确认数量与成本逐行舍入）。
    pub gross_amount: String,
}

impl CreationBasisLineView {
    /// 设置本行授权销售交付要求及已承担数量。
    /// # 参数
    /// `source` 为销售安排，`assigned` 为当前有效采购安排。
    /// # 返回
    /// 返回补齐地址资料的依据行。
    /// # 错误
    /// 无。
    pub fn with_delivery(
        mut self,
        source: Vec<PurchaseDeliveryAllocation>,
        assigned: Vec<PurchaseDeliveryAllocation>,
    ) -> Self {
        self.delivery_allocations = source;
        self.assigned_delivery_allocations = assigned;
        self
    }
    /// 构造采购创建依据行视图。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 销售稳定行身份
    /// * `sales_order_revision_line_id` - 销售当前版本行身份
    /// * `supplier_id` - 确认供应商
    /// * `sales_quantity` - 销售当前版本目标数量
    /// * `covered_quantity` - 当前采购覆盖数量
    /// * `remaining_quantity` - 当前采购剩余数量
    /// * `max_create_quantity` - 本供应商当前最大可创建数量
    /// * `confirmed_quantity` - 兼容展示数量
    /// * `latest_cost_gross` - 最新含税成本
    /// * `input_tax_rate` - 进项税率
    /// * `expected_delivery_date` - 采购预计交付日预填值
    /// * `sales_delivery_deadline` - 销售对客户承诺的最晚交付日
    /// * `gross_amount` - 含税行金额
    ///
    /// # 返回
    /// 返回行号为零、可选快照全空的行视图。
    ///
    /// # 错误
    /// 无。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        sales_order_line_id: String,
        sales_order_revision_line_id: String,
        supplier_id: String,
        sales_quantity: String,
        covered_quantity: String,
        remaining_quantity: String,
        max_create_quantity: String,
        confirmed_quantity: String,
        latest_cost_gross: String,
        input_tax_rate: String,
        expected_delivery_date: String,
        sales_delivery_deadline: String,
        gross_amount: String,
    ) -> Self {
        Self {
            delivery: CreationBasisDeliveryView::default(),
            quantity_scale: None,
            delivery_allocations: Vec::new(),
            assigned_delivery_allocations: Vec::new(),
            sales_order_line_id,
            sales_order_revision_line_id,
            sales_line_no: 0,
            supplier_id,
            sales_quantity,
            covered_quantity,
            remaining_quantity,
            max_create_quantity,
            confirmed_quantity,
            latest_cost_gross,
            input_tax_rate,
            expected_delivery_date,
            sales_delivery_deadline,
            product_name: None,
            specification: None,
            unit: None,
            gross_amount,
        }
    }

    /// 设置本方案的配送判定。
    ///
    /// # 参数
    /// * `delivery` - 模板名称、逐地区与逐仓库判定及起订量
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_delivery_view(mut self, delivery: CreationBasisDeliveryView) -> Self {
        self.delivery = delivery;
        self
    }

    /// 设置基础单位允许数量小数位。
    ///
    /// # 参数
    /// * `scale` - 数量小数位
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_quantity_scale(mut self, scale: Option<u8>) -> Self {
        self.quantity_scale = scale;
        self
    }

    /// 设置销售当前版本内的业务行号。
    ///
    /// # 参数
    /// * `line_no` - 业务行号
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_sales_line_no(mut self, line_no: u32) -> Self {
        self.sales_line_no = line_no;
        self
    }

    /// 设置商品名称快照。
    ///
    /// # 参数
    /// * `name` - 商品名称快照
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_product_name(mut self, name: Option<String>) -> Self {
        self.product_name = name;
        self
    }

    /// 设置规格快照。
    ///
    /// # 参数
    /// * `specification` - 规格快照
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_specification(mut self, specification: Option<String>) -> Self {
        self.specification = specification;
        self
    }

    /// 设置销售单位快照。
    ///
    /// # 参数
    /// * `unit` - 销售单位快照
    ///
    /// # 返回
    /// 返回更新后的行视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_unit(mut self, unit: Option<String>) -> Self {
        self.unit = unit;
        self
    }
}

/// 仅采购依据视图可见的明确专项来源，不向销售轻预览透出。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct InquiryBasisSourceView {
    pub inquiry_id: String,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub inquiry_line_id: String,
    pub quote_revision_id: String,
    pub quote_revision_no: u32,
    pub valid_to: Option<String>,
    pub expected_delivery_date: String,
}
/// 采购创建依据视图（已生效销售单 × 合格供给供应商，§7.4 选源建单入口）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inquiry_reselection_required: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inquiry_source: Option<InquiryBasisSourceView>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommendation_reason: Option<String>,
    /// 当前账号拥有且冻结本依据销售行范围的开放供给分配任务。
    pub work_item_id: String,
    /// 精确创建依据（任务、销售当前版本、供应商、采购类型、付款条件、履约责任及剩余量指纹）。
    pub basis_id: String,
    /// 供给来源。
    pub source_type: SupplySourceType,
    /// 被确认的销售单。
    pub sales_order_id: String,
    /// 销售单号。
    pub sales_order_no: String,
    /// 销售当前版本冻结的客户名称。
    pub customer_name: String,
    /// 销售当前版本冻结的合同编号；无合同时为空。
    pub contract_no: Option<String>,
    /// 销售单负责人展示名；账号档案缺失时为空。
    pub sales_owner_name: Option<String>,
    /// 目标销售当前版本。
    pub sales_order_revision_id: String,
    /// 供应商。
    pub supplier_id: String,
    /// 供应商名称。
    pub supplier_name: String,
    /// 现有库存来源的余额主键；采购来源为空。
    pub stock_balance_id: Option<String>,
    /// 现有库存来源的仓库主键；采购来源为空。
    pub warehouse_id: Option<String>,
    /// 现有库存来源的仓库名称；采购来源为空。
    pub warehouse_name: Option<String>,
    /// 来源当前总可供量；库存为余额可用量，未声明上限的采购来源为空。
    pub source_available_quantity: Option<String>,
    /// 采购类型（由商品稳定业务类型确定）。
    pub purchase_type: String,
    /// 履约责任（由采购在商品类型允许范围内选择）。
    pub fulfillment_responsibility: String,
    /// 付款条件（供应商商业资料快照，缺省 `NET-30`；不含经营类目）。
    pub payment_term_code: String,
    /// 供应商经营类目；未登记时为空。
    pub business_category: Option<String>,
    /// 可拆入本单的已确认分行。
    pub lines: Vec<CreationBasisLineView>,
    /// 含税行汇总（只汇总已舍入行金额）。
    pub estimated_gross: String,
    /// 价格依据：`DROPSHIP` 或 `BULK`；现有库存为空。
    pub price_basis: Option<String>,
}

impl CreationBasisView {
    /// 构造采购创建依据视图。
    ///
    /// # 参数
    /// * `work_item_id` - 冻结本依据责任范围的开放供给分配任务
    /// * `basis_id` - 精确创建依据
    /// * `sales_order_id` - 被确认的销售单
    /// * `sales_order_no` - 销售单号
    /// * `customer_name` - 销售当前版本冻结的客户名称
    /// * `sales_order_revision_id` - 目标销售当前版本
    /// * `supplier_id` - 供应商
    /// * `supplier_name` - 供应商名称
    /// * `purchase_type` - 采购类型
    /// * `fulfillment_responsibility` - 履约责任
    /// * `payment_term_code` - 付款条件
    /// * `estimated_gross` - 含税行汇总
    ///
    /// # 返回
    /// 返回采购来源、行与快照全空的依据视图。
    ///
    /// # 错误
    /// 无。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        work_item_id: String,
        basis_id: String,
        sales_order_id: String,
        sales_order_no: String,
        customer_name: String,
        sales_order_revision_id: String,
        supplier_id: String,
        supplier_name: String,
        purchase_type: String,
        fulfillment_responsibility: String,
        payment_term_code: String,
        estimated_gross: String,
    ) -> Self {
        Self {
            inquiry_source: None,
            inquiry_reselection_required: None,
            recommendation_reason: None,
            work_item_id,
            basis_id,
            source_type: SupplySourceType::Purchase,
            sales_order_id,
            sales_order_no,
            customer_name,
            contract_no: None,
            sales_owner_name: None,
            sales_order_revision_id,
            supplier_id,
            supplier_name,
            stock_balance_id: None,
            warehouse_id: None,
            warehouse_name: None,
            source_available_quantity: None,
            purchase_type,
            fulfillment_responsibility,
            payment_term_code,
            business_category: None,
            lines: Vec::new(),
            estimated_gross,
            price_basis: None,
        }
    }

    /// 标记当前询价来源采用通用条款时必须给出改选原因。
    /// # 参数
    /// `required`由正式销售来源与当前候选共同确定。
    /// # 返回
    /// 返回明确采购确认要求的依据。
    /// # 错误
    /// 无。
    pub fn with_inquiry_reselection_required(mut self, required: bool) -> Self {
        self.inquiry_reselection_required = required.then_some(true);
        self
    }

    /// 标记沿用询价的精确专项版本，供采购显式识别并确认条件。
    /// # 参数
    /// `source`为服务器已核验的专项采用链，通用供给为空。
    /// # 返回
    /// 返回补齐来源标记的采购依据。
    /// # 错误
    /// 无。
    pub fn with_inquiry_source(mut self, source: Option<&PurchaseInquiryQuoteSource>) -> Self {
        self.inquiry_source = source.map(|source| InquiryBasisSourceView {
            inquiry_id: source.inquiry_id.clone(),
            requirement_version: source.requirement_version,
            feedback_round: source.feedback_round,
            inquiry_line_id: source.inquiry_line_id.clone(),
            quote_revision_id: source.quote_revision_id.clone(),
            quote_revision_no: source.quote_revision_no,
            valid_to: source.valid_to.map(|date| date.to_string()),
            expected_delivery_date: source.expected_delivery_date.to_string(),
        });
        self.recommendation_reason =
            source.map(|_| "沿用已接受询价的专项条款；仍须确认当前数量、地址、交期及供应资格".into());
        self
    }

    /// 设置价格依据。
    ///
    /// # 参数
    /// * `price_basis` - 稳定代码
    ///
    /// # 返回
    /// 返回更新后的依据视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_price_basis(mut self, price_basis: &str) -> Self {
        self.price_basis = Some(price_basis.to_string());
        self
    }

    /// 设置供给来源。
    ///
    /// # 参数
    /// * `source_type` - 供给来源
    ///
    /// # 返回
    /// 返回更新后的依据视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_source_type(mut self, source_type: SupplySourceType) -> Self {
        self.source_type = source_type;
        self
    }

    /// 设置合同编号。
    ///
    /// # 参数
    /// * `contract_no` - 销售当前版本冻结的合同编号
    ///
    /// # 返回
    /// 返回更新后的依据视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_contract_no(mut self, contract_no: Option<String>) -> Self {
        self.contract_no = contract_no;
        self
    }

    /// 设置销售单负责人展示名。
    ///
    /// # 参数
    /// * `name` - 销售单负责人展示名
    ///
    /// # 返回
    /// 返回更新后的依据视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_sales_owner_name(mut self, name: Option<String>) -> Self {
        self.sales_owner_name = name;
        self
    }

    /// 设置现有库存来源字段。
    ///
    /// # 参数
    /// * `stock_balance_id` - 余额主键
    /// * `warehouse_id` - 仓库主键
    /// * `warehouse_name` - 仓库名称
    /// * `source_available_quantity` - 来源当前总可供量
    ///
    /// # 返回
    /// 返回更新后的依据视图。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 现有库存依据的供给来源必须是现有库存。`new` 的默认值是采购，不改的话
    /// 客户端会把余额方案当成采购单。
    pub fn with_stock_source(
        mut self,
        stock_balance_id: Option<String>,
        warehouse_id: Option<String>,
        warehouse_name: Option<String>,
        source_available_quantity: Option<String>,
    ) -> Self {
        self.source_type = SupplySourceType::ExistingStock;
        self.stock_balance_id = stock_balance_id;
        self.warehouse_id = warehouse_id;
        self.warehouse_name = warehouse_name;
        self.source_available_quantity = source_available_quantity;
        self
    }

    /// 设置供应商经营类目。
    ///
    /// # 参数
    /// * `category` - 供应商经营类目
    ///
    /// # 返回
    /// 返回更新后的依据视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_business_category(mut self, category: Option<String>) -> Self {
        self.business_category = category;
        self
    }

    /// 设置可拆入本单的已确认分行。
    ///
    /// # 参数
    /// * `lines` - 已确认分行
    ///
    /// # 返回
    /// 返回更新后的依据视图。
    ///
    /// # 错误
    /// 无。
    pub fn with_lines(mut self, lines: Vec<CreationBasisLineView>) -> Self {
        self.lines = lines;
        self
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn bare_basis(supplier_name: &str) -> CreationBasisView {
        CreationBasisView::new(
            "task-1".to_string(),
            "basis-1".to_string(),
            "so-1".to_string(),
            "SO-1".to_string(),
            "客户".to_string(),
            "rev-1".to_string(),
            String::new(),
            supplier_name.to_string(),
            "PHYSICAL".to_string(),
            "WAREHOUSE".to_string(),
            String::new(),
            "0".to_string(),
        )
    }

    #[test]
    fn new_basis_stays_purchase_until_stock_source_is_set() {
        let purchase = bare_basis("狮峰茶叶");
        assert_eq!(purchase.source_type, SupplySourceType::Purchase);
        assert_eq!(purchase.stock_balance_id, None);

        let stock = bare_basis("现有库存 · 北京通州仓").with_stock_source(
            Some("balance-1".to_string()),
            Some("warehouse-1".to_string()),
            Some("北京通州仓".to_string()),
            Some("10".to_string()),
        );
        assert_eq!(stock.source_type, SupplySourceType::ExistingStock);
        assert_eq!(stock.stock_balance_id.as_deref(), Some("balance-1"));
        assert_eq!(stock.warehouse_name.as_deref(), Some("北京通州仓"));
        assert_eq!(stock.source_available_quantity.as_deref(), Some("10"));
        let json = serde_json::to_value(stock).expect("库存依据可序列化");
        assert_eq!(json["source_type"], "EXISTING_STOCK");
    }
}
