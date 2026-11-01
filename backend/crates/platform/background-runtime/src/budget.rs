//! 通用执行切片预算：在已完成步骤的边界让出机会，不取消进行中的工作。

use std::time::Duration;

use tokio::time::Instant;

/// 按完成步骤数和耗时控制软预算；首个步骤始终获得执行机会。
pub struct ExecutionBudget {
    started: Instant,
    rows: usize,
    max_rows: usize,
    duration: Duration,
}

impl ExecutionBudget {
    /// 创建一个任务的执行切片。
    ///
    /// # 参数
    /// * `max_rows` - 每个切片最多完成的步骤数。
    /// * `duration` - 完成首个步骤后生效的软时限。
    ///
    /// # 返回
    /// 返回只在步骤完成边界判断的预算；当前步骤不会被强制中断。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(max_rows: usize, duration: Duration) -> Self {
        Self { started: Instant::now(), rows: 0, max_rows, duration }
    }

    /// 计入当前行并判断保存成功后是否应让出执行机会。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 达到行数或软时限时返回 true；调用方必须先原子保存当前行和进度。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn after_row(&mut self) -> bool {
        self.rows += 1;
        self.rows >= self.max_rows || self.started.elapsed() >= self.duration
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_limit_yields_at_the_completed_row_boundary() {
        let mut budget = ExecutionBudget::new(2, Duration::from_secs(3600));
        assert!(!budget.after_row());
        assert!(budget.after_row());
    }

    #[test]
    fn elapsed_limit_still_finishes_the_current_row() {
        let mut budget = ExecutionBudget::new(10, Duration::from_secs(1));
        budget.started = Instant::now() - Duration::from_secs(2);
        assert!(budget.after_row());
        assert_eq!(budget.rows, 1);
    }
}
