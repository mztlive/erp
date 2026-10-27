//! API 批量选品的公司售价规则；来源采购报价始终只读。

use erp_core::money::{Amount, Quantity, UnitPrice};
use rust_decimal::{Decimal, RoundingStrategy};
use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 公司售价的计算方法；比例按百分数输入，不能当作毛利率。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "method", content = "value", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum PriceRule {
    #[default]
    Unset,
    Percentage(UnitPrice),
    Add(Amount),
    Fixed(Amount),
}

impl PriceRule {
    /// 验证公司售价规则，不读取或改写采购报价。
    /// # 参数
    /// 无。
    /// # 返回
    /// 参数为非负定点数时成功。
    /// # 错误
    /// 负比例或负金额时拒绝。
    pub fn validate(&self) -> Result<()> {
        let value = match self {
            Self::Unset => return Ok(()),
            Self::Percentage(value) => value.to_decimal(),
            Self::Add(value) | Self::Fixed(value) => value.to_decimal(),
        };
        if value.is_sign_negative() {
            return Err(Error::ValidationError("加价比例和售价金额不能为负数".into()));
        }
        Ok(())
    }

    /// 按本次来源价计算公司售价，最终仅舍入一次。
    /// # 参数
    /// `source` 是本次核验的接口采购报价，缺失与零价分别处理。
    /// # 返回
    /// 两位小数公司售价；未设置规则返回空。
    /// # 错误
    /// 缺少必要报价、负数或计算溢出时拒绝。
    pub fn calculate(&self, source: Option<UnitPrice>) -> Result<Option<Amount>> {
        self.validate()?;
        if let Self::Unset = self {
            return Ok(None);
        }
        if let Self::Fixed(value) = self {
            return Ok(Some(*value));
        }
        let source = quote(source)?;
        let value = match self {
            Self::Unset => return Ok(None),
            Self::Fixed(value) => return Ok(Some(*value)),
            Self::Add(value) => source.checked_add(value.to_decimal()),
            Self::Percentage(rate) => {
                let factor = rate
                    .to_decimal()
                    .checked_div(Decimal::ONE_HUNDRED)
                    .and_then(|ratio| ratio.checked_add(Decimal::ONE));
                factor.and_then(|factor| source.checked_mul(factor))
            },
        };
        let value = value.ok_or_else(|| Error::ValidationError("批量售价计算超出金额范围".into()))?;
        // 批量选品合同 §5.1 明确采用非负数半入；不改变财务金额的银行家舍入。
        Ok(Some(Amount::try_from(value.round_dp_with_strategy(2, RoundingStrategy::MidpointAwayFromZero))?))
    }
}

fn quote(source: Option<UnitPrice>) -> Result<Decimal> {
    let value = source.ok_or_else(|| Error::ValidationError("暂无接口采购报价，不能计算售价".into()))?;
    if value.to_decimal().is_sign_negative() {
        return Err(Error::ValidationError("接口采购报价不能为负数".into()));
    }
    Ok(value.to_decimal())
}

/// 本次公司销售参考价配置；同价模式明确联动最终代发价。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum BatchPricing {
    DropshipOnly { dropship: PriceRule },
    Same { rule: PriceRule, minimum_quantity: Option<Quantity> },
    Separate { dropship: PriceRule, bulk: PriceRule, minimum_quantity: Option<Quantity> },
}

impl Default for BatchPricing {
    fn default() -> Self {
        Self::Same { rule: PriceRule::Unset, minimum_quantity: None }
    }
}

/// 单字段值的来源，明确清空不得与尚未填写混淆。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ValueOrigin {
    #[default]
    Unset,
    Source,
    Rule,
    Manual,
    Cleared,
}

/// 规则应用时的覆盖范围；已有公司 SKU 不进入此操作。
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FillPolicy {
    pub overwrite: bool,
    pub preserve_manual: bool,
}

impl Default for FillPolicy {
    fn default() -> Self {
        Self { overwrite: false, preserve_manual: true }
    }
}

impl FillPolicy {
    /// 判断本批草稿字段能否接受批量写入。
    /// # 参数
    /// `origin` 为已保存的字段值来源。
    /// # 返回
    /// 仅空值或明确允许覆盖的值返回真。
    /// # 错误
    /// 不返回错误。
    pub fn allows(self, origin: ValueOrigin) -> bool {
        match origin {
            ValueOrigin::Unset => true,
            ValueOrigin::Manual | ValueOrigin::Cleared => self.overwrite && !self.preserve_manual,
            ValueOrigin::Source | ValueOrigin::Rule => self.overwrite,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn percentage_uses_source_and_rounds_half_up_once() {
        let rule = PriceRule::Percentage("15".parse().unwrap());
        for (source, expected) in [("100", "115"), ("80", "92"), ("10.10", "11.62"), ("0", "0")] {
            let source = Some(source.parse().unwrap());
            assert_eq!(rule.calculate(source).unwrap(), Some(expected.parse().unwrap()));
            assert_eq!(rule.calculate(source).unwrap(), Some(expected.parse().unwrap()));
        }
        let half = PriceRule::Percentage("0".parse().unwrap());
        assert_eq!(half.calculate(Some("1.005".parse().unwrap())).unwrap(), Some("1.01".parse().unwrap()));
    }

    #[test]
    fn missing_quote_zero_fixed_and_unset_remain_distinct() {
        assert!(PriceRule::Add("10".parse().unwrap()).calculate(None).is_err());
        assert!(PriceRule::Percentage("15".parse().unwrap()).calculate(None).is_err());
        assert_eq!(
            PriceRule::Add("10".parse().unwrap()).calculate(Some("0".parse().unwrap())).unwrap(),
            Some("10".parse().unwrap())
        );
        assert_eq!(
            PriceRule::Fixed("99".parse().unwrap()).calculate(None).unwrap(),
            Some("99".parse().unwrap())
        );
        assert_eq!(PriceRule::Unset.calculate(None).unwrap(), None);
    }

    #[test]
    fn negative_parameters_precision_and_overflow_fail() {
        for rule in [
            PriceRule::Percentage("-1".parse().unwrap()),
            PriceRule::Add("-1".parse().unwrap()),
            PriceRule::Fixed("-1".parse().unwrap()),
        ] {
            assert!(rule.calculate(Some("100".parse().unwrap())).is_err());
        }
        assert!(serde_json::from_str::<PriceRule>(r#"{"method":"PERCENTAGE","value":"0.00001"}"#).is_err());
        assert!(serde_json::from_str::<PriceRule>(r#"{"method":"FIXED","value":99}"#).is_err());
        let max = UnitPrice::try_from(Decimal::MAX).unwrap();
        assert!(PriceRule::Percentage(max).calculate(Some(max)).is_err());
        assert!(PriceRule::Add("1".parse().unwrap()).calculate(Some(max)).is_err());
    }

    #[test]
    fn fill_policy_protects_manual_and_explicit_clear() {
        for origin in [ValueOrigin::Source, ValueOrigin::Rule, ValueOrigin::Manual, ValueOrigin::Cleared] {
            assert!(!FillPolicy::default().allows(origin));
        }
        assert!(FillPolicy::default().allows(ValueOrigin::Unset));
        let overwrite = FillPolicy { overwrite: true, preserve_manual: true };
        assert!(overwrite.allows(ValueOrigin::Rule));
        assert!(!overwrite.allows(ValueOrigin::Manual));
        assert!(!overwrite.allows(ValueOrigin::Cleared));
        assert!(FillPolicy { overwrite: true, preserve_manual: false }.allows(ValueOrigin::Cleared));
    }
}
