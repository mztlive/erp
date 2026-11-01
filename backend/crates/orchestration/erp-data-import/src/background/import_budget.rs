//! 导入业务每轮任务数和每个任务的执行切片配置。

use std::time::Duration;

use background_runtime::ExecutionBudget;

/// 每个适配器每轮最多扫描的任务数。
pub(crate) const IMPORT_JOBS_PER_POLL: i64 = 4;

/// 创建十行、一秒软时限的导入切片；当前行始终执行到持久化边界。
pub(crate) fn import_budget() -> ExecutionBudget {
    ExecutionBudget::new(10, Duration::from_secs(1))
}
