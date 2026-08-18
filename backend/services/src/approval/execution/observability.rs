//! 审批运行低基数指标。实例 ID 与用户 ID 不得作为标签。

use bpm::model::types::ApprovalProcessInstanceStatus;

/// 运行指标快照。只使用低基数计数。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ApprovalRuntimeMetrics {
    /// 当前 BLOCKED 实例数。
    pub blocked_count: u64,
    /// BLOCKED 最长持续秒。
    pub blocked_oldest_age_secs: u64,
    /// 决定冲突次数。
    pub decision_conflicts: u64,
    /// 幂等冲突次数。
    pub idempotency_conflicts: u64,
    /// 决定延迟毫秒样本和。
    pub decision_latency_ms_sum: u64,
    /// 决定次数。
    pub decision_count: u64,
    /// 没有 OPEN 任务的 ACTIVE 执行数。
    pub active_without_open_task: u64,
    /// outbox 积压。
    pub outbox_backlog: u64,
    /// outbox 最老未投递秒。
    pub outbox_oldest_age_secs: u64,
    /// outbox 重试次数。
    pub outbox_retries: u64,
    /// outbox 死信次数。
    pub outbox_dead_letters: u64,
}

impl ApprovalRuntimeMetrics {
    /// 记录一次决定冲突。
    ///
    /// # 返回
    /// 无。
    pub fn record_decision_conflict(&mut self) {
        self.decision_conflicts = self.decision_conflicts.saturating_add(1);
    }

    /// 记录一次幂等冲突。
    ///
    /// # 返回
    /// 无。
    pub fn record_idempotency_conflict(&mut self) {
        self.idempotency_conflicts = self.idempotency_conflicts.saturating_add(1);
    }

    /// 记录一次决定延迟。
    ///
    /// # 参数
    /// * `latency_ms` - 延迟毫秒
    ///
    /// # 返回
    /// 无。
    pub fn record_decision_latency(&mut self, latency_ms: u64) {
        self.decision_count = self.decision_count.saturating_add(1);
        self.decision_latency_ms_sum = self.decision_latency_ms_sum.saturating_add(latency_ms);
    }

    /// 记录 outbox 重试。
    ///
    /// # 返回
    /// 无。
    pub fn record_outbox_retry(&mut self) {
        self.outbox_retries = self.outbox_retries.saturating_add(1);
    }

    /// 记录 outbox 死信。
    ///
    /// # 返回
    /// 无。
    pub fn record_outbox_dead_letter(&mut self) {
        self.outbox_dead_letters = self.outbox_dead_letters.saturating_add(1);
    }

    /// 用实例状态刷新 BLOCKED 计数。
    ///
    /// # 参数
    /// * `statuses` - 实例状态列表
    ///
    /// # 返回
    /// 无。
    pub fn refresh_blocked_count(&mut self, statuses: &[ApprovalProcessInstanceStatus]) {
        self.blocked_count = statuses
            .iter()
            .filter(|status| **status == ApprovalProcessInstanceStatus::Blocked)
            .count() as u64;
    }
}

/// dashboard/runbook 入口名称。不得包含实例或用户 ID。
pub const BLOCKED_DASHBOARD: &str = "approval.runtime.blocked";
/// 决定冲突 dashboard。
pub const DECISION_CONFLICT_DASHBOARD: &str = "approval.runtime.decision_conflicts";
/// outbox dashboard。
pub const OUTBOX_DASHBOARD: &str = "approval.runtime.outbox";

#[cfg(test)]
mod tests {
    use super::{ApprovalRuntimeMetrics, BLOCKED_DASHBOARD, DECISION_CONFLICT_DASHBOARD, OUTBOX_DASHBOARD};
    use bpm::model::types::ApprovalProcessInstanceStatus;

    /// 指标不含实例或用户标签，并具备 dashboard 入口。
    #[test]
    fn execution_metrics_are_low_cardinality() {
        let mut metrics = ApprovalRuntimeMetrics::default();
        metrics.record_decision_conflict();
        metrics.record_idempotency_conflict();
        metrics.record_decision_latency(12);
        metrics.record_outbox_retry();
        metrics.record_outbox_dead_letter();
        metrics.refresh_blocked_count(&[
            ApprovalProcessInstanceStatus::Blocked,
            ApprovalProcessInstanceStatus::Running,
        ]);
        assert_eq!(metrics.blocked_count, 1);
        assert_eq!(metrics.decision_conflicts, 1);
        assert_eq!(BLOCKED_DASHBOARD, "approval.runtime.blocked");
        assert_eq!(DECISION_CONFLICT_DASHBOARD, "approval.runtime.decision_conflicts");
        assert_eq!(OUTBOX_DASHBOARD, "approval.runtime.outbox");
    }
}
