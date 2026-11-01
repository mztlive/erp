//! 目录任务进程内停止领取与有界排空；数据库检查点负责跨进程恢复。
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// 每进程最多两条目录任务；所有克隆共享退出状态和运行许可。
#[derive(Clone)]
pub struct CatalogTaskControl {
    stopping: Arc<AtomicBool>,
    permits: Arc<Semaphore>,
}

impl Default for CatalogTaskControl {
    fn default() -> Self {
        Self { stopping: Arc::new(AtomicBool::new(false)), permits: Arc::new(Semaphore::new(2)) }
    }
}

impl CatalogTaskControl {
    /// 请求停止领取并在页面边界交接已运行任务。
    /// # 参数
    /// 无。
    /// # 返回
    /// 所有克隆立即可见停止状态。
    /// # 错误
    /// 不返回错误。
    pub fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    /// 有界等待已领取任务完成当前页和结算。
    /// # 参数
    /// `timeout` 为允许的排空时间。
    /// # 返回
    /// 排空成功为 true；超时为 false，由心跳恢复兜底。
    /// # 错误
    /// 不返回错误。
    pub async fn drain(&self, timeout: Duration) -> bool {
        self.request_stop();
        matches!(tokio::time::timeout(timeout, self.permits.acquire_many(2)).await, Ok(Ok(_)))
    }

    /// 查询本进程是否正在退出。
    pub(crate) fn stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    /// 领取有界执行许可，停止后不再领取。
    pub(crate) fn claim(&self) -> Option<OwnedSemaphorePermit> {
        if self.stopping() {
            return None;
        }
        let permit = self.permits.clone().try_acquire_owned().ok()?;
        if self.stopping() {
            return None;
        }
        Some(permit)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn shutdown_stops_claims_and_waits_for_inflight_page() {
        let control = CatalogTaskControl::default();
        let first = control.claim().unwrap();
        let second = control.claim().unwrap();
        assert!(control.claim().is_none());
        control.request_stop();
        assert!(control.clone().stopping());
        assert!(control.claim().is_none());
        assert!(!control.drain(Duration::ZERO).await);
        drop(first);
        drop(second);
        assert!(control.drain(Duration::from_secs(1)).await);
        assert!(control.claim().is_none());
    }
}
