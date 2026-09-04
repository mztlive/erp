//! 对账差异最新决定批量读取（INT-R26）。
//!
//! 差异列表页此前对当前页每行各执行一次最新决定查询（N+1）；本模块提供按
//! 当前页差异 ID 集合一次装载全部决定行、再按 `(差异 ID, 决定序号)` 在内存中
//! 取每差异最新一条的批量接口。最新语义与单差异 [`Repository::find_latest_by_difference`]
//! 一致（决定序号最大者胜出），无决定行的差异不在结果中，由 Service 解释为
//! 无状态、版本零。

use std::collections::{HashMap, HashSet};

use entities::integration_ops::ReconciliationDifferenceResolution;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

use crate::executor::Executor;
use crate::repository::Repository;
use crate::Result;

/// 对差异 ID 集合去重并保持首次出现顺序。
///
/// # 参数
/// * `ids` - 原始差异 ID 集合（可含重复）
///
/// # 返回
/// 返回去重后的差异 ID；空输入返回空集合。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯内存函数，不访问数据库。
fn dedupe_difference_ids(ids: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id.clone());
        }
    }
    unique
}

/// 构造按差异 ID 集合批量查询决定行的过滤文档。
///
/// # 参数
/// * `ids` - 去重后的差异 ID 集合（非空）
///
/// # 返回
/// 返回含 `$in` 分支与软删除排除的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯过滤构造，不访问数据库；只限定所属差异与未删除标记，不截断行数，
/// 最新选择由内存按决定序号完成，与单差异查询的序号语义同义。
fn latest_batch_filter(ids: &[String]) -> Document {
    doc! {
        "reconciliation_difference_id": { "$in": ids },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 在内存中按差异取决定序号最大的一条（与单差异查询同序）。
///
/// 最新定义与 [`Repository::<ReconciliationDifferenceResolution>::find_latest_by_difference`]
/// 一致：`resolution_no` 最大者胜出；决定序号在同一差异内唯一，无并列。
/// 无决定行的差异不出现，由 Service 解释为无状态。
///
/// # 参数
/// * `records` - 批量取回的决定记录（可含多差异、多条）
///
/// # 返回
/// 返回按差异 ID 索引的最新决定记录。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 纯内存选择，不访问数据库；序号比较只用 `resolution_no`，与数据库
/// `resolution_no` 降序取首条同义。
fn latest_per_difference(
    records: Vec<ReconciliationDifferenceResolution>,
) -> HashMap<String, ReconciliationDifferenceResolution> {
    let mut latest: HashMap<String, ReconciliationDifferenceResolution> = HashMap::new();
    for record in records {
        let key = record.reconciliation_difference_id.to_string();
        let replace = match latest.get(&key) {
            None => true,
            Some(current) => record.resolution_no > current.resolution_no,
        };
        if replace {
            latest.insert(key, record);
        }
    }
    latest
}

impl<'a> Repository<'a, ReconciliationDifferenceResolution> {
    /// 按当前页差异 ID 集合批量读取各差异最新决定（INT-R26）。
    ///
    /// 一次 `$in` 查询装载本页全部决议行，再按单差异查询同序在内存中取每
    /// 差异序号最大的一条；空输入不访问数据库。无决定行的差异不在结果中，
    /// 由 Service 解释为无状态、版本零。页面过滤、稳定排序与总数仍由
    /// `search_differences` 负责，本方法不改变分页语义。
    ///
    /// # 参数
    /// * `difference_ids` - 本页差异 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按差异 ID 索引的最新决定记录；缺项表示该差异尚无决定。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回实体，不返回 services DTO、HTTP View 或授权结论；不裁决缺失；
    /// 不开启或提交事务；软删除行永不命中。
    pub async fn find_latest_by_differences(
        &self,
        difference_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ReconciliationDifferenceResolution>> {
        let ids = dedupe_difference_ids(difference_ids);
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let records = self.find_many(latest_batch_filter(&ids), executor).await?;
        Ok(latest_per_difference(records))
    }
}

#[cfg(test)]
mod tests {
    use entities::common::time::Instant;
    use entities::integration_ops::{
        ReconciliationDifferenceId, ReconciliationDifferenceResolution,
        ReconciliationDifferenceResolutionData, ReconciliationDifferenceResolutionId, ResolutionAction,
    };

    use super::{dedupe_difference_ids, latest_batch_filter, latest_per_difference};

    /// 构造最小决定记录。
    ///
    /// # 参数
    /// * `difference` - 所属差异 ID
    /// * `record` - 记录 ID
    /// * `resolution_no` - 决定序号
    ///
    /// # 返回
    /// 返回可用于内存选择的决定记录。
    fn resolution(difference: &str, record: &str, resolution_no: u32) -> ReconciliationDifferenceResolution {
        ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new(record.to_string()),
            ReconciliationDifferenceResolutionData {
                reconciliation_difference_id: ReconciliationDifferenceId::new(difference.to_string()),
                resolution_no,
                resolution_action: ResolutionAction::QueryOriginalResult,
                resulting_status: ResolutionAction::QueryOriginalResult.derived_status(),
                evidence_reference: None,
                handled_by: "actor-1".to_string(),
                handled_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap()
    }

    #[test]
    fn dedupe_ids_keeps_first_order_and_handles_empty() {
        assert!(dedupe_difference_ids(&[]).is_empty());
        assert_eq!(
            dedupe_difference_ids(&["d1".to_string(), "d2".to_string(), "d1".to_string(),]),
            vec!["d1".to_string(), "d2".to_string()]
        );
    }

    #[test]
    fn batch_filter_uses_in_clause_and_soft_delete() {
        let filter = latest_batch_filter(&["d1".to_string(), "d2".to_string()]);
        let clause = filter.get_document("reconciliation_difference_id").unwrap();
        let ids: Vec<String> = clause
            .get_array("$in")
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, vec!["d1".to_string(), "d2".to_string()]);
        assert_eq!(
            filter.get_i64("deleted_at").unwrap(),
            entity_core::NOT_DELETED_TIMESTAMP as i64
        );
    }

    #[test]
    fn latest_per_difference_picks_max_resolution_no() {
        let records = vec![
            resolution("d1", "r1", 1),
            resolution("d1", "r2", 3),
            resolution("d1", "r3", 2),
            resolution("d2", "r4", 1),
        ];
        let latest = latest_per_difference(records);
        assert_eq!(latest.len(), 2);
        assert_eq!(latest["d1"].resolution_no, 3);
        assert_eq!(latest["d1"].base.id, "r2");
        assert_eq!(latest["d2"].resolution_no, 1);
    }

    #[test]
    fn latest_per_difference_omits_differences_without_resolutions() {
        let records = vec![resolution("d1", "r1", 1)];
        let latest = latest_per_difference(records);
        assert!(latest.contains_key("d1"));
        assert!(!latest.contains_key("d2"));
        assert!(latest_per_difference(Vec::new()).is_empty());
    }
}
