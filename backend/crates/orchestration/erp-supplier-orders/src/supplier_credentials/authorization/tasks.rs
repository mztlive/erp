use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

use super::{AuthorizationError, Result};

/// 应用共享的授权任务边界；HTTP请求结束不会取消原授权保存。
pub struct JdVopAuthorizationTasks {
    slots: Arc<Semaphore>,
    closing: AtomicBool,
}

impl Default for JdVopAuthorizationTasks {
    fn default() -> Self {
        Self { slots: Arc::new(Semaphore::new(1)), closing: AtomicBool::new(false) }
    }
}

impl JdVopAuthorizationTasks {
    pub(super) fn permit(&self) -> Result<OwnedSemaphorePermit> {
        if self.closing.load(Ordering::SeqCst) {
            return Err(AuthorizationError::Unavailable);
        }
        let permit = self.slots.clone().try_acquire_owned().map_err(|_| AuthorizationError::Busy)?;
        if self.closing.load(Ordering::SeqCst) {
            return Err(AuthorizationError::Unavailable);
        }
        Ok(permit)
    }

    /// 停止受理新授权，并等待已经开始的有界授权任务完成保存或保留未知状态。
    ///
    /// 无；由应用停机生命周期调用。
    ///
    /// 已开始任务均已退出，不新发授权请求。
    ///
    /// 任务准入边界不可用时返回固定脱敏错误。
    pub async fn shutdown(&self) -> Result<()> {
        self.closing.store(true, Ordering::SeqCst);
        self.idle().await
    }

    pub(super) async fn idle(&self) -> Result<()> {
        let permit = self.slots.acquire().await.map_err(|_| AuthorizationError::Unavailable)?;
        drop(permit);
        Ok(())
    }
}
