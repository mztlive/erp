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
    /// 请求停止领取并在页面边界交接已运行任务。所有克隆立即可见停止状态。
    pub fn request_stop(&self) {
        self.stopping.store(true, Ordering::Release);
    }

    /// 有界等待已领取任务完成当前页和结算。
    ///
    /// `timeout` 为允许的排空时间。
    ///
    /// 排空成功为 true；超时为 false，由心跳恢复兜底。
    pub async fn drain(&self, timeout: Duration) -> bool {
        self.request_stop();
        matches!(tokio::time::timeout(timeout, self.permits.acquire_many(2)).await, Ok(Ok(_)))
    }

    // 目录任务在领取和页面边界读取共享退出标记，避免在进程退出时继续开始新工作。
    pub(crate) fn stopping(&self) -> bool {
        self.stopping.load(Ordering::Acquire)
    }

    // 目录 worker 开始任务前尝试领取进程内并发许可；许可用尽或已请求停止时不领取。
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
