//! 按地区汇总配送覆盖：公司商品池地区筛选的预算结果与查询条件。
//!
//! 一个 SKU 的覆盖由其全部有效且可供供给的配送政策合并得出，分两档：包邮可送
//! （任一供给代发配送或集采包邮）与可送（另含集采运费另计）。每档压缩为整片可送
//! 的最大地区集合与部分可送的地区集合：整省可送只记省代码，全国可送只记
//! [`NATIONWIDE_REGION_CODE`]，数据库按多键索引直接匹配，不逐个 SKU 现算。

use std::collections::HashMap;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::delivery_policy::{DeliveryMode, DeliveryPolicy, DeliveryRuleSet, DeliveryTerm};
use crate::region::{RegionCode, all_regions};

/// 全国标记：全部地区整片可送时代替各省代码存入整片可送集合。
pub const NATIONWIDE_REGION_CODE: &str = "000000";

/// 一个地区按全部有效供给合并后的可送程度。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryReach {
    /// 至少一条供给代发配送或集采包邮。
    FreightIncluded,
    /// 代发均不配送，只能集采并另付运费。
    FreightExtra,
    /// 代发与集采都不配送。
    NotDeliverable,
}

/// 合并各供给对同一地区的代发、集采判定。
///
/// # 参数
/// * `terms` - 逐供给的（代发判定，集采判定）
///
/// # 返回
/// 任一代发配送或集采包邮为包邮可送；否则任一集采运费另计为需另付运费；其余不配送。
///
/// # 错误
/// 无。
fn merge_reach(terms: impl IntoIterator<Item = (DeliveryTerm, DeliveryTerm)>) -> DeliveryReach {
    let mut freight_extra = false;
    for (dropship, bulk) in terms {
        if dropship.is_deliverable() || bulk == DeliveryTerm::FreightIncluded {
            return DeliveryReach::FreightIncluded;
        }
        freight_extra |= bulk == DeliveryTerm::FreightExtra;
    }
    if freight_extra { DeliveryReach::FreightExtra } else { DeliveryReach::NotDeliverable }
}

/// 按全部有效供给的配送政策判定一个地区的可送程度。
///
/// # 参数
/// * `policies` - 该 SKU 全部有效且可供供给的生效政策
/// * `region` - 收货或筛选地区
///
/// # 返回
/// 返回合并后的可送程度；没有政策时为不配送。
///
/// # 错误
/// 无。
pub fn delivery_reach(policies: &[&DeliveryPolicy], region: &RegionCode) -> DeliveryReach {
    merge_reach(policies.iter().map(|policy| {
        (policy.judge(DeliveryMode::Dropship, region), policy.judge(DeliveryMode::Bulk, region))
    }))
}

/// 一档可送程度在全国区划上的压缩表示。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegionCoverage {
    /// 整片可送的最大地区代码，升序；全国整片可送时只含全国标记。
    pub full: Vec<String>,
    /// 部分可送（有可送也有不可送的下级）的省、市代码，升序。
    pub partial: Vec<String>,
}

impl RegionCoverage {
    /// 判断地区是否整片可送。
    ///
    /// # 参数
    /// * `region` - 任一级地区
    ///
    /// # 返回
    /// 全国标记、地区自身或任一上级在整片集合中时为真。
    ///
    /// # 错误
    /// 无。
    pub fn covers_fully(&self, region: &RegionCode) -> bool {
        full_match_codes(region).iter().any(|code| self.full.contains(code))
    }

    /// 判断地区是否部分可送（有可送也有不可送的下级）。
    ///
    /// # 参数
    /// * `region` - 任一级地区
    ///
    /// # 返回
    /// 地区在部分可送集合中时为真；整片可送或完全不可送时为假。
    ///
    /// # 错误
    /// 无。
    pub fn covers_partly(&self, region: &RegionCode) -> bool {
        self.partial.iter().any(|code| code == region.as_str())
    }

    /// 统计地区内可送的最末级地区数量。
    ///
    /// # 参数
    /// * `region` - 任一级地区
    ///
    /// # 返回
    /// 整片可送时为该地区全部最末级数量；否则累加落在该地区内的整片代码的最末级数量。
    ///
    /// # 错误
    /// 无。
    pub fn leaf_count_within(&self, region: &RegionCode) -> usize {
        if self.covers_fully(region) {
            return TREE.leaf_count(region.as_str());
        }
        self.full
            .iter()
            .filter_map(|code| RegionCode::try_from(code.clone()).ok())
            .filter(|code| code != region && code.self_and_ancestors().contains(region))
            .map(|code| TREE.leaf_count(code.as_str()))
            .sum()
    }
}

/// 一个 SKU 两档可送程度的压缩表示。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeliveryCoverage {
    /// 包邮可送：代发配送或集采包邮。
    pub included: RegionCoverage,
    /// 可送：包邮可送或集采运费另计。
    pub reachable: RegionCoverage,
}

impl DeliveryCoverage {
    /// 没有有效供给时的覆盖：任何地区都不可送。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回两档均为空的覆盖。
    ///
    /// # 错误
    /// 无。
    pub fn none() -> Self {
        Self::default()
    }

    /// 不受配送范围限制的覆盖，用于虚拟商品与线下服务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回两档均为全国整片包邮可送的覆盖。
    ///
    /// # 错误
    /// 无。
    pub fn unrestricted() -> Self {
        let nationwide =
            RegionCoverage { full: vec![NATIONWIDE_REGION_CODE.to_string()], partial: Vec::new() };
        Self { included: nationwide.clone(), reachable: nationwide }
    }

    /// 按全部有效供给的配送政策逐个最末级地区判定并压缩。
    ///
    /// # 参数
    /// * `policies` - 该 SKU 全部有效且可供供给的生效政策
    ///
    /// # 返回
    /// 返回两档覆盖；没有政策时任何地区都不可送。逐地区判定与
    /// [`delivery_reach`] 一致。
    ///
    /// # 错误
    /// 无。
    pub fn from_policies(policies: &[&DeliveryPolicy]) -> Self {
        if policies.is_empty() {
            return Self::none();
        }
        let indexed = policies.iter().map(|policy| IndexedPolicy::new(policy)).collect::<Vec<_>>();
        let (included, reachable): (Vec<_>, Vec<_>) = TREE
            .leaf_chains
            .iter()
            .map(|chain| {
                let reach = merge_reach(indexed.iter().map(|policy| policy.judge(chain)));
                (reach == DeliveryReach::FreightIncluded, reach != DeliveryReach::NotDeliverable)
            })
            .unzip();
        Self { included: TREE.summarize(&included), reachable: TREE.summarize(&reachable) }
    }

    /// 判断覆盖是否满足筛选条件。
    ///
    /// # 参数
    /// * `query` - 地区筛选条件
    ///
    /// # 返回
    /// 整片模式要求所选档位整片可送；部分模式要求所选档位部分可送。
    ///
    /// # 错误
    /// 无。
    pub fn matches(&self, query: &DeliveryCoverageQuery) -> bool {
        let tier = if query.include_freight_extra { &self.reachable } else { &self.included };
        match query.extent {
            DeliveryExtent::Full => tier.covers_fully(&query.region),
            DeliveryExtent::Partial => tier.covers_partly(&query.region),
        }
    }

    /// 判断地区内是否有需另付运费才能送到的地方。
    ///
    /// # 参数
    /// * `region` - 任一级地区
    ///
    /// # 返回
    /// 地区内可送的最末级多于包邮可送的最末级时为真。
    ///
    /// # 错误
    /// 无。
    pub fn freight_extra_within(&self, region: &RegionCode) -> bool {
        self.reachable.leaf_count_within(region) > self.included.leaf_count_within(region)
    }
}

/// 筛选范围：整片可送或部分可送。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum DeliveryExtent {
    /// 所选地区全部可送。
    #[default]
    Full,
    /// 所选地区只有部分可送。
    Partial,
}

/// 公司商品池按地区筛选的条件。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeliveryCoverageQuery {
    /// 所选地区，任一级。
    pub region: RegionCode,
    /// 整片或部分可送。
    pub extent: DeliveryExtent,
    /// 是否把集采运费另计算作可送。
    pub include_freight_extra: bool,
}

impl DeliveryCoverageQuery {
    /// 返回所选档位在覆盖文档中的字段名。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 含运费另计时为 `reachable`，否则为 `included`。
    ///
    /// # 错误
    /// 无。
    pub fn tier_field(&self) -> &'static str {
        if self.include_freight_extra { "reachable" } else { "included" }
    }

    /// 返回整片可送命中所需的任一代码。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回地区自身、各级上级与全国标记。
    ///
    /// # 错误
    /// 无。
    pub fn full_match_codes(&self) -> Vec<String> {
        full_match_codes(&self.region)
    }
}

fn full_match_codes(region: &RegionCode) -> Vec<String> {
    let mut codes = region.self_and_ancestors().into_iter().map(String::from).collect::<Vec<_>>();
    codes.push(NATIONWIDE_REGION_CODE.to_string());
    codes
}

/// 以地区代码索引的规则，供全国逐地区判定时避免线性查找。
struct IndexedRuleSet<'a> {
    default_term: DeliveryTerm,
    rules: HashMap<&'a str, DeliveryTerm>,
}

impl<'a> IndexedRuleSet<'a> {
    fn new(set: &'a DeliveryRuleSet) -> Self {
        Self {
            default_term: set.default_term,
            rules: set.rules.iter().map(|rule| (rule.region_code.as_str(), rule.term)).collect(),
        }
    }

    /// 与 [`DeliveryRuleSet::judge`] 相同：由具体到宽泛取第一条命中的例外。
    fn judge(&self, chain: &[RegionCode]) -> DeliveryTerm {
        chain.iter().find_map(|code| self.rules.get(code.as_str()).copied()).unwrap_or(self.default_term)
    }
}

struct IndexedPolicy<'a> {
    dropship: IndexedRuleSet<'a>,
    bulk: IndexedRuleSet<'a>,
}

impl<'a> IndexedPolicy<'a> {
    fn new(policy: &'a DeliveryPolicy) -> Self {
        Self { dropship: IndexedRuleSet::new(&policy.dropship), bulk: IndexedRuleSet::new(&policy.bulk) }
    }

    fn judge(&self, chain: &[RegionCode]) -> (DeliveryTerm, DeliveryTerm) {
        (self.dropship.judge(chain), self.bulk.judge(chain))
    }
}

/// 一个区划节点；没有下级时为最末级。
struct TreeNode {
    code: &'static str,
    children: Vec<TreeNode>,
    leaf_index: Option<usize>,
}

/// 子树的可送状态。
#[derive(Clone, Copy, PartialEq, Eq)]
enum Extent {
    All,
    Some,
    None,
}

/// 内置区划树、最末级判定链与各节点最末级数量。
struct RegionTree {
    provinces: Vec<TreeNode>,
    leaf_chains: Vec<Vec<RegionCode>>,
    leaf_counts: HashMap<&'static str, usize>,
}

static TREE: LazyLock<RegionTree> = LazyLock::new(RegionTree::load);

impl RegionTree {
    fn load() -> Self {
        let mut children: HashMap<&str, Vec<&'static str>> = HashMap::new();
        let mut provinces = Vec::new();
        for node in all_regions() {
            match node.parent_code {
                Some(parent) => children.entry(parent).or_default().push(node.code),
                None => provinces.push(node.code),
            }
        }
        let mut tree = Self { provinces: Vec::new(), leaf_chains: Vec::new(), leaf_counts: HashMap::new() };
        tree.provinces = provinces.into_iter().map(|code| tree.build(code, &children)).collect();
        tree
    }

    fn build(&mut self, code: &'static str, children: &HashMap<&str, Vec<&'static str>>) -> TreeNode {
        let Some(codes) = children.get(code) else {
            let region = RegionCode::try_from(code.to_string()).expect("内置区划代码必须合法");
            self.leaf_chains.push(region.self_and_ancestors());
            self.leaf_counts.insert(code, 1);
            return TreeNode { code, children: Vec::new(), leaf_index: Some(self.leaf_chains.len() - 1) };
        };
        let nodes = codes.iter().map(|child| self.build(child, children)).collect::<Vec<_>>();
        let count = nodes.iter().map(|node| self.leaf_counts[node.code]).sum();
        self.leaf_counts.insert(code, count);
        TreeNode { code, children: nodes, leaf_index: None }
    }

    fn leaf_count(&self, code: &str) -> usize {
        self.leaf_counts.get(code).copied().unwrap_or_default()
    }

    /// 把逐最末级的可送标记压缩为整片与部分可送集合。
    fn summarize(&self, covered: &[bool]) -> RegionCoverage {
        let mut coverage = RegionCoverage::default();
        let extents =
            self.provinces.iter().map(|node| walk(node, covered, &mut coverage)).collect::<Vec<_>>();
        if extents.iter().all(|extent| *extent == Extent::All) {
            coverage.full = vec![NATIONWIDE_REGION_CODE.to_string()];
        } else {
            push_full_children(&self.provinces, &extents, &mut coverage);
        }
        coverage.full.sort();
        coverage.partial.sort();
        coverage
    }
}

/// 计算子树状态；部分可送的节点记入部分集合，并把整片可送的下级记入整片集合。
fn walk(node: &TreeNode, covered: &[bool], coverage: &mut RegionCoverage) -> Extent {
    if let Some(index) = node.leaf_index {
        return if covered[index] { Extent::All } else { Extent::None };
    }
    let extents = node.children.iter().map(|child| walk(child, covered, coverage)).collect::<Vec<_>>();
    if extents.iter().all(|extent| *extent == Extent::All) {
        return Extent::All;
    }
    if extents.iter().all(|extent| *extent == Extent::None) {
        return Extent::None;
    }
    coverage.partial.push(node.code.to_string());
    push_full_children(&node.children, &extents, coverage);
    Extent::Some
}

fn push_full_children(nodes: &[TreeNode], extents: &[Extent], coverage: &mut RegionCoverage) {
    for (node, extent) in nodes.iter().zip(extents) {
        if *extent == Extent::All {
            coverage.full.push(node.code.to_string());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::delivery_policy::DeliveryRegionRule;

    fn code(value: &str) -> RegionCode {
        RegionCode::parse(value).unwrap()
    }

    fn rules(default_term: DeliveryTerm, exceptions: &[(&str, DeliveryTerm)]) -> DeliveryRuleSet {
        DeliveryRuleSet {
            default_term,
            rules: exceptions
                .iter()
                .map(|(region, term)| DeliveryRegionRule { region_code: code(region), term: *term })
                .collect(),
        }
    }

    fn policy(dropship: DeliveryRuleSet, bulk: DeliveryRuleSet) -> DeliveryPolicy {
        DeliveryPolicy::new(dropship, bulk).unwrap()
    }

    fn query(region: &str, extent: DeliveryExtent, include_freight_extra: bool) -> DeliveryCoverageQuery {
        DeliveryCoverageQuery { region: code(region), extent, include_freight_extra }
    }

    use DeliveryTerm::{FreightExtra, FreightIncluded, NotDeliverable};

    #[test]
    fn nationwide_policy_compresses_to_single_marker() {
        let open = DeliveryPolicy::nationwide_included();
        let coverage = DeliveryCoverage::from_policies(&[&open]);
        assert_eq!(coverage, DeliveryCoverage::unrestricted());
        assert!(coverage.matches(&query("440305", DeliveryExtent::Full, false)));
        assert!(!coverage.matches(&query("440000", DeliveryExtent::Partial, false)));
        assert!(!coverage.freight_extra_within(&code("650000")));
    }

    #[test]
    fn district_exception_splits_province_and_city_into_partial() {
        let shenzhen_nanshan_closed =
            policy(rules(FreightIncluded, &[("440305", NotDeliverable)]), rules(NotDeliverable, &[]));
        let coverage = DeliveryCoverage::from_policies(&[&shenzhen_nanshan_closed]);
        assert_eq!(coverage.included.partial, vec!["440000".to_string(), "440300".to_string()]);
        assert!(!coverage.included.full.contains(&"440305".to_string()));
        assert!(coverage.included.full.contains(&"440304".to_string()));
        assert!(coverage.included.full.contains(&"110000".to_string()));
        assert!(coverage.matches(&query("440100", DeliveryExtent::Full, false)));
        assert!(!coverage.matches(&query("440000", DeliveryExtent::Full, false)));
        assert!(coverage.matches(&query("440000", DeliveryExtent::Partial, false)));
        assert!(coverage.matches(&query("440300", DeliveryExtent::Partial, false)));
        assert!(!coverage.matches(&query("440305", DeliveryExtent::Partial, false)));
        assert!(!coverage.matches(&query("440305", DeliveryExtent::Full, false)));
    }

    #[test]
    fn freight_extra_tier_reaches_regions_dropship_refuses() {
        let xinjiang_bulk_only = policy(
            rules(FreightIncluded, &[("650000", NotDeliverable)]),
            rules(NotDeliverable, &[("650000", FreightExtra)]),
        );
        let coverage = DeliveryCoverage::from_policies(&[&xinjiang_bulk_only]);
        assert!(!coverage.matches(&query("650000", DeliveryExtent::Full, false)));
        assert!(!coverage.matches(&query("650000", DeliveryExtent::Partial, false)));
        assert!(coverage.matches(&query("650102", DeliveryExtent::Full, true)));
        assert_eq!(coverage.reachable.full, vec![NATIONWIDE_REGION_CODE.to_string()]);
        assert!(coverage.freight_extra_within(&code("650000")));
        assert!(coverage.freight_extra_within(&code("650102")));
        assert!(!coverage.freight_extra_within(&code("440000")));
    }

    #[test]
    fn offerings_merge_so_any_open_supply_covers_region() {
        let south_only =
            policy(rules(NotDeliverable, &[("440000", FreightIncluded)]), rules(NotDeliverable, &[]));
        let north_only =
            policy(rules(NotDeliverable, &[("110000", FreightIncluded)]), rules(NotDeliverable, &[]));
        let coverage = DeliveryCoverage::from_policies(&[&south_only, &north_only]);
        assert_eq!(coverage.included.full, vec!["110000".to_string(), "440000".to_string()]);
        assert!(coverage.included.partial.is_empty());
        assert!(!coverage.matches(&query("310000", DeliveryExtent::Full, true)));
        assert!(!coverage.matches(&query("310000", DeliveryExtent::Partial, true)));
    }

    #[test]
    fn no_policies_reach_nothing() {
        let coverage = DeliveryCoverage::from_policies(&[]);
        assert_eq!(coverage, DeliveryCoverage::none());
        assert!(!coverage.matches(&query("440000", DeliveryExtent::Full, true)));
        assert_eq!(coverage.included.leaf_count_within(&code("440000")), 0);
    }

    #[test]
    fn leaf_counts_follow_partial_coverage() {
        let closed_city =
            policy(rules(FreightIncluded, &[("440300", NotDeliverable)]), rules(NotDeliverable, &[]));
        let coverage = DeliveryCoverage::from_policies(&[&closed_city]);
        let province = TREE.leaf_count("440000");
        let city = TREE.leaf_count("440300");
        assert!(city > 1 && province > city);
        assert_eq!(coverage.included.leaf_count_within(&code("440000")), province - city);
        assert_eq!(coverage.included.leaf_count_within(&code("440300")), 0);
        assert_eq!(coverage.included.leaf_count_within(&code("110000")), TREE.leaf_count("110000"));
    }

    /// 全国逐最末级的索引判定必须与单地区判定完全一致，保证预算与实时提示同一口径。
    #[test]
    fn precomputed_coverage_agrees_with_single_region_reach() {
        let mixed = policy(
            rules(
                FreightIncluded,
                &[("650000", NotDeliverable), ("540000", NotDeliverable), ("650102", FreightIncluded)],
            ),
            rules(FreightExtra, &[("540000", NotDeliverable), ("441900", FreightIncluded)]),
        );
        let refuse =
            policy(rules(NotDeliverable, &[("710000", FreightIncluded)]), rules(NotDeliverable, &[]));
        let policies = [&mixed, &refuse];
        let coverage = DeliveryCoverage::from_policies(&policies);
        for chain in &TREE.leaf_chains {
            let leaf = &chain[0];
            let reach = delivery_reach(&policies, leaf);
            assert_eq!(
                coverage.included.covers_fully(leaf),
                reach == DeliveryReach::FreightIncluded,
                "{leaf}"
            );
            assert_eq!(
                coverage.reachable.covers_fully(leaf),
                reach != DeliveryReach::NotDeliverable,
                "{leaf}"
            );
        }
    }

    #[test]
    fn leaf_regions_cover_the_whole_dataset() {
        let total: usize = TREE.provinces.iter().map(|node| TREE.leaf_count(node.code)).sum();
        assert_eq!(total, TREE.leaf_chains.len());
        assert!(total > 2800);
    }
}
