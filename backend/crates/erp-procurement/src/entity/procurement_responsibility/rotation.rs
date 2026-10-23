//! 负责人池的成员规范化及可扩展分配策略；当前实现整单轮询。

use std::collections::HashSet;

use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 责任解析业务场景；询价场景才使用整单询价规则。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolutionPurpose {
    /// 普通采购任务。
    #[default]
    Procurement,
    /// 询价整单任务。
    Inquiry,
}

/// 规则的分配策略；未知策略由反序列化拒绝，禁止静默降级。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AssignmentStrategy {
    /// 每次成功任务批次从池中选择一人。
    #[default]
    RoundRobin,
}

impl AssignmentStrategy {
    /// 从已规范化的池中选取本批次负责人，不修改轮次。
    /// # 参数
    /// `members` 为配置顺序；`last_owner` 为该规则最近成功分派的人员。
    /// # 返回
    /// 返回下一位成员；首次使用或上次成员移除时从首位开始。
    /// # 错误
    /// 空池拒绝解析。
    pub fn select<'a>(self, members: &'a [String], last_owner: Option<&str>) -> Result<&'a str> {
        if members.is_empty() {
            return Err(Error::from("负责人池至少需要一名采购"));
        }
        match self {
            Self::RoundRobin => {
                let next = last_owner
                    .and_then(|last| members.iter().position(|owner| owner == last))
                    .map_or(0, |position| (position + 1) % members.len());
                Ok(&members[next])
            },
        }
    }
}

/// 校验负责人池，保持配置顺序，拒绝重复成员。
/// # 参数
/// `members` 为输入账号，最多 100 人。
/// # 返回
/// 返回去除首尾空白的非空成员列表。
/// # 错误
/// 空池、超长、空账号或重复人员时返回配置错误。
pub(super) fn normalize_pool(members: Vec<String>) -> Result<Vec<String>> {
    if members.is_empty() || members.len() > 100 {
        return Err(Error::from("负责人池人数必须在 1 至 100 之间"));
    }
    let mut seen = HashSet::new();
    let mut result = Vec::with_capacity(members.len());
    for member in members {
        let owner = normalize_required_text(member, "采购负责人不能为空", 128, "采购负责人过长")?;
        if !seen.insert(owner.clone()) {
            return Err(Error::from("负责人池不能重复选择同一采购"));
        }
        result.push(owner);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_robin_preserves_configured_order_wraps_and_handles_removed_members() {
        let pool = vec!["b".into(), "a".into(), "c".into()];
        for (last, expected) in
            [(None, "b"), (Some("b"), "a"), (Some("a"), "c"), (Some("c"), "b"), (Some("removed"), "b")]
        {
            assert_eq!(AssignmentStrategy::RoundRobin.select(&pool, last).unwrap(), expected);
        }
        assert_eq!(AssignmentStrategy::RoundRobin.select(&["a".into()], Some("a")).unwrap(), "a");
        assert!(AssignmentStrategy::RoundRobin.select(&[], None).is_err());
    }
    #[test]
    fn pool_normalization_rejects_empty_duplicates_and_excess_members() {
        assert_eq!(normalize_pool(vec![" b ".into(), "a".into()]).unwrap(), ["b", "a"]);
        for members in [
            vec![],
            vec![" ".into()],
            vec!["a".into(), " a ".into()],
            vec!["a".repeat(129)],
            vec!["a".into(); 101],
        ] {
            assert!(normalize_pool(members).is_err());
        }
        assert!(serde_json::from_str::<AssignmentStrategy>("\"LOAD_BALANCE\"").is_err());
    }
}
