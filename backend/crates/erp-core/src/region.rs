//! 国家统计用行政区划（省、市、区县三级 6 位代码）。
//!
//! 数据来自 `data/regions.tsv`（modood/Administrative-divisions-of-China 整理版），
//! 每行 `代码\t名称\t上级代码`。不设区县的地级市以市为最末级，台湾、香港、
//! 澳门以省级为最末级。上下级按代码数位推导：`ab0000` 为省，`abcd00` 为市。

use std::collections::HashMap;
use std::fmt;
use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

const REGION_DATA: &str = include_str!("../data/regions.tsv");

/// 名称中不表达地理层级的市级占位，拼接完整名称时跳过。
const PLACEHOLDER_CITY_NAMES: [&str; 4] = ["市辖区", "县", "省直辖县级行政区划", "自治区直辖县级行政区划"];

/// 一条行政区划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegionNode {
    /// 6 位代码。
    pub code: &'static str,
    /// 名称。
    pub name: &'static str,
    /// 上级代码；省级为空。
    pub parent_code: Option<&'static str>,
}

struct RegionIndex {
    nodes: Vec<RegionNode>,
    by_code: HashMap<&'static str, usize>,
    parents: HashMap<&'static str, usize>,
}

static REGIONS: LazyLock<RegionIndex> = LazyLock::new(load_regions);

fn load_regions() -> RegionIndex {
    let nodes = REGION_DATA
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| {
            let mut parts = line.split('\t');
            let code = parts.next().unwrap_or_default();
            let name = parts.next().unwrap_or_default();
            let parent_code = parts.next().filter(|value| !value.is_empty());
            RegionNode { code, name, parent_code }
        })
        .collect::<Vec<_>>();
    let by_code = nodes.iter().enumerate().map(|(index, node)| (node.code, index)).collect();
    let mut parents = HashMap::new();
    for node in &nodes {
        if let Some(parent_code) = node.parent_code {
            *parents.entry(parent_code).or_insert(0) += 1;
        }
    }
    RegionIndex { nodes, by_code, parents }
}

/// 返回全部行政区划，按省、市、区县的数据顺序排列。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回内置区划列表。
///
/// # 错误
/// 无。
pub fn all_regions() -> &'static [RegionNode] {
    &REGIONS.nodes
}

/// 区划层级。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RegionLevel {
    /// 省级。
    Province,
    /// 地级。
    City,
    /// 县级。
    District,
}

/// 已校验格式的 6 位行政区划代码。
///
/// 反序列化只校验格式，存在性与最末级由 [`RegionCode::parse`]、
/// [`RegionCode::parse_leaf`] 在输入边界校验，避免区划调整后历史文档无法读取。
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct RegionCode(String);

impl RegionCode {
    /// 解析内置区划中存在的任一级代码。
    ///
    /// # 参数
    /// * `value` - 原始代码，允许首尾空白
    ///
    /// # 返回
    /// 返回区划代码。
    ///
    /// # 错误
    /// 格式非法或内置区划中不存在时返回错误。
    pub fn parse(value: &str) -> Result<Self> {
        let code = Self::try_from(value.trim().to_string())?;
        if !REGIONS.by_code.contains_key(code.as_str()) {
            return Err(Error::from(format!("地区代码 {code} 不存在")));
        }
        Ok(code)
    }

    /// 解析收货地址使用的最末级代码。
    ///
    /// # 参数
    /// * `value` - 原始代码，允许首尾空白
    ///
    /// # 返回
    /// 返回没有下级的区划代码。
    ///
    /// # 错误
    /// 格式非法、不存在或仍有下级区划时返回错误。
    pub fn parse_leaf(value: &str) -> Result<Self> {
        let code = Self::parse(value)?;
        if REGIONS.parents.contains_key(code.as_str()) {
            return Err(Error::from("收货地区须选择到区县"));
        }
        Ok(code)
    }

    /// 返回代码文本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 6 位代码。
    ///
    /// # 错误
    /// 无。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 返回按代码数位推导的层级。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `ab0000` 为省级，`abcd00` 为地级，其余为县级。
    ///
    /// # 错误
    /// 无。
    pub fn level(&self) -> RegionLevel {
        if self.0.ends_with("0000") {
            RegionLevel::Province
        } else if self.0.ends_with("00") {
            RegionLevel::City
        } else {
            RegionLevel::District
        }
    }

    /// 返回自身及各级上级代码，由具体到宽泛排列。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 县级返回县、市、省；地级返回市、省；省级只返回自身。
    ///
    /// # 错误
    /// 无。
    pub fn self_and_ancestors(&self) -> Vec<RegionCode> {
        let mut codes = vec![self.clone()];
        if self.level() == RegionLevel::District {
            codes.push(Self(format!("{}00", &self.0[..4])));
        }
        if self.level() != RegionLevel::Province {
            codes.push(Self(format!("{}0000", &self.0[..2])));
        }
        codes
    }

    /// 返回本级名称；内置区划中不存在时返回代码本身。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回区划名称。
    ///
    /// # 错误
    /// 无。
    pub fn name(&self) -> String {
        node_of(&self.0).map_or_else(|| self.0.clone(), |node| node.name.to_string())
    }

    /// 返回由省到本级拼接的完整名称，跳过"市辖区"等占位层级。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 例如"广东省深圳市南山区"；内置区划中不存在时返回代码本身。
    ///
    /// # 错误
    /// 无。
    pub fn full_name(&self) -> String {
        let mut names = Vec::new();
        let mut current = node_of(&self.0);
        while let Some(node) = current {
            if !PLACEHOLDER_CITY_NAMES.contains(&node.name) {
                names.push(node.name);
            }
            current = node.parent_code.and_then(node_of);
        }
        if names.is_empty() {
            return self.0.clone();
        }
        names.reverse();
        names.concat()
    }
}

fn node_of(code: &str) -> Option<&'static RegionNode> {
    REGIONS.by_code.get(code).map(|&index| &REGIONS.nodes[index])
}

impl TryFrom<String> for RegionCode {
    type Error = Error;

    fn try_from(value: String) -> Result<Self> {
        if value.len() != 6 || !value.bytes().all(|byte| byte.is_ascii_digit()) || value.starts_with("00") {
            return Err(Error::from(format!("地区代码 {value} 格式非法")));
        }
        Ok(Self(value))
    }
}

impl From<RegionCode> for String {
    fn from(value: RegionCode) -> Self {
        value.0
    }
}

impl fmt::Display for RegionCode {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::{RegionCode, RegionLevel, all_regions};

    #[test]
    fn dataset_contains_three_levels_and_special_regions() {
        let regions = all_regions();
        assert!(regions.len() > 3000);
        assert!(regions.iter().any(|node| node.code == "810000" && node.parent_code.is_none()));
        assert!(regions.iter().all(|node| {
            node.parent_code.is_none_or(|parent| regions.iter().any(|candidate| candidate.code == parent))
        }));
    }

    #[test]
    fn parse_accepts_any_existing_level_and_rejects_unknown() {
        assert_eq!(RegionCode::parse(" 440000 ").unwrap().level(), RegionLevel::Province);
        assert_eq!(RegionCode::parse("440300").unwrap().level(), RegionLevel::City);
        assert_eq!(RegionCode::parse("440305").unwrap().level(), RegionLevel::District);
        assert!(RegionCode::parse("449999").is_err());
        assert!(RegionCode::parse("44030").is_err());
        assert!(RegionCode::parse("44030a").is_err());
    }

    #[test]
    fn parse_leaf_requires_lowest_level() {
        assert!(RegionCode::parse_leaf("440305").is_ok());
        assert!(RegionCode::parse_leaf("441900").is_ok(), "东莞不设区县");
        assert!(RegionCode::parse_leaf("810000").is_ok(), "香港以省级为最末级");
        assert!(RegionCode::parse_leaf("440300").is_err());
        assert!(RegionCode::parse_leaf("440000").is_err());
    }

    #[test]
    fn ancestors_run_from_specific_to_broad() {
        let codes = RegionCode::parse("440305").unwrap().self_and_ancestors();
        let codes = codes.iter().map(RegionCode::as_str).collect::<Vec<_>>();
        assert_eq!(codes, ["440305", "440300", "440000"]);
        let city = RegionCode::parse("441900").unwrap().self_and_ancestors();
        assert_eq!(city.iter().map(RegionCode::as_str).collect::<Vec<_>>(), ["441900", "440000"]);
        assert_eq!(RegionCode::parse("650000").unwrap().self_and_ancestors().len(), 1);
    }

    #[test]
    fn full_name_skips_placeholder_cities() {
        assert_eq!(RegionCode::parse("440305").unwrap().full_name(), "广东省深圳市南山区");
        assert_eq!(RegionCode::parse("110105").unwrap().full_name(), "北京市朝阳区");
        assert_eq!(RegionCode::parse("810000").unwrap().full_name(), "香港特别行政区");
    }

    #[test]
    fn serde_round_trips_as_plain_string() {
        let code = RegionCode::parse("440305").unwrap();
        let json = serde_json::to_string(&code).unwrap();
        assert_eq!(json, "\"440305\"");
        assert_eq!(serde_json::from_str::<RegionCode>(&json).unwrap(), code);
        assert!(serde_json::from_str::<RegionCode>("\"abc\"").is_err());
    }
}
