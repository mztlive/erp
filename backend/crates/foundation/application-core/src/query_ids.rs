//! 有界、去重的 HTTP 身份筛选；不表达或授予数据权限。

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

/// 单次人员条件 ID 上限。
const MAX_QUERY_IDS: usize = 100;
/// 单个稳定 ID 长度上限（字节）。
const MAX_QUERY_ID_LEN: usize = 128;

// 使用人员 ID 筛选前，检查非空、至多 128 字节且只含 ASCII 字母数字、`-` 或 `_`，不接受姓名。
fn is_valid_query_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_QUERY_ID_LEN
        && id.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

// 把逗号参数转换为去重且按字典序排列的 ID；先检查原始段数不超过 100，再裁剪并校验每段，重复值不能绕过段数上限。
fn parse_query_ids(raw: &str) -> Result<Vec<String>, &'static str> {
    let parts = raw.split(',');
    if parts.clone().nth(MAX_QUERY_IDS).is_some() {
        return Err("一次最多选择 100 人，请减少选择人数");
    }

    let mut ids = BTreeSet::new();
    for part in parts {
        let id = part.trim();
        if !is_valid_query_id(id) {
            return Err("人员筛选中有无效选项，请重新选择人员");
        }
        ids.insert(id);
    }
    Ok(ids.into_iter().map(str::to_owned).collect())
}

/// 逗号分隔的稳定 ID 集合。显式空值、空片段及超过 100 项的请求拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct QueryIds(Vec<String>);

impl QueryIds {
    /// 借用规范化的人员 ID 来构造筛选条件；调用方必须与服务端授权范围求交。
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }

    /// 需要自有 ID 集合作为查询条件时，直接转移已校验、去重并排序的向量，不复制。
    pub fn into_vec(self) -> Vec<String> {
        self.0
    }
}

impl<'de> Deserialize<'de> for QueryIds {
    // 将单个字符串查询参数交给人员 ID 解析器；超过 100 段，或任一段为空、超 128 字节、含非法字符都会拒绝。
    // 姓名应走独立关键词搜索。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        parse_query_ids(&raw).map(Self).map_err(serde::de::Error::custom)
    }
}

/// 由资源查询生成的候选项；值是稳定身份，标签用于区分同名候选。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterOption {
    pub value: String,
    pub label: String,
}
/// 分页结果的归属口径；候选由独立目录查询。
#[derive(Debug, Clone, Serialize)]
pub struct OwnershipPage<T> {
    #[serde(flatten)]
    pub page: crate::PageView<T>,
    /// 当前负责人权威来源，不代表组织隔离已生效。
    pub ownership_basis: &'static str,
}

#[cfg(test)]
mod tests {
    use serde::de::value::{Error, StrDeserializer};

    use super::*;
    fn parse(raw: &str) -> Result<QueryIds, Error> {
        QueryIds::deserialize(StrDeserializer::new(raw))
    }
    #[test]
    fn identities_are_deduplicated_and_names_are_rejected() {
        assert_eq!(parse(" user-2,user-1,user-2 ").unwrap().into_vec(), ["user-1", "user-2"]);

        for raw in ["", " ", "张三", "user-1,", "user-1,,user-2"] {
            assert!(parse(raw).is_err());
        }

        assert!(parse(&vec!["u"; 101].join(",")).is_err());
        assert!(parse(&vec!["u"; 100].join(",")).is_ok());
    }

    #[test]
    fn overlong_id_is_rejected_at_boundary() {
        assert!(parse_query_ids(&"a".repeat(128)).is_ok());
        assert!(parse_query_ids(&"a".repeat(129)).is_err());
        assert!(!is_valid_query_id("user@1"));
        assert!(is_valid_query_id("user-1_x"));
    }

    #[test]
    fn segment_limit_is_checked_before_individual_ids() {
        let mut parts = vec!["u"; 100];
        parts[0] = "";
        assert_eq!(parse_query_ids(&parts.join(",")), Err("人员筛选中有无效选项，请重新选择人员"));
        parts.push("u");
        assert_eq!(parse_query_ids(&parts.join(",")), Err("一次最多选择 100 人，请减少选择人数"));
    }
}
