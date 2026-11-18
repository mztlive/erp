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
    /// 在批任务开始处理一个切片时记录起点，配置可完成的行数和软时限。
    /// 预算只在当前行完成后检查，即使时限已经用完，也不会取消第一行或中断进行中的工作。
    pub fn new(max_rows: usize, duration: Duration) -> Self {
        Self { started: Instant::now(), rows: 0, max_rows, duration }
    }

    /// 当前行及其进度原子保存后，将该行计入切片预算。
    /// 达到行数或耗时上限时返回 `true`，调用方应结束切片，以便后续轮次从已保存进度继续。
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
