//! 映射任务页面批量事实装载（INT-R17）。
//!
//! 列表页按当前页任务 ID 一次性装载完整任务、来源快照与最新归集操作，查询
//! 次数不随页内行数增长。正式责任行、审计时间线、来源系统与谱系映射/目标由
//! 各自属主仓储的批量接口提供，本文件只补充 `mall_sync` 拥有的三块批量读取。

use std::collections::{HashMap, HashSet};

use entities::mall_sync::{MallSalesOrderSnapshot, MallSnapshotReapplyOperation, MasterMappingTask};
use mongodb::bson::{doc, Document};

use super::super::Repository;
use crate::executor::Executor;
use crate::Result;

/// 按 ID 去重并保持首次出现顺序（页面装载的输入归一化）。
///
/// # 参数
/// * `ids` - 原始 ID 迭代器（可含重复）
///
/// # 返回
/// 返回去重后的 ID；空输入返回空集合。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯内存函数，不访问数据库。
fn dedupe_ids(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id);
        }
    }
    unique
}

/// 构造 `$in` 主键过滤（调用方已去重；空输入由调用方短路）。
///
/// # 参数
/// * `ids` - 去重后的主键集合（非空）
///
/// # 返回
/// 返回主键 `$in` 过滤文档。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯过滤构造，不访问数据库；软删除过滤由基类统一追加。
fn ids_in_filter(ids: &[String]) -> Document {
    doc! { "id": { "$in": ids } }
}

/// 在内存中按映射任务取最新归集操作（与单任务查询同序）。
///
/// 最新定义与 [`Repository::<MallSnapshotReapplyOperation>::latest_reapply_for_task`]
/// 一致：`last_updated_at` 降序、`created_at` 降序、同值时 `id` 升序稳定。
/// 每任务只保留一条；无操作的任务不出现。
///
/// # 参数
/// * `operations` - 批量取回的归集操作（可含多任务、多条）
///
/// # 返回
/// 返回按映射任务 ID 索引的最新操作。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯内存选择，不访问数据库；排序键与单任务数据库排序同义。
fn latest_reapply_per_task(
    operations: Vec<MallSnapshotReapplyOperation>,
) -> HashMap<String, MallSnapshotReapplyOperation> {
    let mut latest: HashMap<String, MallSnapshotReapplyOperation> = HashMap::new();
    for operation in operations {
        let key = operation.mapping_task_id.to_string();
        let replace = match latest.get(&key) {
            None => true,
            Some(current) => reapply_sort_key(&operation) > reapply_sort_key(current),
        };
        if replace {
            latest.insert(key, operation);
        }
    }
    latest
}

/// 归集操作的内存排序键（越大越新；与数据库排序同义）。
///
/// # 参数
/// * `operation` - 待比较的归集操作
///
/// # 返回
/// 返回 `(last_updated_at, created_at, Reverse(id))` 比较键。
///
/// # 错误
/// 无错误返回。
///
/// # 约束
/// 纯比较键构造；同值时 `id` 升序的一条胜出，与单任务查询一致。
fn reapply_sort_key(operation: &MallSnapshotReapplyOperation) -> (i64, u64, std::cmp::Reverse<String>) {
    (
        operation.last_updated_at.unix_secs(),
        operation.base.created_at,
        std::cmp::Reverse(operation.base.id.clone()),
    )
}

impl<'a> Repository<'a, MasterMappingTask> {
    /// 按页面任务 ID 集合批量读取完整映射任务（INT-R17）。
    ///
    /// 一次 `$in` 查询替代逐行 `find_by_id`；空输入不访问数据库。软删除、
    /// 稳定排序与缺失语义与单任务读取一致，缺项由 Service 解释为内部错误。
    ///
    /// # 参数
    /// * `task_ids` - 本页任务 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的映射任务；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回实体，不返回 services DTO、HTTP View 或授权结论。
    pub async fn find_mapping_tasks_by_ids(
        &self,
        task_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MasterMappingTask>> {
        let ids = dedupe_ids(task_ids.iter().cloned());
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(ids_in_filter(&ids), executor).await
    }
}

impl<'a> Repository<'a, MallSalesOrderSnapshot> {
    /// 按快照 ID 集合批量读取来源快照（INT-R17）。
    ///
    /// 一次 `$in` 查询替代逐行 `find_by_id`；空输入不访问数据库。缺项由
    /// Service 解释为内部错误（映射任务引用的来源快照不存在）。
    ///
    /// # 参数
    /// * `snapshot_ids` - 快照 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配且未删除的快照；返回顺序不承诺与输入一致。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 只返回实体，不返回 services DTO、HTTP View 或授权结论。
    pub async fn find_snapshots_by_ids(
        &self,
        snapshot_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallSalesOrderSnapshot>> {
        let ids = dedupe_ids(snapshot_ids.iter().cloned());
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(ids_in_filter(&ids), executor).await
    }
}

impl<'a> Repository<'a, MallSnapshotReapplyOperation> {
    /// 按页面任务 ID 集合批量读取各任务最新归集操作（INT-R17）。
    ///
    /// 一次 `$in` 查询装载本页全部归集操作，再按单任务查询同序在内存中取每
    /// 任务最新一条；空输入不访问数据库。无操作的任务不出现在结果中。
    ///
    /// # 参数
    /// * `mapping_task_ids` - 本页映射任务 ID 集合；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按映射任务 ID 索引的最新归集操作。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 约束
    /// 最新语义与 [`Self::latest_reapply_for_task`] 一致；不裁决缺失。
    pub async fn find_reapply_latest_by_task_ids(
        &self,
        mapping_task_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, MallSnapshotReapplyOperation>> {
        let ids = dedupe_ids(mapping_task_ids.iter().cloned());
        if ids.is_empty() {
            return Ok(HashMap::new());
        }
        let operations = self
            .find_many(doc! { "mapping_task_id": { "$in": ids } }, executor)
            .await?;
        Ok(latest_reapply_per_task(operations))
    }
}

#[cfg(test)]
mod tests {
    use super::{dedupe_ids, ids_in_filter, latest_reapply_per_task, reapply_sort_key};
    use entities::common::time::Instant;
    use entities::ids::{MallSalesOrderSnapshotId, MasterMappingTaskId};
    use entities::mall_sync::{MallSnapshotReapplyOperation, MallSnapshotReapplyOperationData};
    use mongodb::bson::doc;

    fn reapply(
        task: &str,
        operation: &str,
        requested_secs: i64,
        last_secs: i64,
    ) -> MallSnapshotReapplyOperation {
        let mut entity = MallSnapshotReapplyOperation::new(
            operation.to_string(),
            MallSnapshotReapplyOperationData {
                mapping_task_id: MasterMappingTaskId::new(task.to_string()),
                source_snapshot_id: MallSalesOrderSnapshotId::new("snapshot-1".to_string()),
                idempotency_key_hash: format!("hash-{operation}"),
                command_fingerprint: format!("fingerprint-{operation}"),
                requested_by: "actor-1".to_string(),
                requested_at: Instant::from_unix_secs(requested_secs),
            },
        )
        .unwrap();
        entity.last_updated_at = Instant::from_unix_secs(last_secs);
        entity
    }

    #[test]
    fn dedupe_ids_keeps_first_order_and_handles_empty() {
        assert!(dedupe_ids(Vec::<String>::new()).is_empty());
        assert_eq!(
            dedupe_ids(vec!["t-2".to_string(), "t-1".to_string(), "t-2".to_string(),]),
            vec!["t-2".to_string(), "t-1".to_string()]
        );
    }

    #[test]
    fn ids_in_filter_uses_in_clause() {
        let filter = ids_in_filter(&["a".to_string(), "b".to_string()]);
        assert_eq!(
            filter.get_document("id").unwrap().get_array("$in").unwrap().len(),
            2
        );
        assert_eq!(filter.get_document("id").unwrap(), &doc! { "$in": ["a", "b"] });
    }

    #[test]
    fn latest_reapply_per_task_picks_newest_and_groups() {
        let operations = vec![
            reapply("task-1", "op-old", 10, 20),
            reapply("task-1", "op-new", 11, 30),
            reapply("task-2", "op-only", 12, 15),
        ];
        let latest = latest_reapply_per_task(operations);
        assert_eq!(latest.len(), 2);
        assert_eq!(latest["task-1"].base.id, "op-new");
        assert_eq!(latest["task-2"].base.id, "op-only");
        assert!(latest_reapply_per_task(Vec::new()).is_empty());
    }

    #[test]
    fn reapply_sort_key_orders_by_last_updated_then_created() {
        let older = reapply("task-1", "op-a", 10, 20);
        let newer = reapply("task-1", "op-b", 10, 21);
        assert!(reapply_sort_key(&newer) > reapply_sort_key(&older));
    }
}
