//! 采购创建依据查询参数与行视图。

use super::*;

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
            quantity_scale: None,
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

/// 采购创建依据视图（已生效销售单 × 合格供给供应商，§7.4 选源建单入口）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CreationBasisView {
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
        }
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
    pub fn with_stock_source(
        mut self,
        stock_balance_id: Option<String>,
        warehouse_id: Option<String>,
        warehouse_name: Option<String>,
        source_available_quantity: Option<String>,
    ) -> Self {
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
