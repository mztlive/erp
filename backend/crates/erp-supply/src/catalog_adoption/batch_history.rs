//! 批次列表只读取行状态摘要，不加载来源商品、定价及执行载荷。

use std::collections::HashMap;

use futures_util::TryStreamExt;
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result};
use serde::{Deserialize, Serialize};

use super::{BatchItemStatus, CatalogAdoptionBatch, PreparationStatus, recent_batches};

/// 可重开的批次及基于当前成员状态计算的列表状态。
#[derive(Debug, Serialize)]
pub struct BatchHistoryEntry {
    #[serde(flatten)]
    pub batch: CatalogAdoptionBatch,
    pub history_status: BatchHistoryStatus,
}

/// 列表状态不以已提交标志代替已完成结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchHistoryStatus {
    Preparing,
    PreparationFailed,
    Cancelled,
    Processing,
    Pending,
    Draft,
    Completed,
}

impl BatchHistoryStatus {
    /// 根据批次提交事实和完整成员状态集合判断列表状态。
    /// # 参数
    /// `submitted`、`execution_pending` 为批次事实；`states` 为聚合状态集合。
    /// # 返回
    /// 执行优先；未提交归草稿；已提交且全部为终态时才归已完成。
    /// # 错误
    /// 不返回错误；缺少成员摘要时按待处理保守展示。
    fn from_states(submitted: bool, execution_pending: bool, states: &[BatchItemStatus]) -> Self {
        if execution_pending || states.contains(&BatchItemStatus::Processing) {
            Self::Processing
        } else if !submitted {
            Self::Draft
        } else if !states.is_empty()
            && states.iter().all(|state| {
                matches!(
                    state,
                    BatchItemStatus::Succeeded | BatchItemStatus::Skipped | BatchItemStatus::Excluded
                )
            })
        {
            Self::Completed
        } else {
            Self::Pending
        }
    }
}

#[derive(Debug, Deserialize)]
struct BatchStates {
    #[serde(rename = "_id")]
    batch_id: String,
    states: Vec<BatchItemStatus>,
}

/// 查询当前已授权连接最近二十个批次，并一次聚合其成员状态。
/// # 参数
/// `connection` 须已核验当前权限及供应商范围；`executor` 沿用调用方事务。
/// # 返回
/// 最近批次及当前状态，保持批次原有倒序；不返回成员详情。
/// # 错误
/// 数据库读取、聚合或状态解析失败时返回错误。
pub async fn batch_history(
    db: &Database,
    connection: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<BatchHistoryEntry>> {
    let batches = recent_batches(db, connection, None, executor).await?;
    if batches.is_empty() {
        return Ok(Vec::new());
    }
    let ids: Vec<_> = batches.iter().map(|batch| batch.base.id.as_str()).collect();
    let pipeline = vec![
        doc! {"$match":{"batch_id":{"$in":ids},"deleted_at":0_i64}},
        doc! {"$group":{"_id":"$batch_id","states":{"$addToSet":"$status"}}},
    ];
    let collection = db.collection::<BatchStates>("supplier_catalog_adoption_batch_items");
    let states: Vec<BatchStates> = match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<BatchStates>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect()
                .await?
        },
        None => collection.aggregate(pipeline).with_type::<BatchStates>().await?.try_collect().await?,
    };
    let states: HashMap<_, _> = states.into_iter().map(|entry| (entry.batch_id, entry.states)).collect();
    Ok(batches
        .into_iter()
        .map(|batch| {
            let history_status = match batch.preparation.as_ref().map(|state| state.status) {
                Some(PreparationStatus::Queued | PreparationStatus::Preparing) => {
                    BatchHistoryStatus::Preparing
                },
                Some(PreparationStatus::Failed) => BatchHistoryStatus::PreparationFailed,
                Some(PreparationStatus::Cancelled) => BatchHistoryStatus::Cancelled,
                _ => BatchHistoryStatus::from_states(
                    batch.submitted,
                    batch.execution_pending,
                    states.get(&batch.base.id).map(Vec::as_slice).unwrap_or_default(),
                ),
            };
            BatchHistoryEntry { batch, history_status }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn submitted_batch_requires_all_terminal_states_to_be_completed() {
        use BatchItemStatus::{Draft, Excluded, Failed, Ready, Skipped, Succeeded, Unknown};
        assert_eq!(
            BatchHistoryStatus::from_states(true, false, &[Succeeded, Skipped, Excluded]),
            BatchHistoryStatus::Completed
        );
        for state in [Draft, Ready, Failed, Unknown] {
            assert_eq!(
                BatchHistoryStatus::from_states(true, false, &[Succeeded, state]),
                BatchHistoryStatus::Pending
            );
        }
        assert_eq!(BatchHistoryStatus::from_states(true, false, &[]), BatchHistoryStatus::Pending);
    }

    #[test]
    fn execution_takes_priority_over_pending_and_draft() {
        assert_eq!(
            BatchHistoryStatus::from_states(true, true, &[BatchItemStatus::Failed]),
            BatchHistoryStatus::Processing
        );
        assert_eq!(
            BatchHistoryStatus::from_states(
                true,
                false,
                &[BatchItemStatus::Processing, BatchItemStatus::Failed]
            ),
            BatchHistoryStatus::Processing
        );
        assert_eq!(
            BatchHistoryStatus::from_states(false, false, &[BatchItemStatus::Ready]),
            BatchHistoryStatus::Draft
        );
        assert_eq!(BatchHistoryStatus::from_states(false, false, &[]), BatchHistoryStatus::Draft);
    }
}
