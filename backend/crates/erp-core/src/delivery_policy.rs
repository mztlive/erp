//! 配送政策共享值对象。
//!
//! 供给领域拥有配送模板并以本值对象保存规则；采购与读取模型只消费判定结果。
//! 代发价已包含运费，代发只判定配送或不配送；集采判定包邮、运费另计或不配送。
//! 每种方式先设其他地区的判定，再按省、市或区县列出例外，判定取最具体的一条。

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::region::RegionCode;
use crate::{Error, Result};

/// 每种方式最多的地区例外条数。
pub const MAX_DELIVERY_RULES: usize = 1000;

/// 供货方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryMode {
    /// 一件代发。
    Dropship,
    /// 集采。
    Bulk,
}

impl DeliveryMode {
    /// 返回中文名称。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回"代发"或"集采"。
    ///
    /// # 错误
    /// 无。
    pub fn label(self) -> &'static str {
        match self {
            Self::Dropship => "代发",
            Self::Bulk => "集采",
        }
    }
}

/// 发往某地区的配送判定。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryTerm {
    /// 配送且运费已含在价格中：代发为"配送"，集采为"包邮"。
    FreightIncluded,
    /// 配送，供应商另收运费；只用于集采。
    FreightExtra,
    /// 不配送。
    NotDeliverable,
}

impl DeliveryTerm {
    /// 判断是否可配送。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 包邮或运费另计返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_deliverable(self) -> bool {
        self != Self::NotDeliverable
    }
}

/// 一条地区例外。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRegionRule {
    /// 省、市或区县代码。
    pub region_code: RegionCode,
    /// 该地区的判定。
    pub term: DeliveryTerm,
}

/// 一种供货方式的地区规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryRuleSet {
    /// 其他地区的判定。
    pub default_term: DeliveryTerm,
    /// 地区例外，按代码升序。
    pub rules: Vec<DeliveryRegionRule>,
}

impl DeliveryRuleSet {
    /// 构造全部地区同一判定、没有例外的规则。
    ///
    /// # 参数
    /// * `term` - 全部地区的判定
    ///
    /// # 返回
    /// 返回无例外规则。
    ///
    /// # 错误
    /// 无。
    pub fn uniform(term: DeliveryTerm) -> Self {
        Self { default_term: term, rules: Vec::new() }
    }

    /// 按最具体的地区例外判定，未命中时取其他地区判定。
    ///
    /// # 参数
    /// * `region` - 收货或仓库所在地区
    ///
    /// # 返回
    /// 区县、市、省依次查找，命中即返回该例外的判定。
    ///
    /// # 错误
    /// 无。
    pub fn judge(&self, region: &RegionCode) -> DeliveryTerm {
        region
            .self_and_ancestors()
            .iter()
            .find_map(|code| self.rules.iter().find(|rule| rule.region_code == *code))
            .map_or(self.default_term, |rule| rule.term)
    }

    fn normalized(mut self, mode: DeliveryMode) -> Result<Self> {
        if self.rules.len() > MAX_DELIVERY_RULES {
            return Err(Error::from(format!("{}规则最多 {MAX_DELIVERY_RULES} 个地区", mode.label())));
        }
        ensure_term_allowed(mode, self.default_term)?;
        let mut seen = HashSet::new();
        for rule in &self.rules {
            RegionCode::parse(rule.region_code.as_str())?;
            ensure_term_allowed(mode, rule.term)?;
            if !seen.insert(rule.region_code.clone()) {
                return Err(Error::from(format!(
                    "{}规则中地区 {} 重复",
                    mode.label(),
                    rule.region_code.full_name()
                )));
            }
        }
        self.rules.sort_by(|left, right| left.region_code.cmp(&right.region_code));
        Ok(self)
    }
}

fn ensure_term_allowed(mode: DeliveryMode, term: DeliveryTerm) -> Result<()> {
    if mode == DeliveryMode::Dropship && term == DeliveryTerm::FreightExtra {
        return Err(Error::from("代发价已含运费，代发规则只能选择配送或不配送"));
    }
    Ok(())
}

/// 代发与集采两部分配送规则。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryPolicy {
    /// 代发规则。
    pub dropship: DeliveryRuleSet,
    /// 集采规则。
    pub bulk: DeliveryRuleSet,
}

impl Default for DeliveryPolicy {
    fn default() -> Self {
        Self::nationwide_included()
    }
}

impl DeliveryPolicy {
    /// 校验并规范化配送规则。
    ///
    /// # 参数
    /// * `dropship` - 代发规则
    /// * `bulk` - 集采规则
    ///
    /// # 返回
    /// 返回例外按地区代码排序的政策。
    ///
    /// # 错误
    /// 代发出现运费另计、地区不存在、同一方式内地区重复或例外超过上限时返回错误。
    pub fn new(dropship: DeliveryRuleSet, bulk: DeliveryRuleSet) -> Result<Self> {
        Ok(Self {
            dropship: dropship.normalized(DeliveryMode::Dropship)?,
            bulk: bulk.normalized(DeliveryMode::Bulk)?,
        })
    }

    /// 未设置配送模板时使用的政策：全国配送，代发与集采均包邮。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回无例外的全国包邮政策。
    ///
    /// # 错误
    /// 无。
    pub fn nationwide_included() -> Self {
        Self {
            dropship: DeliveryRuleSet::uniform(DeliveryTerm::FreightIncluded),
            bulk: DeliveryRuleSet::uniform(DeliveryTerm::FreightIncluded),
        }
    }

    /// 判定指定供货方式发往某地区的配送结果。
    ///
    /// # 参数
    /// * `mode` - 代发或集采
    /// * `region` - 收货或仓库所在地区
    ///
    /// # 返回
    /// 返回最具体规则的判定。
    ///
    /// # 错误
    /// 无。
    pub fn judge(&self, mode: DeliveryMode, region: &RegionCode) -> DeliveryTerm {
        match mode {
            DeliveryMode::Dropship => self.dropship.judge(region),
            DeliveryMode::Bulk => self.bulk.judge(region),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DeliveryMode, DeliveryPolicy, DeliveryRegionRule, DeliveryRuleSet, DeliveryTerm};
    use crate::region::RegionCode;

    fn code(value: &str) -> RegionCode {
        RegionCode::parse(value).unwrap()
    }

    fn rule(region: &str, term: DeliveryTerm) -> DeliveryRegionRule {
        DeliveryRegionRule { region_code: code(region), term }
    }

    fn sample_policy() -> DeliveryPolicy {
        DeliveryPolicy::new(
            DeliveryRuleSet {
                default_term: DeliveryTerm::FreightIncluded,
                rules: vec![rule("650000", DeliveryTerm::NotDeliverable)],
            },
            DeliveryRuleSet {
                default_term: DeliveryTerm::FreightExtra,
                rules: vec![
                    rule("440300", DeliveryTerm::NotDeliverable),
                    rule("440000", DeliveryTerm::FreightIncluded),
                    rule("440305", DeliveryTerm::FreightExtra),
                ],
            },
        )
        .unwrap()
    }

    #[test]
    fn most_specific_rule_wins() {
        let policy = sample_policy();
        assert_eq!(policy.judge(DeliveryMode::Bulk, &code("440106")), DeliveryTerm::FreightIncluded);
        assert_eq!(policy.judge(DeliveryMode::Bulk, &code("440303")), DeliveryTerm::NotDeliverable);
        assert_eq!(policy.judge(DeliveryMode::Bulk, &code("440305")), DeliveryTerm::FreightExtra);
        assert_eq!(policy.judge(DeliveryMode::Bulk, &code("310115")), DeliveryTerm::FreightExtra);
        assert_eq!(policy.judge(DeliveryMode::Dropship, &code("650102")), DeliveryTerm::NotDeliverable);
        assert_eq!(policy.judge(DeliveryMode::Dropship, &code("440305")), DeliveryTerm::FreightIncluded);
    }

    #[test]
    fn rules_are_sorted_by_region_code() {
        let codes =
            sample_policy().bulk.rules.iter().map(|rule| rule.region_code.to_string()).collect::<Vec<_>>();
        assert_eq!(codes, ["440000", "440300", "440305"]);
    }

    #[test]
    fn dropship_rejects_freight_extra() {
        let error = DeliveryPolicy::new(
            DeliveryRuleSet::uniform(DeliveryTerm::FreightExtra),
            DeliveryRuleSet::uniform(DeliveryTerm::FreightIncluded),
        )
        .unwrap_err();
        assert!(error.to_string().contains("代发规则只能选择配送或不配送"));
        let in_rule = DeliveryPolicy::new(
            DeliveryRuleSet {
                default_term: DeliveryTerm::FreightIncluded,
                rules: vec![rule("440000", DeliveryTerm::FreightExtra)],
            },
            DeliveryRuleSet::uniform(DeliveryTerm::FreightIncluded),
        );
        assert!(in_rule.is_err());
    }

    #[test]
    fn duplicate_region_in_same_mode_is_rejected() {
        let error = DeliveryPolicy::new(
            DeliveryRuleSet::uniform(DeliveryTerm::FreightIncluded),
            DeliveryRuleSet {
                default_term: DeliveryTerm::FreightExtra,
                rules: vec![
                    rule("440000", DeliveryTerm::FreightIncluded),
                    rule("440000", DeliveryTerm::NotDeliverable),
                ],
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("集采规则中地区 广东省 重复"));
    }

    #[test]
    fn unknown_region_in_rule_is_rejected() {
        let unknown: RegionCode = serde_json::from_str("\"449999\"").unwrap();
        let result = DeliveryPolicy::new(
            DeliveryRuleSet {
                default_term: DeliveryTerm::FreightIncluded,
                rules: vec![DeliveryRegionRule { region_code: unknown, term: DeliveryTerm::NotDeliverable }],
            },
            DeliveryRuleSet::uniform(DeliveryTerm::FreightIncluded),
        );
        assert!(result.is_err());
    }

    #[test]
    fn nationwide_default_includes_freight_everywhere() {
        let policy = DeliveryPolicy::default();
        for mode in [DeliveryMode::Dropship, DeliveryMode::Bulk] {
            assert_eq!(policy.judge(mode, &code("650102")), DeliveryTerm::FreightIncluded);
        }
    }

    #[test]
    fn serde_uses_screaming_snake_case() {
        let json = serde_json::to_value(sample_policy()).unwrap();
        assert_eq!(json["bulk"]["default_term"], "FREIGHT_EXTRA");
        assert_eq!(json["dropship"]["rules"][0]["term"], "NOT_DELIVERABLE");
    }
}
