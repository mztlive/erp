//! 采购行公共字段与校验（数据模型 §6.6 行字典）。
//!
//! 提交行（`purchase_order_submission_line`）、版本行（`purchase_order_revision_line`）
//! 与变更提交行的商品/数量/单位/成本/进项税/预计交期字段组完全相同，本模块提供
//! 唯一校验实现，三个行实体复用，避免同义分叉。
//!
//! 逐行金额守恒按 §4.2 铁律 1：`gross = net + tax` 精确成立，只能经
//! [`erp_core::money::line_amounts`] 或 [`erp_core::money::round_to_cent`] 舍入。

use erp_core::ids::{ProcurementConfirmationLineId, SkuId};
use erp_core::money::{Amount, Quantity, Rate, UnitPrice, line_amounts, round_to_cent};
use erp_core::validation::normalize_optional_text;
use erp_core::{Error, Result};

use crate::entity::purchase_order::PurchaseExpenseDetail;
use crate::entity::purchase_order::types::PurchaseLineType;

/// 商品名称快照最大长度。
const PRODUCT_NAME_MAX_LEN: usize = 256;
/// 规格快照最大长度。
const SPECIFICATION_MAX_LEN: usize = 512;
/// 单位代码最大长度。
const BASE_UNIT_MAX_LEN: usize = 64;

/// 一行采购明细的共享字段组（提交行/版本行/变更提交行共用，§6.6 行字典）。
pub(super) struct PurchaseLineFields {
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU；物流费用行为空。
    pub sku_id: Option<SkuId>,
    /// 商品名称快照；物流费用行为空。
    pub product_name_snapshot: Option<String>,
    /// 可选规格快照；来源销售行无规格时及物流费用行为空。
    pub specification_snapshot: Option<String>,
    /// 基础单位数量；物流费用行为空。
    pub quantity: Option<Quantity>,
    /// 单位代码；物流费用行为空。
    pub base_unit_code: Option<String>,
    /// 含税采购单价；物流费用行为空。
    pub unit_cost_gross: Option<UnitPrice>,
    /// 含税行金额。
    pub gross_amount: Amount,
    /// 不含税行金额。
    pub net_amount: Amount,
    /// 税额。
    pub tax_amount: Amount,
    /// 进项税率。
    pub input_tax_rate: Option<Rate>,
}

impl PurchaseLineFields {
    /// 先规范化快照，再按行类型校验字段归属与金额守恒。
    ///
    /// # 参数
    /// * `line_no` - 从 1 开始的行号
    /// * `allow_zero_quantity` - 仅变更提交行和版本行允许零数量表示已移除
    ///
    /// # 返回
    /// 校验成功时保留规范化后的快照。
    ///
    /// # 错误
    /// 行号为零、快照超长、字段归属错误或金额不守恒时返回领域错误。
    pub(super) fn normalize_and_validate(&mut self, line_no: u32, allow_zero_quantity: bool) -> Result<()> {
        if line_no == 0 {
            return Err(Error::from("行号必须从 1 开始"));
        }

        self.product_name_snapshot =
            normalize_optional_text(self.product_name_snapshot.take(), "商品名称快照", PRODUCT_NAME_MAX_LEN)?;
        self.specification_snapshot =
            normalize_optional_text(self.specification_snapshot.take(), "规格快照", SPECIFICATION_MAX_LEN)?;
        self.base_unit_code = normalize_optional_text(self.base_unit_code.take(), "单位", BASE_UNIT_MAX_LEN)?;

        match self.line_type {
            PurchaseLineType::ItemService => validate_item_service_line(self, allow_zero_quantity),
            PurchaseLineType::LogisticsFee => validate_logistics_fee_line(self),
        }
    }
}

/// 按行类型校验并规范化费用行内容。
///
/// # 参数
/// * `line_type` - 行类型
/// * `expense` - 行携带的费用内容
///
/// # 返回
/// 费用行返回规范化内容（历史费用行可为空），商品行返回 `None`。
///
/// # 错误
/// 商品行携带费用内容或费用内容非法时返回错误。
pub(super) fn normalize_expense(
    line_type: PurchaseLineType,
    expense: Option<PurchaseExpenseDetail>,
) -> Result<Option<PurchaseExpenseDetail>> {
    match (line_type, expense) {
        (PurchaseLineType::ItemService, Some(_)) => Err(Error::from("商品/服务行不得携带费用内容")),
        (_, Some(detail)) => Ok(Some(detail.normalized()?)),
        (_, None) => Ok(None),
    }
}

/// 校验商品/服务成本行。
///
/// # 参数
/// * `fields` - 一行采购明细的共享字段组
/// * `allow_zero_quantity` - 是否允许零数量
///
/// # 错误
/// 必填字段缺失、数量为负或（不允许零时）为零、含税单价或进项税率为负，或金额三元组不守恒时返回错误。
fn validate_item_service_line(fields: &PurchaseLineFields, allow_zero_quantity: bool) -> Result<()> {
    if fields.procurement_confirmation_line_id.is_none() {
        return Err(Error::from("商品/服务行必须引用采购二次确认分行"));
    }
    let sku_id = fields.sku_id.as_ref().ok_or("商品/服务行必须引用 SKU")?;
    if fields.product_name_snapshot.is_none() {
        return Err(Error::from("商品/服务行必须保存商品名称快照"));
    }
    if fields.base_unit_code.is_none() {
        return Err(Error::from("商品/服务行必须保存单位快照"));
    }

    let quantity = fields.quantity.ok_or("商品/服务行数量不能为空")?;
    if quantity.to_decimal() < rust_decimal::Decimal::ZERO
        || (!allow_zero_quantity && quantity.to_decimal().is_zero())
    {
        return Err(Error::from("商品/服务行数量必须为正"));
    }
    let unit_cost = fields.unit_cost_gross.ok_or("商品/服务行含税单价不能为空")?;
    if unit_cost.to_decimal() < rust_decimal::Decimal::ZERO {
        return Err(Error::from("商品/服务行含税单价不能为负"));
    }
    let tax_rate = fields.input_tax_rate.ok_or("商品/服务行进项税率不能为空")?;
    if tax_rate.to_decimal() < rust_decimal::Decimal::ZERO {
        return Err(Error::from("进项税率不能为负"));
    }

    let expected = line_amounts(unit_cost, quantity, tax_rate);
    if (fields.gross_amount, fields.net_amount, fields.tax_amount) != expected {
        return Err(Error::from(format!(
            "行金额三元组与含税单价×数量×税率复算不一致（{}）",
            sku_id.as_ref()
        )));
    }
    Ok(())
}

/// 校验物流费用行。
///
/// # 参数
/// * `fields` - 一行采购明细的共享字段组
///
/// # 错误
/// 携带商品行字段、缺少进项税率、税率或金额为负，或金额三元组不守恒时返回错误。
fn validate_logistics_fee_line(fields: &PurchaseLineFields) -> Result<()> {
    if fields.procurement_confirmation_line_id.is_some() {
        return Err(Error::from("物流费用行不得引用采购二次确认分行"));
    }
    if fields.sku_id.is_some() {
        return Err(Error::from("物流费用行不得引用 SKU"));
    }
    if fields.product_name_snapshot.is_some() {
        return Err(Error::from("物流费用行不得保存商品名称快照"));
    }
    if fields.quantity.is_some() {
        return Err(Error::from("物流费用行数量必须为空"));
    }
    if fields.base_unit_code.is_some() {
        return Err(Error::from("物流费用行单位必须为空"));
    }
    if fields.unit_cost_gross.is_some() {
        return Err(Error::from("物流费用行不得填写含税单价"));
    }

    let tax_rate = fields.input_tax_rate.ok_or("物流费用行进项税率不能为空")?;
    if tax_rate.to_decimal() < rust_decimal::Decimal::ZERO {
        return Err(Error::from("进项税率不能为负"));
    }
    if fields.gross_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || fields.net_amount.to_decimal() < rust_decimal::Decimal::ZERO
        || fields.tax_amount.to_decimal() < rust_decimal::Decimal::ZERO
    {
        return Err(Error::from("物流费用行金额不能为负"));
    }

    let expected_tax = round_to_cent(fields.gross_amount.to_decimal() * tax_rate.to_decimal());
    let expected_net = fields.gross_amount.to_decimal() - expected_tax;
    if expected_tax != fields.tax_amount.to_decimal() || expected_net != fields.net_amount.to_decimal() {
        return Err(Error::from("物流费用行金额三元组不守恒"));
    }
    Ok(())
}
