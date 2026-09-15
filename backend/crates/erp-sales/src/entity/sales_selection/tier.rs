//! 套餐形态档位规则。

use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::limits::{
    PACKAGE_SKU_MAX, PACKAGE_SKU_MIN, TIER_NAME_MAX_LEN, TIER_PACKAGE_MAX, TIER_PACKAGE_MIN,
};

/// 档位创建数据。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TierRule {
    /// 册内稳定档位身份。
    pub tier_id: String,
    /// 名称，如「100 元档」。
    pub name: String,
    /// 目标金额，含税，必须大于 0。
    pub target_amount: Amount,
    /// 容差，含税，必须显式填写且大于或等于 0。
    pub tolerance: Amount,
    /// 本档希望生成多少个套餐。
    pub expected_count: u32,
    /// 每个套餐包含多少个独立 SKU。
    pub sku_count: u32,
}

impl TierRule {
    /// 规范化并校验一档规则。
    ///
    /// # 参数
    /// * `self` - 原始档位
    ///
    /// # 返回
    /// 返回合法档位。
    ///
    /// # 错误
    /// 名称、金额、数量或 SKU 数不合规时拒绝。
    pub fn normalize(self) -> Result<Self> {
        let name = normalize_required_text(self.name, "档位名称不能为空", TIER_NAME_MAX_LEN, "档位名称过长")?;
        let tier_id = self.tier_id.trim();
        if tier_id.is_empty() {
            return Err(Error::from("档位身份不能为空"));
        }
        if self.target_amount <= Amount::zero() {
            return Err(Error::from("档位目标金额必须大于 0"));
        }
        if self.tolerance < Amount::zero() {
            return Err(Error::from("档位容差必须大于或等于 0"));
        }
        if !(TIER_PACKAGE_MIN..=TIER_PACKAGE_MAX).contains(&self.expected_count) {
            return Err(Error::from("每档期望套餐数必须在 1 到 20 之间"));
        }
        if !(PACKAGE_SKU_MIN..=PACKAGE_SKU_MAX).contains(&self.sku_count) {
            return Err(Error::from("每套餐独立 SKU 数必须在 2 到 8 之间"));
        }
        let _ = super::pricing::try_add(self.target_amount, self.tolerance)?;
        Ok(Self {
            tier_id: tier_id.to_string(),
            name,
            target_amount: self.target_amount,
            tolerance: self.tolerance,
            expected_count: self.expected_count,
            sku_count: self.sku_count,
        })
    }
}

/// 校验档位列表。
///
/// # 参数
/// * `tiers` - 创建顺序的档位
///
/// # 返回
/// 返回规范化后的档位列表。
///
/// # 错误
/// 数量超限、名称重复或单档非法时拒绝。
pub fn normalize_tiers(tiers: Vec<TierRule>) -> Result<Vec<TierRule>> {
    if tiers.len() < super::limits::TIER_COUNT_MIN || tiers.len() > super::limits::TIER_COUNT_MAX {
        return Err(Error::from("套餐形态必须填写 1 到 10 个档位"));
    }
    let mut normalized = Vec::with_capacity(tiers.len());
    let mut names = std::collections::BTreeSet::new();
    let mut ids = std::collections::BTreeSet::new();
    for tier in tiers {
        let tier = tier.normalize()?;
        if !names.insert(tier.name.clone()) {
            return Err(Error::from("档位名称在册内不得重复"));
        }
        if !ids.insert(tier.tier_id.clone()) {
            return Err(Error::from("档位身份在册内不得重复"));
        }
        normalized.push(tier);
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::money::Amount;

    use super::{TierRule, normalize_tiers};

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn tier(name: &str) -> TierRule {
        TierRule {
            tier_id: format!("tier-{name}"),
            name: name.into(),
            target_amount: amount("100.00"),
            tolerance: amount("5.00"),
            expected_count: 3,
            sku_count: 3,
        }
    }

    #[test]
    fn rejects_duplicate_names() {
        let error = normalize_tiers(vec![tier("100 元档"), tier("100 元档")]).unwrap_err();
        assert!(error.to_string().contains("不得重复"));
    }

    #[test]
    fn rejects_zero_target() {
        let mut rule = tier("A");
        rule.target_amount = Amount::zero();
        assert!(rule.normalize().is_err());
    }
}
