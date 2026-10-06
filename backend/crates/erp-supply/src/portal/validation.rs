//! 门户包装证据与单位数量精度的纯供给规则。
use erp_core::common::time::Instant;
use rust_decimal::Decimal;

use crate::dto::supplier_offering::SupplierOfferingTermsWrite;
use crate::entity::supplier_offering::write_data::{
    parse_minimum_order_quantity, parse_optional_quantity, parse_unit_price,
};
use crate::{Error, Result};
/// 校验原始可供填报时间，不设置未经配置的过期阈值。
/// # 参数
/// 供应商实际填报时间及服务器接收时间。
/// # 返回
/// 历史或当前时间成功。
/// # 错误
/// 负值或未来时间拒绝。
pub fn validate_portal_reported_at(reported_at: Instant, received_at: Instant) -> Result<()> {
    if reported_at.unix_secs() < 0 || reported_at > received_at {
        return Err(Error::ValidationError("可供实际填报时间无效或位于未来".into()));
    }
    Ok(())
}
/// 校验供应商包装原始单位价，不自动换算为基础单位报价。
/// # 参数
/// `raw` 为箱、包等供应商原始计价单位的价格证据。
/// # 返回
/// 合法非负、最多四位小数的单价成功。
/// # 错误
/// 负值、非法字符串或超出单价精度拒绝。
pub fn validate_portal_packaging_price(raw: &str) -> Result<()> {
    let price = parse_unit_price(raw, "原始包装单位价")?;
    if price.to_decimal().is_sign_negative() {
        return Err(Error::ValidationError("原始包装单位价不能为负".into()));
    }
    Ok(())
}
/// 校验已确认基础单位下的起订量及可供数量精度。
/// # 参数
/// `quantity_scale` 来自当前有效公司SKU基础单位；不得由供应商任填。
/// # 返回
/// 不发生静默舍入的合法数量成功。
/// # 错误
/// 非正起订量、负可供数量或超出单位精度拒绝。
pub fn validate_portal_quantities(
    terms: &SupplierOfferingTermsWrite,
    available_quantity: Option<&str>,
    quantity_scale: u8,
) -> Result<()> {
    let minimum = parse_minimum_order_quantity(&terms.bulk_minimum_order_quantity)?;
    if quantity_scale > 6
        || minimum.to_decimal() <= Decimal::ZERO
        || minimum.to_decimal().normalize().scale() > u32::from(quantity_scale)
    {
        return Err(Error::ValidationError("集采起订量与基础单位数量精度不一致".into()));
    }
    validate_portal_available_quantity(available_quantity, quantity_scale)
}
/// 校验当前基础单位下的可供数量，不改变供应商填报值。
/// # 参数
/// 数量原值及从当前有效公司SKU基础单位读取的数量精度。
/// # 返回
/// 非负且无需舍入的数量成功；未填数量保持未知。
/// # 错误
/// 非法数量、负值、无效精度或超出单位精度时拒绝。
pub fn validate_portal_available_quantity(quantity: Option<&str>, quantity_scale: u8) -> Result<()> {
    if quantity_scale > 6 {
        return Err(Error::ValidationError("基础单位数量精度无效".into()));
    }
    if let Some(quantity) = parse_optional_quantity(quantity)?
        && (quantity.to_decimal().is_sign_negative()
            || quantity.to_decimal().normalize().scale() > u32::from(quantity_scale))
    {
        return Err(Error::ValidationError("可供数量与基础单位数量精度不一致".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_portal_available_quantity;

    #[test]
    fn availability_quantity_preserves_unknown_zero_and_base_unit_precision() {
        validate_portal_available_quantity(None, 0).unwrap();
        validate_portal_available_quantity(Some("0"), 0).unwrap();
        validate_portal_available_quantity(Some("1.200"), 1).unwrap();
        assert!(validate_portal_available_quantity(Some("1.21"), 1).is_err());
        assert!(validate_portal_available_quantity(Some("-1"), 6).is_err());
        assert!(validate_portal_available_quantity(Some("invalid"), 6).is_err());
        assert!(validate_portal_available_quantity(None, 7).is_err());
    }
}
