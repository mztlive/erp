//! 公司 SKU 按销售数量选择含税参考价的纯规则。

use erp_core::money::{Amount, Quantity};

/// 按数量选择销售参考价所需的公司 SKU 价格事实。
///
/// 供应商集采起订量与成本不参与本规则。缺省公司集采起订数量时使用一件代发价。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkuSalesPrices {
    /// 公司一件代发含税销售参考价；保留历史字段名。
    pub sales_visible_price_gross: Option<Amount>,
    /// 公司集采含税销售参考价。
    pub bulk_price_gross: Option<Amount>,
    /// 公司集采价起订数量。
    pub bulk_min_quantity: Option<Quantity>,
}

impl SkuSalesPrices {
    /// 按销售数量选择一件代发价或集采价。
    ///
    /// # 参数
    /// * `quantity` - 当前销售明细数量
    ///
    /// # 返回
    /// 数量达到正数集采起订数量且已维护集采价时返回集采价；其他情况返回一件代发价。
    /// 一件代发价缺失且未满足集采条件时返回 `None`，零金额保持为已维护价格。
    ///
    /// # 错误
    /// 无；数量与价格的输入合法性由拥有领域的写入边界校验。
    pub fn reference_price(&self, quantity: Quantity) -> Option<Amount> {
        let qualifies = self
            .bulk_min_quantity
            .is_some_and(|minimum| minimum.to_decimal() > 0.into() && quantity >= minimum);
        if qualifies && self.bulk_price_gross.is_some() {
            self.bulk_price_gross
        } else {
            self.sales_visible_price_gross
        }
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    /// 带小数的销售数量在达到阈值时切换集采价。
    #[test]
    fn selects_bulk_price_at_and_above_minimum() {
        let prices = SkuSalesPrices {
            sales_visible_price_gross: Some(Amount::from_str("99.90").unwrap()),
            bulk_price_gross: Some(Amount::from_str("80.00").unwrap()),
            bulk_min_quantity: Some(Quantity::from_str("10.500000").unwrap()),
        };
        assert_eq!(
            prices.reference_price(Quantity::from_str("10.499999").unwrap()),
            prices.sales_visible_price_gross
        );
        assert_eq!(prices.reference_price(Quantity::from_str("10.500000").unwrap()), prices.bulk_price_gross);
        assert_eq!(prices.reference_price(Quantity::from_str("10.500001").unwrap()), prices.bulk_price_gross);
    }

    /// 缺省起订量、未维护集采价或无效历史阈值时使用一件代发价。
    #[test]
    fn falls_back_to_dropship_when_bulk_conditions_are_missing() {
        let quantity = Quantity::from_str("100.000000").unwrap();
        let prices = SkuSalesPrices {
            sales_visible_price_gross: Some(Amount::from_str("99.90").unwrap()),
            bulk_price_gross: Some(Amount::from_str("80.00").unwrap()),
            bulk_min_quantity: None,
        };
        assert_eq!(prices.reference_price(quantity), prices.sales_visible_price_gross);
        for minimum in ["0.000000", "-1.000000"] {
            let invalid =
                SkuSalesPrices { bulk_min_quantity: Some(Quantity::from_str(minimum).unwrap()), ..prices };
            assert_eq!(invalid.reference_price(quantity), prices.sales_visible_price_gross);
        }
        let missing_bulk =
            SkuSalesPrices { bulk_price_gross: None, bulk_min_quantity: Some(quantity), ..prices };
        assert_eq!(missing_bulk.reference_price(quantity), prices.sales_visible_price_gross);
    }

    /// 零元集采价是有效的独立价格，缺省价格不从其他字段推导。
    #[test]
    fn preserves_zero_price_and_missing_dropship_price() {
        let prices = SkuSalesPrices {
            sales_visible_price_gross: None,
            bulk_price_gross: Some(Amount::from_str("0.00").unwrap()),
            bulk_min_quantity: Some(Quantity::from_str("10.000000").unwrap()),
        };
        assert_eq!(prices.reference_price(Quantity::from_str("9.000000").unwrap()), None);
        assert_eq!(prices.reference_price(Quantity::from_str("10.000000").unwrap()), prices.bulk_price_gross);
    }
}
