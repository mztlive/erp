//! 供给数量必须遵守基础单位精度，整数单位不得拆成小数。
use erp_core::money::Quantity;

/// 验证供给数量与基础单位的最小计量粒度一致。
///
/// # 参数
/// `quantity` 为本次分配数量，`scale` 来自单位主数据，`unit` 用于错误文案。
/// # 返回
/// 正数且小数位不超过配置时返回成功。
/// # 错误
/// 单位精度缺失或非法、数量非正或粒度不符时返回业务校验错误。
pub fn ensure_sourcing_quantity(quantity: Quantity, scale: Option<u8>, unit: &str) -> crate::Result<()> {
    let scale = scale
        .filter(|value| *value <= 6)
        .ok_or_else(|| crate::Error::ValidationError(format!("{unit}的数量精度未配置，请检查计量单位")))?;
    let decimal = quantity.to_decimal().normalize();
    if decimal <= rust_decimal::Decimal::ZERO || decimal.scale() > u32::from(scale) {
        let rule = match scale {
            0 => "必须为正整数".to_string(),
            _ => format!("必须大于 0，且最多 {scale} 位小数"),
        };
        return Err(crate::Error::ValidationError(format!("{unit}的分配数量{rule}")));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    /// 整盒拒绝半盒，尾随零不改变单位粒度。
    #[test]
    fn whole_units_reject_fractional_allocations() {
        assert!(ensure_sourcing_quantity(Quantity::from_str("0.5").unwrap(), Some(0), "盒").is_err());
        assert!(ensure_sourcing_quantity(Quantity::from_str("1.000000").unwrap(), Some(0), "盒").is_ok());
    }

    /// 可分割单位遵守各自精度，缺失配置不得按通用六位小数放行。
    #[test]
    fn fractional_units_obey_configured_scale() {
        assert!(ensure_sourcing_quantity(Quantity::from_str("0.25").unwrap(), Some(2), "千克").is_ok());
        assert!(ensure_sourcing_quantity(Quantity::from_str("0.001").unwrap(), Some(2), "千克").is_err());
        assert!(ensure_sourcing_quantity(Quantity::from_str("1").unwrap(), None, "盒").is_err());
    }
}
