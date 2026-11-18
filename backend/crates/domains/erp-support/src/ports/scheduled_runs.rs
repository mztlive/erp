//! 调度回执持久化端口；接收确认必须先完成可靠写入。
use async_trait::async_trait;
use persistence_core::Result;

use crate::entity::scheduled_run::ScheduledRun;

/// 唯一身份、Job ID 槽位和执行状态的原子存储边界。
#[async_trait]
pub trait ScheduledRunStore: Send + Sync {
    /// 查询回执。
    /// 原记录或不存在。
    /// key 为固定主键。
    /// 传播存储错误。
    async fn find(&self, key: &str) -> Result<Option<ScheduledRun>>;
    /// 插入 accepted 回执。
    /// 写入已确认。
    /// run 为不可变身份和初始状态。
    /// 唯一身份、活动槽位冲突或存储失败返回错误。
    async fn insert(&self, run: &ScheduledRun) -> Result<()>;
    /// 查询调度中心任务是否已有活动执行。
    /// 该任务有 accepted 或 running 回执时为 true；无需预先登记任务号。
    /// app 为执行器名，job_id 来自已认证调度请求。
    /// 数据库不可用返回错误。
    async fn job_busy(&self, app: &str, job_id: i32) -> Result<bool>;
    /// 原子认领一个 accepted 回执。
    /// 已认领记录或无可执行任务。
    /// app 为执行器名，owner 为当前进程唯一身份。
    /// 数据库失败返回错误。
    async fn claim(&self, app: &str, owner: &str) -> Result<Option<ScheduledRun>>;
    /// 保存完成结果并释放槽位。
    /// 结果已持久化。
    /// run 为当前认领记录，code 和 message 为脱敏结果。
    /// owner 或状态冲突、存储失败返回错误。
    async fn finish(&self, run: &ScheduledRun, code: i32, message: &str) -> Result<()>;
    /// 单副本启动时将旧进程 running 标为中断。
    /// 旧 running 已完成为中断失败，不重跑业务。
    /// app 为执行器名，owner 为新进程身份。
    /// 传播存储错误。
    async fn recover(&self, app: &str, owner: &str) -> Result<()>;
    /// 查询有界到期回报。
    /// 最多二十条已完成且未确认回报的记录。
    /// app 为执行器名，now 为 Unix 秒。
    /// 传播存储错误。
    async fn callbacks(&self, app: &str, now: i64) -> Result<Vec<ScheduledRun>>;
    /// 保存回报确认或下一次重试时刻。
    /// 成功确认后开始三十天保留期，失败退避三十秒。
    /// run 为原回执，success 为远端确认，now 为 Unix 秒。
    /// 传播存储错误。
    async fn callback_result(&self, run: &ScheduledRun, success: bool, now: i64) -> Result<()>;
}
