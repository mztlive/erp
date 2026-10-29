//! 保存与核对任务的持久化进度；中断后沿原游标继续。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{Error, Result};

/// 当前后台处理阶段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchWorkPhase {
    Saving,
    Reviewing,
    Finalizing,
}

/// 后台工作状态，失败仍保留写入资格，必须重试原操作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BatchWorkStatus {
    Running,
    Failed,
    Completed,
}

/// 批次头只携带进度，不包含可能很大的请求载荷。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchWork {
    pub id: String,
    pub claimed: bool,
    pub actor_id: String,
    pub phase: BatchWorkPhase,
    pub status: BatchWorkStatus,
    pub completed: usize,
    pub total: usize,
    pub after: String,
    pub generation: u64,
    pub previous_preview: Option<u64>,
    pub review_after_save: bool,
    pub error: Option<String>,
}

/// 原请求独立保存；相同版本与载荷重试只恢复原任务。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct BatchWorkRequest {
    #[serde(flatten)]
    pub base: BaseModel,
    pub batch_id: String,
    pub fingerprint: String,
    pub payload: Value,
}

impl BatchWork {
    /// 判断是否阻止其他草稿修改和入库确认。
    /// # 参数
    /// 无。
    /// # 返回
    /// 正在处理及失败待恢复时为真。
    /// # 错误
    /// 不返回错误。
    pub fn blocking(&self) -> bool {
        self.status != BatchWorkStatus::Completed
    }

    /// 记录一块已经与草稿原子提交的扫描位置。
    /// # 参数
    /// `count` 为处理数量，`after` 为本块末行身份。
    /// # 返回
    /// 推进进度；行处理包括明确跳过的不可编辑项。
    /// # 错误
    /// 非运行态、重复游标或进度超过总数时拒绝。
    pub fn advance(&mut self, count: usize, after: String) -> Result<()> {
        let completed = self.completed.checked_add(count).filter(|value| *value <= self.total);
        if self.status != BatchWorkStatus::Running || count == 0 || after <= self.after || completed.is_none()
        {
            return Err(Error::ConflictError("批次处理进度已变化，请重新读取".into()));
        }
        self.completed = completed.expect("已检查进度范围");
        self.after = after;
        Ok(())
    }

    /// 将已保存草稿交给核对阶段，保留同一任务身份与预览代次。
    /// # 参数
    /// 无。
    /// # 返回
    /// 清零核对阶段游标，已保存内容不重新应用。
    /// # 错误
    /// 保存未完成或阶段不符时拒绝。
    pub fn start_review(&mut self) -> Result<()> {
        if self.status != BatchWorkStatus::Running
            || self.phase != BatchWorkPhase::Saving
            || self.completed != self.total
        {
            return Err(Error::ConflictError("草稿尚未保存完成，不能开始核对".into()));
        }
        self.phase = BatchWorkPhase::Reviewing;
        self.completed = 0;
        self.after.clear();
        Ok(())
    }

    /// 重试原失败任务，不重放已完成行。
    /// # 参数
    /// 无。
    /// # 返回
    /// 恢复运行，保留阶段、游标与原预览代次。
    /// # 错误
    /// 已完成的任务不能重试。
    pub fn retry(&mut self) -> Result<()> {
        if self.status == BatchWorkStatus::Completed {
            return Err(Error::ConflictError("本次处理已完成".into()));
        }
        self.status = BatchWorkStatus::Running;
        self.error = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn work() -> BatchWork {
        BatchWork {
            id: "work".into(),
            claimed: false,
            actor_id: "actor".into(),
            phase: BatchWorkPhase::Saving,
            status: BatchWorkStatus::Running,
            completed: 0,
            total: 201,
            after: String::new(),
            generation: 8,
            previous_preview: Some(7),
            review_after_save: true,
            error: None,
        }
    }
    #[test]
    fn retry_preserves_checkpoint_and_review_starts_only_after_all_saved() {
        let mut work = work();
        work.advance(200, "200".into()).unwrap();
        assert!(work.start_review().is_err());
        work.status = BatchWorkStatus::Failed;
        work.retry().unwrap();
        assert_eq!((work.completed, work.after.as_str(), work.generation), (200, "200", 8));
        assert!(work.advance(1, "200".into()).is_err());
        assert!(work.advance(2, "201".into()).is_err());
        work.advance(1, "201".into()).unwrap();
        work.start_review().unwrap();
        assert_eq!(work.phase, BatchWorkPhase::Reviewing);
        assert_eq!(work.completed, 0);
        assert!(work.after.is_empty());
        work.status = BatchWorkStatus::Completed;
        assert!(!work.blocking());
        assert!(work.retry().is_err());
    }
}
