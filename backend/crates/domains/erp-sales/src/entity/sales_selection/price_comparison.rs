//! 册内客户售价与初始客户售价的只读对比。

use erp_core::money::{Amount, round_to_cent};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::SelectionReferencePrices;

/// 集采价兼容字段与相对初始客户售价的累计调整百分比。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionPriceComparison {
    /// 集采价兼容字段；套餐为成员集采价之和，仅返回正值。
    pub original_price: Option<Amount>,
    /// 相对初始客户售价的百分数，10 表示加价 10%，负数表示降价。
    pub markup_percent: Option<Amount>,
}

impl SelectionPriceComparison {
    /// 从冻结或按原 SKU 修订补齐的一件代发价取得初始客户售价。
    /// 计算结果保留两位小数；初始客户售价缺失、非正或比例超出表示范围时，比例为空。
    pub fn new(price: Amount, references: Option<&SelectionReferencePrices>) -> Self {
        let original_price =
            references.and_then(|prices| prices.bulk_price_gross).filter(|value| *value > Amount::zero());
        let initial_price =
            references.and_then(|prices| prices.dropship_price_gross).filter(|value| *value > Amount::zero());
        let markup_percent = initial_price.and_then(|initial| {
            price
                .to_decimal()
                .checked_div(initial.to_decimal())
                .and_then(|ratio| ratio.checked_sub(Decimal::ONE))
                .and_then(|ratio| ratio.checked_mul(Decimal::from(100)))
                .and_then(|percent| Amount::try_from(round_to_cent(percent)).ok())
        });
        Self { original_price, markup_percent }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_current_price_with_initial_including_repeated_markup_and_discount() {
        let references = SelectionReferencePrices {
            dropship_price_gross: Some("100".parse().unwrap()),
            bulk_price_gross: Some("80".parse().unwrap()),
            ..Default::default()
        };
        for (price, expected) in [("110", "10"), ("121", "21"), ("100", "0"), ("90", "-10")] {
            let comparison = SelectionPriceComparison::new(price.parse().unwrap(), Some(&references));
            assert_eq!(comparison.original_price, references.bulk_price_gross);
            assert_eq!(comparison.markup_percent, Some(expected.parse().unwrap()));
        }

        let references = SelectionReferencePrices {
            dropship_price_gross: Some("3".parse().unwrap()),
            ..Default::default()
        };
        let rounded = SelectionPriceComparison::new("1".parse().unwrap(), Some(&references));
        assert!(rounded.original_price.is_none());
        assert_eq!(rounded.markup_percent, Some("-66.67".parse().unwrap()));
    }

    #[test]
    fn unavailable_initial_price_leaves_markup_empty() {
        let mut references =
            SelectionReferencePrices { bulk_price_gross: Some("80".parse().unwrap()), ..Default::default() };
        for initial in [None, Some("0".parse().unwrap()), Some("-1".parse().unwrap())] {
            references.dropship_price_gross = initial;
            let comparison = SelectionPriceComparison::new("100".parse().unwrap(), Some(&references));
            assert_eq!(comparison.original_price, references.bulk_price_gross);
            assert!(comparison.markup_percent.is_none());
        }

        references.dropship_price_gross = Some("0.01".parse().unwrap());
        let huge = Amount::try_from(Decimal::MAX).unwrap();
        let comparison = SelectionPriceComparison::new(huge, Some(&references));
        assert_eq!(comparison.original_price, references.bulk_price_gross);
        assert!(comparison.markup_percent.is_none());
    }
}
