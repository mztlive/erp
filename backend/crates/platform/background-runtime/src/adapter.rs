//! 后台任务适配器契约：每种领域任务实现同一轮询入口。

/// 轮询错误；保留业务提供方的实际错误和来源链。
pub type TaskError = Box<dyn std::error::Error + Send + Sync>;

/// 不依赖业务错误枚举的轮询执行结果。
pub type TaskResult<T> = std::result::Result<T, TaskError>;

/// 后台任务适配器。
///
/// 每种任务（导入、导出、同步等）各自实现领取与执行，执行器只负责按顺序调用，不感知领域细节。
#[async_trait::async_trait]
pub trait BackgroundTaskAdapter: Send + Sync {
    /// 提供稳定的任务名，供执行器将每轮认领数、耗时和轮询错误归到对应任务。
    fn name(&self) -> &'static str;

    /// 每次轮询时认领并执行一批到期工作，返回本轮认领数，而不是成功完成数。
    /// 认领查询等轮次级故障交给执行器记录；单项业务失败由适配器记录详情并按业务规则处置，
    /// 不应因此中断整轮。
    async fn run_due(&self) -> TaskResult<usize>;

    /// 停止轮询后等待此适配器派生的异步工作收尾，避免执行器提前退出。
    /// 未派生后台工作的适配器可用默认空实现；业务结果由原任务保存，不在排空时重放。
    async fn drain(&self) {}
}
