//! 准备任务：同册同时最多一个活动任务，超期失败且旧运行不得覆盖新结果。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::{SalesSelectionBookletId, SalesSelectionPrepareTaskId};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::combination::TierSearchReport;
use super::limits::PREPARE_TASK_DEADLINE_SECS;
use super::types::{PrepareKind, PrepareStage, PrepareTaskStatus};

/// 准备任务创建数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SalesSelectionPrepareTaskData {
    /// 选品册。
    pub booklet_id: SalesSelectionBookletId,
    /// 启动时册版本。
    pub booklet_version: u64,
    /// 任务种类。
    pub kind: PrepareKind,
    /// 按档重生成的档位；其他种类为空。
    pub tier_ids: Vec<String>,
    /// 幂等键。
    pub idempotency_key: String,
    /// 请求哈希。
    pub request_hash: String,
    /// 搜索种子。
    pub seed: u64,
    /// 创建时间。
    pub now: Instant,
}

/// 准备任务.
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesSelectionPrepareTask {
    /// 原始命令，供排队任务执行；历史任务无命令时明确失败。
    #[serde(default)]
    pub request_json: String,
    /// 原操作人，用于结果审计。
    #[serde(default)]
    pub actor_id: String,
    #[serde(flatten)]
    pub base: BaseModel,
    /// 选品册。
    pub booklet_id: SalesSelectionBookletId,
    /// 启动时的册版本。
    pub booklet_version: u64,
    /// 运行代数，领取时递增。
    pub run_version: u64,
    /// 任务种类。
    pub kind: PrepareKind,
    /// 按档重生成的档位；其他种类为空。
    pub tier_ids: Vec<String>,
    /// 运行状态。
    pub status: PrepareTaskStatus,
    /// 当前阶段。
    pub stage: PrepareStage,
    /// 幂等键。
    pub idempotency_key: String,
    /// 请求哈希。
    pub request_hash: String,
    /// 搜索种子。
    pub seed: u64,
    /// 截止时间。
    pub deadline_at: Instant,
    /// 心跳时间。
    pub heartbeat_at: Instant,
    /// 完成时间。
    pub finished_at: Option<Instant>,
    /// 已完成档位数。
    pub completed_tier_count: u32,
    /// 失败原因。
    pub failure_reason: Option<String>,
    /// 逐档报告。
    pub tier_reports: Vec<TierSearchReport>,
    /// 成功后的批次身份。
    pub result_batch_id: Option<String>,
}

impl SalesSelectionPrepareTask {
    /// 创建排队中的准备任务。
    ///
    /// # 参数
    /// * `id` - 任务身份
    /// * `data` - 创建字段
    ///
    /// # 返回
    /// 返回排队任务，期限 180 秒。
    ///
    /// # 错误
    /// 无。
    pub fn queued(id: SalesSelectionPrepareTaskId, data: SalesSelectionPrepareTaskData) -> Self {
        Self {
            request_json: String::new(),
            actor_id: String::new(),
            base: BaseModel::new(id.to_string()),
            booklet_id: data.booklet_id,
            booklet_version: data.booklet_version,
            run_version: 1,
            kind: data.kind,
            tier_ids: data.tier_ids,
            status: PrepareTaskStatus::Queued,
            stage: PrepareStage::Queued,
            idempotency_key: data.idempotency_key,
            request_hash: data.request_hash,
            seed: data.seed,
            deadline_at: Instant::from_unix_secs(
                data.now.unix_secs().saturating_add(PREPARE_TASK_DEADLINE_SECS as i64),
            ),
            heartbeat_at: data.now,
            finished_at: None,
            completed_tier_count: 0,
            failure_reason: None,
            tier_reports: Vec::new(),
            result_batch_id: None,
        }
    }

    /// 判断结果是否仍可写入。
    ///
    /// # 参数
    /// * `expected_run` - 工作进程持有的运行代数
    /// * `now` - 当前时间
    ///
    /// # 返回
    /// 仍在运行、未到期且代数匹配时通过。
    ///
    /// # 错误
    /// 已结束、到期或代数不匹配时拒绝，调用方不得覆盖有效结果。
    pub fn ensure_writable_run(&self, expected_run: u64, now: Instant) -> Result<()> {
        if self.status != PrepareTaskStatus::Running {
            return Err(Error::from("准备任务已结束，不能写入结果"));
        }
        if self.run_version != expected_run {
            return Err(Error::from("准备任务已被新运行替代"));
        }
        if now >= self.deadline_at {
            return Err(Error::from("准备任务已超过时限"));
        }
        Ok(())
    }

    /// 领取任务进入运行。
    ///
    /// # 参数
    /// * `now` - 领取时间
    ///
    /// # 返回
    /// 运行代数不变（首次）或由仓储在恢复时递增。
    ///
    /// # 错误
    /// 已结束时拒绝。
    pub fn mark_running(&mut self, now: Instant) -> Result<()> {
        if !self.status.is_active() {
            return Err(Error::from("准备任务已结束"));
        }
        self.status = PrepareTaskStatus::Running;
        self.stage = PrepareStage::Snapshot;
        self.heartbeat_at = now;
        Ok(())
    }

    /// 更新阶段与心跳。
    ///
    /// # 参数
    /// * `stage` - 新阶段
    /// * `completed_tier_count` - 已完成档位数
    /// * `now` - 心跳时间
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    pub fn heartbeat(&mut self, stage: PrepareStage, completed_tier_count: u32, now: Instant) {
        self.stage = stage;
        self.completed_tier_count = completed_tier_count;
        self.heartbeat_at = now;
    }

    /// 标记成功。
    ///
    /// # 参数
    /// * `batch_id` - 结果批次
    /// * `reports` - 逐档报告
    /// * `now` - 完成时间
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    pub fn mark_succeeded(&mut self, batch_id: String, reports: Vec<TierSearchReport>, now: Instant) {
        self.status = PrepareTaskStatus::Succeeded;
        self.stage = PrepareStage::Write;
        self.result_batch_id = Some(batch_id);
        self.tier_reports = reports;
        self.finished_at = Some(now);
        self.heartbeat_at = now;
    }

    /// 标记失败。
    ///
    /// # 参数
    /// * `reason` - 失败原因
    /// * `reports` - 已有报告
    /// * `now` - 完成时间
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 无。
    pub fn mark_failed(&mut self, reason: String, reports: Vec<TierSearchReport>, now: Instant) {
        self.status = PrepareTaskStatus::Failed;
        self.failure_reason = Some(reason);
        self.tier_reports = reports;
        self.finished_at = Some(now);
        self.heartbeat_at = now;
    }

    /// 判断是否已到期。
    ///
    /// # 参数
    /// * `now` - 当前时间
    ///
    /// # 返回
    /// 已到截止时间返回 `true`。
    ///
    /// # 错误
    /// 无。
    pub fn is_deadline_passed(&self, now: Instant) -> bool {
        now >= self.deadline_at
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 在固定时钟创建排队任务，覆盖重启后的截止判断。
    fn task() -> SalesSelectionPrepareTask {
        SalesSelectionPrepareTask::queued(
            SalesSelectionPrepareTaskId::new("task"),
            SalesSelectionPrepareTaskData {
                booklet_id: SalesSelectionBookletId::new("book"),
                booklet_version: 2,
                kind: PrepareKind::FirstPrepare,
                tier_ids: vec![],
                idempotency_key: "key".into(),
                request_hash: "hash".into(),
                seed: 1,
                now: Instant::from_unix_secs(100),
            },
        )
    }

    #[test]
    fn deadline_includes_queue_and_rejects_late_or_replaced_results() {
        let mut task = task();
        assert_eq!(task.deadline_at, Instant::from_unix_secs(280));
        assert!(task.ensure_writable_run(1, Instant::from_unix_secs(101)).is_err());
        task.mark_running(Instant::from_unix_secs(270)).unwrap();
        assert!(task.ensure_writable_run(1, Instant::from_unix_secs(279)).is_ok());
        assert!(task.ensure_writable_run(2, Instant::from_unix_secs(279)).is_err());
        assert!(task.ensure_writable_run(1, Instant::from_unix_secs(280)).is_err());
        task.mark_failed("expired".into(), vec![], Instant::from_unix_secs(280));
        assert!(!task.status.is_active());
        assert!(task.ensure_writable_run(1, Instant::from_unix_secs(281)).is_err());
    }

    #[test]
    fn persisted_task_keeps_command_actor_and_completed_report_state() {
        let mut task = task();
        task.request_json = "{\"kind\":\"FIRST_PREPARE\"}".into();
        task.actor_id = "sales".into();
        task.mark_running(Instant::from_unix_secs(110)).unwrap();
        let mut restored: SalesSelectionPrepareTask =
            serde_json::from_str(&serde_json::to_string(&task).unwrap()).unwrap();
        assert_eq!(restored.request_json, task.request_json);
        assert_eq!(restored.actor_id, "sales");
        restored.mark_succeeded("batch".into(), vec![], Instant::from_unix_secs(120));
        assert_eq!(restored.result_batch_id.as_deref(), Some("batch"));
        assert!(!restored.status.is_active());
        assert!(restored.ensure_writable_run(1, Instant::from_unix_secs(121)).is_err());
    }
}
