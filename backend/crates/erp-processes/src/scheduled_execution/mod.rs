//! XXL 调度持久化执行编排；协议接收、业务完成与回报相互独立。
mod admin;
use std::sync::Arc;

pub use admin::XxlAdmin;
use async_trait::async_trait;
use erp_support::entity::scheduled_run::{ScheduledHandler, ScheduledRequest, ScheduledRun};
use erp_support::ports::scheduled_runs::ScheduledRunStore;

/// 调度编排错误；协议层只返回固定脱敏分类。
#[derive(Debug, thiserror::Error)]
pub enum DispatchError {
    #[error("调度回执存储失败")]
    Store(#[from] persistence_core::Error),
    #[error("调度执行身份不一致")]
    Identity,
    #[error("该任务已有待执行或执行中的调度")]
    Busy,
    #[error("调度编号或时间无效，或已过接收窗口")]
    Stale,
}

/// 固定业务能力，不接受任意脚本或业务载荷。
#[async_trait]
pub trait ScheduledWork: Send + Sync {
    /// 执行一个有界业务轮次。
    /// # 参数
    /// handler 为已核对的固定能力。
    /// # 返回
    /// 本轮处理数量。
    /// # 错误
    /// 业务失败只返回固定脱敏摘要。
    async fn run(&self, handler: ScheduledHandler) -> Result<usize, &'static str>;
}

/// 调度中心回报端口，false 表示稍后仅重发回报。
#[async_trait]
pub trait ScheduleCallback: Send + Sync {
    /// 发送已有结果，不能触发任何业务重放。
    /// # 参数
    /// run 为已持久化完成的回执。
    /// # 返回
    /// 远端确认时为 true。
    /// # 错误
    /// 网络或协议错误返回 false，保留待回报。
    async fn send(&self, run: &ScheduledRun) -> bool;
}

/// 持久化调度编排。
pub struct ScheduledExecutionProcess {
    pub app: String,
    owner: String,
    store: Arc<dyn ScheduledRunStore>,
    work: Arc<dyn ScheduledWork>,
    callback: Arc<dyn ScheduleCallback>,
}
impl ScheduledExecutionProcess {
    /// 装配持久化和业务端口。
    /// # 参数
    /// app 为固定执行器名，owner 为每次启动新生成的身份，其他参数为窄端口。
    /// # 返回
    /// 不发起 I/O 的编排器。
    /// # 错误
    /// 无。
    pub fn new(
        app: String,
        owner: String,
        store: Arc<dyn ScheduledRunStore>,
        work: Arc<dyn ScheduledWork>,
        callback: Arc<dyn ScheduleCallback>,
    ) -> Self {
        Self { app, owner, store, work, callback }
    }

    /// 在监听和领取之前恢复旧进程中断回执，不重跑其业务。
    /// # 参数
    /// 无。
    /// # 返回
    /// 旧 running 均已转为可回报失败。
    /// # 错误
    /// 数据库不可用时拒绝启动。
    pub async fn recover(&self) -> Result<(), DispatchError> {
        Ok(self.store.recover(&self.app, &self.owner).await?)
    }

    /// 先持久化后确认接收，同一身份可重复请求。
    /// # 参数
    /// request 为固定业务身份，now_ms 为可信当前时间。
    /// # 返回
    /// accepted 或已有相同身份回执时成功。
    /// # 错误
    /// 身份冲突、过期、繁忙或存储失败时拒绝。
    pub async fn accept(&self, request: ScheduledRequest, now_ms: i64) -> Result<(), DispatchError> {
        if request.app != self.app {
            return Err(DispatchError::Identity);
        }
        if let Some(old) = self.store.find(&request.key()).await? {
            return same_request(&old, &request);
        }
        if request.job_id <= 0
            || request.log_id <= 0
            || request.log_time <= 0
            || request.log_time < now_ms.saturating_sub(86_400_000)
            || request.log_time > now_ms.saturating_add(300_000)
        {
            return Err(DispatchError::Stale);
        }
        let run = ScheduledRun::accepted(request);
        if let Err(error) = self.store.insert(&run).await {
            if let Some(old) = self.store.find(&run.base.id).await? {
                return same_request(&old, &run.request);
            }
            if self.store.job_busy(&self.app, run.request.job_id).await? {
                return Err(DispatchError::Busy);
            }
            return Err(error.into());
        }
        Ok(())
    }

    /// 原子领取一个请求，业务结束后先保存结果。
    /// # 参数
    /// 无。
    /// # 返回
    /// 是否执行了一个回执。
    /// # 错误
    /// 领取或结果保存失败必须终止当前执行循环，由重启沿原记录核实。
    pub async fn run_next(&self) -> Result<bool, DispatchError> {
        let Some(run) = self.store.claim(&self.app, &self.owner).await? else { return Ok(false) };
        let result = self.work.run(run.request.handler).await;
        let (code, message) = match result {
            Ok(count) => (200, format!("本轮扫描完成，处理 {count} 项；逐项业务结果请查看 ERP")),
            Err(message) => (500, message.to_string()),
        };
        self.store.finish(&run, code, &message).await?;
        Ok(true)
    }

    /// 有界重发待回报结果，不再次调用业务能力。
    /// # 参数
    /// now 为当前 Unix 秒。
    /// # 返回
    /// 当前批次已确认或已记录退避时间。
    /// # 错误
    /// 存储失败时返回错误；远端失败保留回报记录。
    pub async fn callbacks(&self, now: i64) -> Result<(), DispatchError> {
        for run in self.store.callbacks(&self.app, now).await? {
            let success = self.callback.send(&run).await;
            self.store.callback_result(&run, success, now).await?;
            if !success {
                tracing::warn!(log_id = run.request.log_id, "XXL 回报失败，保留原结果稍后重发");
            }
        }
        Ok(())
    }

    /// 查询某个调度中心任务是否有活动执行。
    /// # 参数
    /// job_id 为调度请求中的任务号。
    /// # 返回
    /// 是否有待处理或执行中记录。
    /// # 错误
    /// 存储不可用时返回错误。
    pub async fn busy(&self, job_id: i32) -> Result<bool, DispatchError> {
        Ok(self.store.job_busy(&self.app, job_id).await?)
    }

    /// 查询自身应用的脱敏执行回执。
    /// # 参数
    /// log_id 为调度日志号。
    /// # 返回
    /// 原回执或不存在。
    /// # 错误
    /// 存储不可用时返回错误。
    pub async fn receipt(&self, log_id: i64) -> Result<Option<ScheduledRun>, DispatchError> {
        Ok(self.store.find(&format!("{}:{log_id}", self.app)).await?)
    }
}

/// 原日志号仅允许完全一致的执行身份。
fn same_request(old: &ScheduledRun, request: &ScheduledRequest) -> Result<(), DispatchError> {
    if &old.request == request { Ok(()) } else { Err(DispatchError::Identity) }
}

#[cfg(test)]
mod tests;
