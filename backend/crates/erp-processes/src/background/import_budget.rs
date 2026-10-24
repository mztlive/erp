//! 导入切片预算：只在当前行结果准备持久化时判断，不取消已经开始的业务写入。

use std::time::Duration;

use tokio::time::Instant;

/// 每个适配器每轮最多扫描的任务数。
pub(crate) const IMPORT_JOBS_PER_POLL: i64 = 4;

/// 每个任务至多十行或一秒；首行始终获得执行机会，耗时检查发生在行边界。
pub(crate) struct ImportBudget {
    started: Instant,
    rows: usize,
    max_rows: usize,
    duration: Duration,
}

impl ImportBudget {
    /// 创建一个任务的执行切片。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回十行、一秒软时限的预算；文件读取与当前行不会被强制中断。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn new() -> Self {
        Self { started: Instant::now(), rows: 0, max_rows: 10, duration: Duration::from_secs(1) }
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
    pub(crate) fn after_row(&mut self) -> bool {
        self.rows += 1;
        self.rows >= self.max_rows || self.started.elapsed() >= self.duration
    }

    #[cfg(test)]
    pub(crate) fn rows(max_rows: usize) -> Self {
        Self { max_rows, duration: Duration::from_secs(3600), ..Self::new() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn row_limit_yields_at_the_completed_row_boundary() {
        let mut budget = ImportBudget::rows(2);
        assert!(!budget.after_row());
        assert!(budget.after_row());
    }

    #[test]
    fn elapsed_limit_still_finishes_the_current_row() {
        let mut budget = ImportBudget::new();
        budget.started = Instant::now() - Duration::from_secs(2);
        assert!(budget.after_row());
        assert_eq!(budget.rows, 1);
    }
}
