//! 采购行公共字段与校验（数据模型 §6.6 行字典）。
//!
//! 提交行（`purchase_order_submission_line`）、版本行（`purchase_order_revision_line`）
//! 与变更提交行的商品/数量/单位/成本/进项税/预计交期字段组完全相同，本模块提供
//! 唯一校验实现，三个行实体复用，避免同义分叉。
//!
//! 逐行金额守恒按 §4.2 铁律 1：`gross = net + tax` 精确成立，只能经
//! [`crate::money::line_amounts`] 或 [`crate::money::round_to_cent`] 舍入。

use crate::errors::{Error, Result};
use crate::ids::{ProcurementConfirmationLineId, SkuId};
use crate::money::{line_amounts, round_to_cent, Amount, Quantity, Rate, UnitPrice};
use crate::purchase_order::types::PurchaseLineType;
use crate::validation::normalize_optional_text;

/// 商品名称快照最大长度。
pub(crate) const PRODUCT_NAME_MAX_LEN: usize = 256;
/// 规格快照最大长度。
pub(crate) const SPECIFICATION_MAX_LEN: usize = 512;
/// 单位代码最大长度。
pub(crate) const BASE_UNIT_MAX_LEN: usize = 64;

/// 一行采购明细的共享字段组（提交行/版本行/变更提交行共用，§6.6 行字典）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PurchaseLineFields {
    /// 行类型。
    pub line_type: PurchaseLineType,
    /// 商品/服务行对应的采购二次确认分行；物流费用行为空。
    pub procurement_confirmation_line_id: Option<ProcurementConfirmationLineId>,
    /// 商品行引用的 SKU；物流费用行为空。
    pub sku_id: Option<SkuId>,
    /// 商品名称快照；物流费用行为空。
    pub product_name_snapshot: Option<String>,
    /// 规格快照；物流费用行为空。
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

/// 校验一行采购明细（提交行/版本行/变更提交行共用）。
///
/// 按行类型强制字段归属（§6.6）：
/// - 商品/服务成本行：`sku_id`、商品名称、单位、数量（>0）、含税单价、进项税率必填；
/// - 物流费用行：`sku_id`、商品名称、单位、数量、含税单价全部为空，
///   与商品成本分开计税（进项税率必填），金额直接入账；
/// - 两种行都必须满足金额三元组守恒：商品行按 `line_amounts(单价, 数量, 税率)`
///   复算，物流费用行按 `gross − round(gross × 税率) = net` 复算（§4.2 铁律 4）。
///
/// # 参数
/// * `fields` - 一行采购明细的共享字段组
///
/// # 返回
/// 校验通过返回 `Ok(())`。
///
/// # 错误
/// 字段归属与行类型不符、数量/单价/税率越界或金额三元组不守恒时返回错误。
pub(crate) fn validate_purchase_line(fields: &PurchaseLineFields) -> Result<()> {
    match fields.line_type {
        PurchaseLineType::ItemService => validate_item_service_line(fields),
        PurchaseLineType::LogisticsFee => validate_logistics_fee_line(fields),
    }
}

/// 采购行创建数据的共享访问接口（提交行/版本行/变更提交行共用，§6.6 行字典）。
///
/// 行实体只需实现本接口即可复用 [`normalize_and_validate_line`] 的完整校验；
/// 有销售分配字段的行类型覆写 [`ensure_allocation`](Self::ensure_allocation)。
pub(crate) trait PurchaseLineDataRef {
    /// 返回行类型。
    fn line_type(&self) -> PurchaseLineType;
    /// 返回采购二次确认分行引用。
    fn procurement_confirmation_line_id(&self) -> &Option<ProcurementConfirmationLineId>;
    /// 返回 SKU 引用。
    fn sku_id(&self) -> &Option<SkuId>;
    /// 返回商品名称快照。
    fn product_name_snapshot(&self) -> &Option<String>;
    /// 返回规格快照。
    fn specification_snapshot(&self) -> &Option<String>;
    /// 返回基础单位数量。
    fn quantity(&self) -> Option<Quantity>;
    /// 返回单位代码。
    fn base_unit_code(&self) -> &Option<String>;
    /// 返回含税采购单价。
    fn unit_cost_gross(&self) -> Option<UnitPrice>;
    /// 返回含税行金额。
    fn gross_amount(&self) -> Amount;
    /// 返回不含税行金额。
    fn net_amount(&self) -> Amount;
    /// 返回税额。
    fn tax_amount(&self) -> Amount;
    /// 返回进项税率。
    fn input_tax_rate(&self) -> Option<Rate>;

    /// 校验销售分配归属；无销售分配字段的行类型保持默认空实现。
    ///
    /// # 返回
    /// 校验通过返回 `Ok(())`。
    ///
    /// # 错误
    /// 分配字段与行类型不符时返回错误。
    fn ensure_allocation(&self) -> Result<()> {
        Ok(())
    }
}

/// 规范化并校验一行采购明细。
///
/// 完成快照文本规范化（商品名称/规格/单位），按行类型校验字段归属与金额三元组
/// 守恒（§6.6），并执行行类型的销售分配归属校验。
///
/// # 参数
/// * `data` - 采购行创建数据（实现 [`PurchaseLineDataRef`]）
///
/// # 返回
/// 返回 `(商品名称快照, 规格快照, 单位代码)` 三元组。
///
/// # 错误
/// 快照超长、字段归属与行类型不符、数量/单价/税率越界或金额三元组不守恒时
/// 返回错误。
pub(crate) fn normalize_and_validate_line<D: PurchaseLineDataRef>(
    data: &D,
) -> Result<(Option<String>, Option<String>, Option<String>)> {
    let product_name = normalize_optional_text(
        data.product_name_snapshot().clone(),
        "商品名称快照",
        PRODUCT_NAME_MAX_LEN,
    )?;
    let specification = normalize_optional_text(
        data.specification_snapshot().clone(),
        "规格快照",
        SPECIFICATION_MAX_LEN,
    )?;
    let base_unit_code = normalize_optional_text(data.base_unit_code().clone(), "单位", BASE_UNIT_MAX_LEN)?;
    validate_purchase_line(&PurchaseLineFields {
        line_type: data.line_type(),
        procurement_confirmation_line_id: data.procurement_confirmation_line_id().clone(),
        sku_id: data.sku_id().clone(),
        product_name_snapshot: product_name.clone(),
        specification_snapshot: specification.clone(),
        quantity: data.quantity(),
        base_unit_code: base_unit_code.clone(),
        unit_cost_gross: data.unit_cost_gross(),
        gross_amount: data.gross_amount(),
        net_amount: data.net_amount(),
        tax_amount: data.tax_amount(),
        input_tax_rate: data.input_tax_rate(),
    })?;
    data.ensure_allocation()?;
    Ok((product_name, specification, base_unit_code))
}

/// 校验商品/服务成本行。
///
/// # 参数
/// * `fields` - 一行采购明细的共享字段组
///
/// # 错误
/// 必填字段缺失、数量非正或金额三元组不守恒时返回错误。
fn validate_item_service_line(fields: &PurchaseLineFields) -> Result<()> {
    if fields.procurement_confirmation_line_id.is_none() {
        return Err(Error::from("商品/服务行必须引用采购二次确认分行"));
    }
    let sku_id = fields.sku_id.as_ref().ok_or("商品/服务行必须引用 SKU")?;
    ensure_snapshot_present(fields.product_name_snapshot.as_deref(), "商品名称")?;
    ensure_snapshot_present(fields.specification_snapshot.as_deref(), "规格")?;
    ensure_snapshot_present(fields.base_unit_code.as_deref(), "单位")?;
    let quantity = fields.quantity.ok_or("商品/服务行数量不能为空")?;
    if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
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
    ensure_amount_triple(fields, expected, sku_id.as_ref())?;
    Ok(())
}

/// 校验物流费用行。
///
/// # 参数
/// * `fields` - 一行采购明细的共享字段组
///
/// # 错误
/// 携带商品行字段、缺少税率或金额三元组不守恒时返回错误。
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

/// 断言必填快照字段非空。
///
/// # 参数
/// * `value` - 快照文本
/// * `label` - 字段说明
///
/// # 错误
/// 快照缺失或为空时返回错误。
fn ensure_snapshot_present(value: Option<&str>, label: &str) -> Result<()> {
    if value.is_none() || value.unwrap().is_empty() {
        return Err(Error::from(format!("商品/服务行必须保存{label}快照")));
    }
    Ok(())
}

/// 断言金额三元组与复算结果一致。
///
/// # 参数
/// * `fields` - 一行采购明细的共享字段组
/// * `expected` - 按 §4.2 铁律 1 复算的 `(gross, net, tax)`
/// * `context` - 错误提示中的行上下文（如 SKU）
///
/// # 错误
/// 任一分量不一致时返回错误。
fn ensure_amount_triple(
    fields: &PurchaseLineFields,
    expected: (Amount, Amount, Amount),
    context: &str,
) -> Result<()> {
    if fields.gross_amount != expected.0 || fields.net_amount != expected.1 || fields.tax_amount != expected.2
    {
        return Err(Error::from(format!(
            "行金额三元组与含税单价×数量×税率复算不一致（{context}）"
        )));
    }
    Ok(())
}
