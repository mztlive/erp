//! 有界、去重的 HTTP 身份筛选；不表达或授予数据权限。

use std::collections::BTreeSet;

use serde::{Deserialize, Deserializer, Serialize};

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
        let parts = raw.split(',').collect::<Vec<_>>();
        if parts.len() > 100 {
            return Err(serde::de::Error::custom("单次人员条件最多 100 个 ID"));
        }
        let mut ids = BTreeSet::new();
        for part in parts {
            let id = part.trim();
            if id.is_empty()
                || id.len() > 128
                || !id.bytes().all(|c| c.is_ascii_alphanumeric() || b"_-".contains(&c))
            {
                return Err(serde::de::Error::custom("人员筛选必须为非空的稳定 ID"));
            }
            ids.insert(id.to_owned());
        }
        Ok(Self(ids.into_iter().collect()))
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
}

/// 由资源查询生成的候选项；值是稳定身份，标签用于区分同名候选。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FilterOption {
    pub value: String,
    pub label: String,
}
/// 分页查询附带完整可见范围内的负责人候选；候选不授予命令资格。
#[derive(Debug, Clone, Serialize)]
pub struct FilteredPage<T> {
    #[serde(flatten)]
    pub page: crate::PageView<T>,
    pub owner_options: Vec<FilterOption>,
    /// 当前负责人权威来源，不代表组织隔离已生效。
    pub ownership_basis: &'static str,
}
