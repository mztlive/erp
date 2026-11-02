//! 册内客户售价与准备时商品原价的只读对比。

use erp_core::money::{Amount, round_to_cent};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// 原价与相对原价的累计调整百分比；缺少可信原价时均为空。
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionPriceComparison {
    /// 准备时引用的不可变 SKU 修订售价；套餐为成员原价之和。
    pub original_price: Option<Amount>,
    /// 百分数，10 表示加价 10%，负数表示降价。
    pub markup_percent: Option<Amount>,
}

impl SelectionPriceComparison {
    /// 对比客户售价和已核对修订的原价。
    ///
    /// # 参数
    /// `price` 为当前客户售价，`original` 为原价；无原价或非正原价不得推算。
    ///
    /// # 返回
    /// 原价及保留两位小数的累计百分比；比例超出可表示范围时保留原价、比例为空。
    ///
    /// # 错误
    /// 不返回错误，不修改任何销售价格。
    pub fn new(price: Amount, original: Option<Amount>) -> Self {
        let original_price = original.filter(|value| *value > Amount::zero());
        let markup_percent = original_price.and_then(|original| {
            price
                .to_decimal()
                .checked_div(original.to_decimal())
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
    fn compares_current_price_with_original_including_repeated_markup_and_discount() {
        for (price, expected) in [("110", "10"), ("121", "21"), ("100", "0"), ("90", "-10")] {
            let comparison =
                SelectionPriceComparison::new(price.parse().unwrap(), Some("100".parse().unwrap()));
            assert_eq!(comparison.original_price, Some("100".parse().unwrap()));
            assert_eq!(comparison.markup_percent, Some(expected.parse().unwrap()));
        }
        let rounded = SelectionPriceComparison::new("1".parse().unwrap(), Some("3".parse().unwrap()));
        assert_eq!(rounded.markup_percent, Some("-66.67".parse().unwrap()));
    }

    #[test]
    fn missing_zero_or_negative_original_is_never_inferred_from_customer_price() {
        for original in [None, Some("0".parse().unwrap()), Some("-1".parse().unwrap())] {
            assert_eq!(
                SelectionPriceComparison::new("100".parse().unwrap(), original),
                SelectionPriceComparison::default()
            );
        }
        let huge = Amount::try_from(Decimal::MAX).unwrap();
        let comparison = SelectionPriceComparison::new(huge, Some("0.01".parse().unwrap()));
        assert_eq!(comparison.original_price, Some("0.01".parse().unwrap()));
        assert!(comparison.markup_percent.is_none());
    }
}
