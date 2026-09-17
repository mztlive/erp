//! 库存调整列表的申请人与当前审批人事实验口；不授予动作权。

use std::collections::{BTreeSet, HashMap};

use async_trait::async_trait;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 调整单列表投影用的审批快照申请人与当前开放审批人。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AdjustmentPeopleFact {
    /// 审批快照 `submitted_by`；草稿未提交时为空，不得回退 `created_by`。
    pub submitted_by: Option<String>,
    /// 当前开放审批任务的 `current_assignee`。
    pub current_assignee: Option<String>,
}

/// 库存调整人员事实端口；筛选只收窄仓库授权结果。
#[async_trait]
pub trait AdjustmentPeopleFactsPort: Send + Sync {
    /// 按调整单主键批量读取最新快照申请人与当前开放审批人。
    ///
    /// # 参数
    /// * `ids` - 调整单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回已存在人员事实的映射；缺失键表示该单尚无申请人或开放审批人。
    ///
    /// # 错误
    /// 端口未接线或读取失败。
    async fn people_by_adjustment_ids(
        &self,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, AdjustmentPeopleFact>>;

    /// 最新审批快照申请人落在 `applicant_ids` 的调整单主键。
    ///
    /// # 参数
    /// * `applicant_ids` - 已规范化申请人 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回匹配主键；无命中返回空集合。
    ///
    /// # 错误
    /// 端口未接线、超限或读取失败。
    async fn adjustment_ids_submitted_by(
        &self,
        applicant_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;

    /// 当前开放审批人落在 `handler_ids` 的调整单主键。
    ///
    /// # 参数
    /// * `handler_ids` - 已规范化当前审批人 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 返回匹配主键；无命中返回空集合。
    ///
    /// # 错误
    /// 端口未接线、超限或读取失败。
    async fn adjustment_ids_assigned_to(
        &self,
        handler_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 未接线时失败关闭，禁止把人员筛选假装成无条件。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAdjustmentPeopleFactsPort;

#[async_trait]
impl AdjustmentPeopleFactsPort for FailClosedAdjustmentPeopleFactsPort {
    async fn people_by_adjustment_ids(
        &self,
        _ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<HashMap<String, AdjustmentPeopleFact>> {
        Err(Error::Internal("库存调整人员事实端口未接线".to_string()))
    }

    async fn adjustment_ids_submitted_by(
        &self,
        _applicant_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(Error::Internal("库存调整人员事实端口未接线".to_string()))
    }

    async fn adjustment_ids_assigned_to(
        &self,
        _handler_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        Err(Error::Internal("库存调整人员事实端口未接线".to_string()))
    }
}

/// 同一对象只保留最高快照版本的申请人，禁止用创建人顶替。
///
/// # 参数
/// * `rows` - `(调整单 ID, 快照版本, submitted_by)`
///
/// # 返回
/// 返回每个调整单的最新 `submitted_by`。
pub fn latest_snapshot_submitters(
    rows: impl IntoIterator<Item = (String, u32, String)>,
) -> HashMap<String, String> {
    let mut latest = HashMap::<String, (u32, String)>::new();
    for (object_id, version, submitted_by) in rows {
        if submitted_by.trim().is_empty() {
            continue;
        }
        match latest.get(&object_id) {
            Some((current, _)) if *current > version => {},
            _ => {
                latest.insert(object_id, (version, submitted_by));
            },
        }
    }
    latest.into_iter().map(|(id, (_, submitted_by))| (id, submitted_by)).collect()
}

/// 仅当最新快照申请人命中筛选时保留对象；历史快照与 `created_by` 不贡献命中。
///
/// # 参数
/// * `latest` - 最新快照申请人
/// * `wanted` - 申请人筛选
///
/// # 返回
/// 返回命中的调整单 ID。
pub fn applicant_object_ids(latest: &HashMap<String, String>, wanted: &[String]) -> Vec<String> {
    let wanted = wanted.iter().cloned().collect::<BTreeSet<_>>();
    let mut ids = latest
        .iter()
        .filter(|(_, submitted_by)| wanted.contains(*submitted_by))
        .map(|(id, _)| id.clone())
        .collect::<Vec<_>>();
    ids.sort();
    ids
}

/// 合并申请人与当前审批人；缺快照时申请人保持空，不得填入创建人。
///
/// # 参数
/// * `submitted` - 最新快照申请人
/// * `assignees` - 开放审批当前处理人
///
/// # 返回
/// 返回按调整单合并后的人员事实。
pub fn merge_adjustment_people(
    submitted: impl IntoIterator<Item = (String, String)>,
    assignees: impl IntoIterator<Item = (String, String)>,
) -> HashMap<String, AdjustmentPeopleFact> {
    let mut facts = HashMap::<String, AdjustmentPeopleFact>::new();
    for (id, submitted_by) in submitted {
        facts.entry(id).or_default().submitted_by = Some(submitted_by);
    }
    for (id, current_assignee) in assignees {
        facts.entry(id).or_default().current_assignee = Some(current_assignee);
    }
    facts
}

/// 不同人员字段按 AND 求交；`None` 表示该字段未筛选。
///
/// # 参数
/// * `left` - 申请人命中集合
/// * `right` - 当前审批人命中集合
///
/// # 返回
/// 两边都未筛选时返回 `None`；否则返回交集，空集合表示无命中。
pub fn intersect_object_ids(left: Option<Vec<String>>, right: Option<Vec<String>>) -> Option<Vec<String>> {
    match (left, right) {
        (None, None) => None,
        (Some(ids), None) | (None, Some(ids)) => Some(ids),
        (Some(left), Some(right)) => {
            let wanted = left.into_iter().collect::<BTreeSet<_>>();
            Some(right.into_iter().filter(|id| wanted.contains(id)).collect())
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{
        applicant_object_ids, intersect_object_ids, latest_snapshot_submitters, merge_adjustment_people,
    };

    #[test]
    fn latest_snapshot_submitter_is_not_created_by() {
        let latest = latest_snapshot_submitters([
            ("adj-1".into(), 1, "creator-1".into()),
            ("adj-1".into(), 2, "applicant-1".into()),
            ("adj-2".into(), 1, "applicant-2".into()),
        ]);
        assert_eq!(latest.get("adj-1").map(String::as_str), Some("applicant-1"));
        assert_ne!(latest.get("adj-1").map(String::as_str), Some("creator-1"));
        assert_eq!(applicant_object_ids(&latest, &["applicant-1".into()]), vec!["adj-1".to_string()]);
        assert!(applicant_object_ids(&latest, &["creator-1".into()]).is_empty());
    }

    #[test]
    fn people_facts_keep_applicant_and_handler_independent() {
        let facts = merge_adjustment_people(
            [("adj-1".into(), "applicant-1".into())],
            [("adj-1".into(), "handler-1".into()), ("adj-2".into(), "handler-2".into())],
        );
        assert_eq!(facts["adj-1"].submitted_by.as_deref(), Some("applicant-1"));
        assert_eq!(facts["adj-1"].current_assignee.as_deref(), Some("handler-1"));
        assert!(facts["adj-2"].submitted_by.is_none());
        assert_eq!(facts["adj-2"].current_assignee.as_deref(), Some("handler-2"));
    }

    #[test]
    fn applicant_and_handler_filters_and_together() {
        assert_eq!(
            intersect_object_ids(Some(vec!["a".into(), "b".into()]), Some(vec!["b".into(), "c".into()])),
            Some(vec!["b".to_string()])
        );
        assert_eq!(intersect_object_ids(None, Some(vec!["a".into()])), Some(vec!["a".to_string()]));
        assert_eq!(intersect_object_ids(Some(Vec::new()), Some(vec!["a".into()])), Some(Vec::new()));
    }
}
