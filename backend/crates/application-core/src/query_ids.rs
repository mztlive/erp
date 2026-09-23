//! 有界、去重的 HTTP 身份筛选；不表达或授予数据权限。

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

/// 单次人员条件 ID 上限。
const MAX_QUERY_IDS: usize = 100;
/// 单个稳定 ID 长度上限（字节）。
const MAX_QUERY_ID_LEN: usize = 128;

/// 校验单个稳定 ID：非空、限长、仅 ASCII 字母数字及 `-_`。
fn is_valid_query_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= MAX_QUERY_ID_LEN
        && id.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
}

/// 解析逗号分隔的 ID 集合：限项、去首尾空白、去重排序。
fn parse_query_ids(raw: &str) -> Result<BTreeSet<String>, &'static str> {
    let parts = raw.split(',').collect::<Vec<_>>();
    if parts.len() > MAX_QUERY_IDS {
        return Err("单次人员条件最多 100 个 ID");
    }
    let mut ids = BTreeSet::new();
    for part in parts {
        let id = part.trim();
        if !is_valid_query_id(id) {
            return Err("人员筛选必须为非空的稳定 ID");
        }
        ids.insert(id.to_owned());
    }
    Ok(ids)
}

/// 逗号分隔的稳定 ID 集合。显式空值、空片段及超过 100 项的请求拒绝。
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct QueryIds(Vec<String>);

impl QueryIds {
    /// 返回规范化身份集合；调用方必须与服务端授权条件求交。
    pub fn as_slice(&self) -> &[String] {
        &self.0
    }
}

impl<'de> Deserialize<'de> for QueryIds {
    /// 从单个查询参数解析 ID；姓名只允许用于独立关键词搜索。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = String::deserialize(deserializer)?;
        parse_query_ids(&raw).map(|ids| Self(ids.into_iter().collect())).map_err(serde::de::Error::custom)
    }
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
        assert_eq!(parse(" user-2,user-1,user-2 ").unwrap().as_slice(), &["user-1", "user-2"]);
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
