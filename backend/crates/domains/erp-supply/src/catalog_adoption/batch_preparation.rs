//! 选择准备的可恢复状态；成员清单与逐项草稿分开持久化。

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 后台准备状态，只有完成后允许进入配置。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreparationStatus {
    Queued,
    Preparing,
    Failed,
    Cancelled,
    Ready,
}

/// 每块成员和进度在同一事务提交，不依赖浏览器或进程内任务句柄。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchPreparation {
    pub request_key: String,
    pub status: PreparationStatus,
    pub completed: usize,
    pub next_page: u32,
    pub error: Option<String>,
    pub fingerprint: String,
}

/// 对准备任务的明确用户操作。
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum PreparationAction {
    Retry,
    Cancel,
}

impl BatchPreparation {
    /// 创建尚未生成草稿的准备任务。
    /// # 参数
    /// `request_key` 为作用域内原操作身份，`fingerprint` 为原选择请求摘要。
    /// # 返回
    /// 进度为零的排队状态。
    /// # 错误
    /// 不返回错误。
    pub fn new(request_key: String, fingerprint: String) -> Self {
        Self {
            request_key,
            status: PreparationStatus::Queued,
            completed: 0,
            next_page: 0,
            error: None,
            fingerprint,
        }
    }

    /// 判断后台是否仍需准备。
    /// # 参数
    /// 无。
    /// # 返回
    /// 排队及准备中为真。
    /// # 错误
    /// 不返回错误。
    pub fn runnable(&self) -> bool {
        matches!(self.status, PreparationStatus::Queued | PreparationStatus::Preparing)
    }

    /// 提交一块真实完成的进度。
    /// # 参数
    /// `count` 为本块行数，`total` 为不可变成员总数。
    /// # 返回
    /// 更新累计数量、下一块与完成状态。
    /// # 错误
    /// 非运行态、空块、溢出或超过冻结总数时拒绝。
    pub fn advance(&mut self, count: usize, total: usize) -> Result<()> {
        let completed = self.completed.checked_add(count).filter(|value| *value <= total);
        if !self.runnable() || count == 0 || completed.is_none() {
            return Err(Error::ConflictError("准备进度与本次选择不一致".into()));
        }
        let next_page =
            self.next_page.checked_add(1).ok_or_else(|| Error::ValidationError("准备分块数量超限".into()))?;

        self.completed = completed.expect("已校验累计数量有效");
        self.next_page = next_page;
        self.status =
            if self.completed == total { PreparationStatus::Ready } else { PreparationStatus::Preparing };
        self.error = None;
        Ok(())
    }

    /// 保存确定的准备失败，保留原进度供重试。
    /// # 参数
    /// `message` 为可向操作人展示的失败原因。
    /// # 返回
    /// 进入失败状态，已完成数量保持不变。
    /// # 错误
    /// 已终止或不在准备中的任务拒绝覆盖。
    pub fn fail(&mut self, message: String) -> Result<()> {
        if !self.runnable() {
            return Err(Error::ConflictError("准备任务已停止，不能覆盖当前状态".into()));
        }
        self.status = PreparationStatus::Failed;
        self.error = Some(message);
        Ok(())
    }

    /// 对原任务执行重试或取消，不改变冻结范围及已提交进度。
    /// # 参数
    /// `action` 为用户明确动作。
    /// # 返回
    /// 合法状态迁移完成；相同取消或已排队重试幂等。
    /// # 错误
    /// 完成后取消、取消后重试或不支持的状态拒绝。
    pub fn apply(&mut self, action: PreparationAction) -> Result<()> {
        match action {
            PreparationAction::Retry if self.status == PreparationStatus::Failed => {
                self.status = PreparationStatus::Queued;
                self.error = None;
            },
            PreparationAction::Retry if self.runnable() => {},
            PreparationAction::Cancel if self.status != PreparationStatus::Ready => {
                self.status = PreparationStatus::Cancelled;
                self.error = None;
            },
            _ => return Err(Error::ConflictError("当前准备状态不允许此操作".into())),
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_survives_retry_and_finishes_only_at_exact_total() {
        let mut task = BatchPreparation::new("key".into(), "request".into());
        task.advance(200, 201).unwrap();
        assert_eq!(task.status, PreparationStatus::Preparing);

        task.fail("interrupted".into()).unwrap();
        task.apply(PreparationAction::Retry).unwrap();
        assert_eq!((task.completed, task.next_page), (200, 1));
        assert!(task.error.is_none());

        task.advance(1, 201).unwrap();
        assert_eq!(task.status, PreparationStatus::Ready);
        assert!(task.apply(PreparationAction::Cancel).is_err());
        assert!(task.advance(1, 201).is_err());
    }

    #[test]
    fn invalid_progress_and_cancel_cannot_publish_or_restart_partial_selection() {
        let mut task = BatchPreparation::new("key".into(), "request".into());
        assert!(task.advance(0, 20).is_err());
        assert!(task.advance(21, 20).is_err());
        assert_eq!(task.completed, 0);

        task.advance(10, 20).unwrap();
        task.apply(PreparationAction::Cancel).unwrap();
        task.apply(PreparationAction::Cancel).unwrap();
        assert!(!task.runnable());
        assert!(task.advance(10, 20).is_err());
        assert!(task.apply(PreparationAction::Retry).is_err());
        assert!(task.fail("late failure".into()).is_err());
        assert_eq!(task.completed, 10);
    }
}
