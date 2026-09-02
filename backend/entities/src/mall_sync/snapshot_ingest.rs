//! 商城销售单快照落盘分类（INT-R16）。
//!
//! 先按事实身份 `(source_system_id, external_order_key, source_updated_at)` exact
//! 去重，再按版本时间稳定排序，并用 [`MallSalesOrderSnapshot::supersedes_candidate`]
//! 同一规则把剩余项分为 `Duplicate` / `Stale` / `Accept`。不读写数据库、不生成 ID。

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::ids::SourceSystemId;

use super::{ExternalOrderKey, MallSalesOrderSnapshot};

/// 快照落盘分类结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotIngestDecision {
    /// 与已有或本批更早出现的事实键完全相同，应幂等跳过。
    Duplicate,
    /// 已有更新的来源版本（库内最新或本批已接受项），应丢弃。
    Stale,
    /// 应落盘的新版本。
    Accept,
}

/// 来源单身份：同一商城下的一个来源单。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SnapshotOrderIdentity {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 来源单二进制比较键。
    pub external_order_key: ExternalOrderKey,
}

/// 快照事实身份 / 最新版本最小事实。
///
/// 同时用于批内候选项、库内 exact 键和 latest 投影；版本比较复用
/// [`SnapshotFactIdentity::supersedes_candidate`]。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SnapshotFactIdentity {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 来源单二进制比较键。
    pub external_order_key: ExternalOrderKey,
    /// 商城更新时间。
    pub source_updated_at: Instant,
}

impl SnapshotFactIdentity {
    /// 构造事实身份。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    /// * `external_order_key` - 来源单比较键
    /// * `source_updated_at` - 商城更新时间
    ///
    /// # 返回
    /// 返回事实身份。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不规范化来源单号；调用方须传入已 trim 的比较键。
    pub fn new(
        source_system_id: SourceSystemId,
        external_order_key: ExternalOrderKey,
        source_updated_at: Instant,
    ) -> Self {
        Self {
            source_system_id,
            external_order_key,
            source_updated_at,
        }
    }

    /// 从已构造快照提取事实身份。
    ///
    /// # 参数
    /// * `snapshot` - 快照实体
    ///
    /// # 返回
    /// 返回与持久化事实键一致的身份。
    ///
    /// # 错误
    /// 无。
    pub fn from_snapshot(snapshot: &MallSalesOrderSnapshot) -> Self {
        Self::new(
            snapshot.source_system_id.clone(),
            snapshot.external_order_key.clone(),
            snapshot.source_updated_at,
        )
    }

    /// 返回来源单身份（不含版本时间）。
    ///
    /// # 返回
    /// 返回 `(来源商城, 比较键)`。
    pub fn order_identity(&self) -> SnapshotOrderIdentity {
        SnapshotOrderIdentity {
            source_system_id: self.source_system_id.clone(),
            external_order_key: self.external_order_key.clone(),
        }
    }

    /// 判断本事实版本是否严格新于候选项。
    ///
    /// 与 [`MallSalesOrderSnapshot::supersedes_candidate`] 使用同一比较：
    /// `source_updated_at > candidate_updated_at`。等时不是迟到。
    ///
    /// # 参数
    /// * `candidate_updated_at` - 待接收快照的来源更新时间
    ///
    /// # 返回
    /// 本事实严格更新时返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn supersedes_candidate(&self, candidate_updated_at: Instant) -> bool {
        self.source_updated_at > candidate_updated_at
    }
}

/// 一页快照相对库内事实的落盘分类计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotIngestPlan {
    decisions: Vec<SnapshotIngestDecision>,
}

impl SnapshotIngestPlan {
    /// 按 exact 去重再按版本时间分类本批候选项。
    ///
    /// 处理顺序：
    /// 1. 事实键与库内 exact 集合相同，或在本批首次出现之后再次出现 → `Duplicate`；
    /// 2. 剩余项按 `source_updated_at` 降序、原下标升序稳定排序；
    /// 3. 相对库内 latest 与本批已接受项调用 [`SnapshotFactIdentity::supersedes_candidate`]：
    ///    被新版本覆盖 → `Stale`，等时 → `Duplicate`，否则 → `Accept` 并推进该来源单 latest。
    ///
    /// # 参数
    /// * `candidates` - 本页候选项（保持请求原序）
    /// * `existing_exact` - 库内已存在的精确事实键
    /// * `latest` - 各来源单当前最新快照最小事实
    ///
    /// # 返回
    /// 返回与 `candidates` 等长的分类计划。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 纯内存确定性计算；不代替唯一索引或单调水位 CAS。
    pub fn classify(
        candidates: &[SnapshotFactIdentity],
        existing_exact: &[SnapshotFactIdentity],
        latest: &[SnapshotFactIdentity],
    ) -> Self {
        let mut decisions = vec![SnapshotIngestDecision::Accept; candidates.len()];
        let pending = mark_exact_duplicates(candidates, existing_exact, &mut decisions);
        classify_versions(candidates, latest, &pending, &mut decisions);
        Self { decisions }
    }

    /// 返回与候选项下标对齐的分类切片。
    ///
    /// # 返回
    /// 返回只读分类切片。
    pub fn decisions(&self) -> &[SnapshotIngestDecision] {
        &self.decisions
    }

    /// 返回应落盘的候选项下标（原请求顺序）。
    ///
    /// # 返回
    /// 返回 `Accept` 下标。
    pub fn accepted_indexes(&self) -> impl Iterator<Item = usize> + '_ {
        self.decisions
            .iter()
            .enumerate()
            .filter_map(|(index, decision)| (*decision == SnapshotIngestDecision::Accept).then_some(index))
    }

    /// 返回应跳过的条数（`Duplicate` + `Stale`）。
    ///
    /// # 返回
    /// 返回跳过计数。
    pub fn skipped_count(&self) -> u64 {
        self.decisions
            .iter()
            .filter(|decision| **decision != SnapshotIngestDecision::Accept)
            .count() as u64
    }
}

/// 标记库内与批内 exact 重复，返回待按版本分类的下标。
///
/// # 参数
/// * `candidates` - 本页候选项
/// * `existing_exact` - 库内精确事实键
/// * `decisions` - 与候选项等长的分类输出
///
/// # 返回
/// 返回尚未判定为 `Duplicate` 的下标。
///
/// # 错误
/// 无。
fn mark_exact_duplicates(
    candidates: &[SnapshotFactIdentity],
    existing_exact: &[SnapshotFactIdentity],
    decisions: &mut [SnapshotIngestDecision],
) -> Vec<usize> {
    let existing: HashSet<&SnapshotFactIdentity> = existing_exact.iter().collect();
    let mut seen: HashSet<&SnapshotFactIdentity> = HashSet::new();
    let mut pending = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if existing.contains(candidate) || !seen.insert(candidate) {
            decisions[index] = SnapshotIngestDecision::Duplicate;
        } else {
            pending.push(index);
        }
    }
    pending
}

/// 按版本时间降序分类剩余项。
///
/// # 参数
/// * `candidates` - 本页候选项
/// * `latest` - 库内最新事实
/// * `pending` - 非 exact 重复的下标
/// * `decisions` - 分类输出
///
/// # 错误
/// 无。
///
/// # 约束
/// 同来源单只接受本批最新且严格新于库内 latest 的一项；先新后旧与先旧后新结果一致。
fn classify_versions(
    candidates: &[SnapshotFactIdentity],
    latest: &[SnapshotFactIdentity],
    pending: &[usize],
    decisions: &mut [SnapshotIngestDecision],
) {
    let mut latest_by_order = fold_latest_by_order(latest);
    let mut ordered = pending.to_vec();
    ordered.sort_by(|&left, &right| {
        candidates[right]
            .source_updated_at
            .cmp(&candidates[left].source_updated_at)
            .then(left.cmp(&right))
    });
    for index in ordered {
        let candidate = &candidates[index];
        let order = candidate.order_identity();
        if let Some(existing_at) = latest_by_order.get(&order).copied() {
            let existing = SnapshotFactIdentity::new(
                candidate.source_system_id.clone(),
                candidate.external_order_key.clone(),
                existing_at,
            );
            if existing.supersedes_candidate(candidate.source_updated_at) {
                decisions[index] = SnapshotIngestDecision::Stale;
                continue;
            }
            if existing_at == candidate.source_updated_at {
                decisions[index] = SnapshotIngestDecision::Duplicate;
                continue;
            }
        }
        decisions[index] = SnapshotIngestDecision::Accept;
        latest_by_order.insert(order, candidate.source_updated_at);
    }
}

/// 按来源单折叠最新商城更新时间。
///
/// # 参数
/// * `latest` - 库内最新事实（同一来源单可出现多次，取最大时间）
///
/// # 返回
/// 返回来源单到最新时间的映射。
fn fold_latest_by_order(latest: &[SnapshotFactIdentity]) -> HashMap<SnapshotOrderIdentity, Instant> {
    let mut latest_by_order = HashMap::new();
    for fact in latest {
        let order = fact.order_identity();
        latest_by_order
            .entry(order)
            .and_modify(|current: &mut Instant| {
                if fact.source_updated_at > *current {
                    *current = fact.source_updated_at;
                }
            })
            .or_insert(fact.source_updated_at);
    }
    latest_by_order
}

#[cfg(test)]
mod tests {
    use super::{
        fold_latest_by_order, SnapshotFactIdentity, SnapshotIngestDecision, SnapshotIngestPlan,
        SnapshotOrderIdentity,
    };
    use crate::common::time::Instant;
    use crate::ids::{MallSalesOrderSnapshotId, MallSalesSyncJobId, SourceSystemId};
    use crate::mall_sync::{
        ExternalOrderKey, MallSalesOrderSnapshot, MallSalesOrderSnapshotData, SnapshotMappingStatus,
    };

    fn at(secs: i64) -> Instant {
        Instant::from_unix_secs(secs)
    }

    fn fact(order: &str, secs: i64) -> SnapshotFactIdentity {
        fact_on("sys-mall", order, secs)
    }

    fn fact_on(source: &str, order: &str, secs: i64) -> SnapshotFactIdentity {
        SnapshotFactIdentity::new(
            SourceSystemId::new(source),
            ExternalOrderKey::from_trimmed(order),
            at(secs),
        )
    }

    fn snapshot_at(id: &str, order: &str, secs: i64) -> MallSalesOrderSnapshot {
        MallSalesOrderSnapshot::new(
            MallSalesOrderSnapshotId::new(id),
            MallSalesOrderSnapshotData {
                source_system_id: SourceSystemId::new("sys-mall"),
                external_order_no: order.to_string(),
                source_updated_at: at(secs),
                content_hash: None,
                source_status_code: "EFFECTIVE".to_string(),
                normalized_snapshot: "{\"sell_order\":\"x\"}".to_string(),
                raw_payload_reference: None,
                observed_at: at(secs + 1),
                sync_job_id: MallSalesSyncJobId::new("j-1"),
            },
        )
        .unwrap()
    }

    fn decisions_of(
        candidates: &[SnapshotFactIdentity],
        exact: &[SnapshotFactIdentity],
        latest: &[SnapshotFactIdentity],
    ) -> Vec<SnapshotIngestDecision> {
        SnapshotIngestPlan::classify(candidates, exact, latest)
            .decisions()
            .to_vec()
    }

    #[test]
    fn empty_candidates_yield_empty_plan() {
        let plan = SnapshotIngestPlan::classify(&[], &[], &[]);
        assert!(plan.decisions().is_empty());
        assert_eq!(plan.skipped_count(), 0);
        assert_eq!(plan.accepted_indexes().count(), 0);
    }

    #[test]
    fn in_batch_exact_duplicate_keeps_first() {
        let candidates = [fact("SO-1", 10), fact("SO-1", 10), fact("SO-2", 10)];
        let decisions = decisions_of(&candidates, &[], &[]);
        assert_eq!(
            decisions,
            vec![
                SnapshotIngestDecision::Accept,
                SnapshotIngestDecision::Duplicate,
                SnapshotIngestDecision::Accept,
            ]
        );
    }

    #[test]
    fn newer_then_older_in_batch_discards_stale() {
        let candidates = [fact("SO-1", 20), fact("SO-1", 10)];
        let decisions = decisions_of(&candidates, &[], &[]);
        assert_eq!(
            decisions,
            vec![SnapshotIngestDecision::Accept, SnapshotIngestDecision::Stale]
        );
    }

    #[test]
    fn older_then_newer_in_batch_still_keeps_only_newest() {
        let candidates = [fact("SO-1", 10), fact("SO-1", 20)];
        let decisions = decisions_of(&candidates, &[], &[]);
        assert_eq!(
            decisions,
            vec![SnapshotIngestDecision::Stale, SnapshotIngestDecision::Accept]
        );
    }

    #[test]
    fn database_exact_duplicate_is_skipped() {
        let candidates = [fact("SO-1", 10), fact("SO-2", 10)];
        let exact = [fact("SO-1", 10)];
        let decisions = decisions_of(&candidates, &exact, &exact);
        assert_eq!(
            decisions,
            vec![SnapshotIngestDecision::Duplicate, SnapshotIngestDecision::Accept]
        );
    }

    #[test]
    fn equal_time_against_latest_is_duplicate_not_stale() {
        let candidates = [fact("SO-1", 10)];
        let latest = [fact("SO-1", 10)];
        let decisions = decisions_of(&candidates, &[], &latest);
        assert_eq!(decisions, vec![SnapshotIngestDecision::Duplicate]);
    }

    #[test]
    fn historical_exact_duplicate_is_not_confused_with_latest() {
        let candidates = [fact("SO-1", 10)];
        let exact = [fact("SO-1", 10)];
        let latest = [fact("SO-1", 30)];
        let decisions = decisions_of(&candidates, &exact, &latest);
        assert_eq!(decisions, vec![SnapshotIngestDecision::Duplicate]);
    }

    #[test]
    fn older_than_database_latest_is_stale() {
        let candidates = [fact("SO-1", 10)];
        let latest = [fact("SO-1", 20)];
        let decisions = decisions_of(&candidates, &[], &latest);
        assert_eq!(decisions, vec![SnapshotIngestDecision::Stale]);
    }

    #[test]
    fn newer_than_database_latest_is_accepted() {
        let candidates = [fact("SO-1", 30)];
        let latest = [fact("SO-1", 20)];
        let decisions = decisions_of(&candidates, &[], &latest);
        assert_eq!(decisions, vec![SnapshotIngestDecision::Accept]);
    }

    #[test]
    fn mixed_orders_do_not_share_latest() {
        let candidates = [fact("SO-1", 10), fact("SO-2", 5), fact("SO-1", 20)];
        let latest = [fact("SO-2", 8)];
        let decisions = decisions_of(&candidates, &[], &latest);
        assert_eq!(
            decisions,
            vec![
                SnapshotIngestDecision::Stale,
                SnapshotIngestDecision::Stale,
                SnapshotIngestDecision::Accept,
            ]
        );
    }

    #[test]
    fn classification_matches_per_item_snapshot_rule() {
        let db_latest = snapshot_at("snap-db", "SO-1", 20);
        let newer = snapshot_at("snap-new", "SO-1", 30);
        let older = snapshot_at("snap-old", "SO-1", 10);
        let equal = snapshot_at("snap-eq", "SO-1", 20);

        assert!(db_latest.supersedes_candidate(older.source_updated_at));
        assert!(!db_latest.supersedes_candidate(db_latest.source_updated_at));
        assert!(!db_latest.supersedes_candidate(newer.source_updated_at));

        let latest = [SnapshotFactIdentity::from_snapshot(&db_latest)];
        let candidates = [
            SnapshotFactIdentity::from_snapshot(&newer),
            SnapshotFactIdentity::from_snapshot(&older),
            SnapshotFactIdentity::from_snapshot(&equal),
        ];
        let decisions = decisions_of(
            &candidates,
            &[SnapshotFactIdentity::from_snapshot(&equal)],
            &latest,
        );
        assert_eq!(
            decisions,
            vec![
                SnapshotIngestDecision::Accept,
                SnapshotIngestDecision::Stale,
                SnapshotIngestDecision::Duplicate,
            ]
        );
        assert_eq!(
            SnapshotFactIdentity::from_snapshot(&db_latest).supersedes_candidate(older.source_updated_at),
            db_latest.supersedes_candidate(older.source_updated_at)
        );
    }

    #[test]
    fn skipped_count_and_accepted_indexes_follow_original_order() {
        let plan =
            SnapshotIngestPlan::classify(&[fact("SO-1", 10), fact("SO-1", 20), fact("SO-1", 20)], &[], &[]);
        assert_eq!(plan.skipped_count(), 2);
        assert_eq!(plan.accepted_indexes().collect::<Vec<_>>(), vec![1]);
    }

    #[test]
    fn fold_latest_keeps_max_time_per_order() {
        let folded = fold_latest_by_order(&[fact("SO-1", 10), fact("SO-1", 30), fact("SO-2", 5)]);
        assert_eq!(
            folded.get(&SnapshotOrderIdentity {
                source_system_id: SourceSystemId::new("sys-mall"),
                external_order_key: ExternalOrderKey::from_trimmed("SO-1"),
            }),
            Some(&at(30))
        );
        assert_eq!(
            folded.get(&SnapshotOrderIdentity {
                source_system_id: SourceSystemId::new("sys-mall"),
                external_order_key: ExternalOrderKey::from_trimmed("SO-2"),
            }),
            Some(&at(5))
        );
    }

    #[test]
    fn snapshot_mapping_status_pending_is_unchanged_by_identity() {
        let snapshot = snapshot_at("snap-1", "SO-1", 10);
        assert_eq!(snapshot.mapping_status, SnapshotMappingStatus::Pending);
        assert_eq!(
            SnapshotFactIdentity::from_snapshot(&snapshot).external_order_key,
            ExternalOrderKey::from_trimmed("SO-1")
        );
    }
}
