//! 统一后台任务轮询循环：顺序执行已注册适配器，轮次之间不重叠。

use std::sync::Arc;
use std::time::Duration;

use tokio::task::JoinHandle;

use super::adapter::BackgroundTaskAdapter;

/// 统一后台任务执行器。
///
/// 单循环内按注册顺序依次调用适配器；上一轮未结束前不会开始下一轮，避免同一进程内重叠领取。
pub struct BackgroundRunner {
    /// 已注册的任务适配器。
    adapters: Vec<Arc<dyn BackgroundTaskAdapter>>,
    /// 轮询间隔。
    interval: Duration,
}

impl BackgroundRunner {
    /// 创建执行器。
    ///
    /// # 参数
    /// * `interval` - 每轮轮询结束后的等待时长
    ///
    /// # 返回
    /// 返回空适配器的执行器，需继续调用 `register` 装配任务。
    pub fn new(interval: Duration) -> Self {
        Self { adapters: Vec::new(), interval }
    }

    /// 注册任务适配器。
    ///
    /// # 参数
    /// * `adapter` - 领域任务适配器实现
    ///
    /// # 返回
    /// 返回装配后的执行器，支持链式调用。
    pub fn register<A>(mut self, adapter: A) -> Self
    where
        A: BackgroundTaskAdapter + 'static,
    {
        self.adapters.push(Arc::new(adapter));
        self
    }

    /// 执行一轮所有适配器。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回各适配器本轮认领数，顺序与注册顺序一致。
    ///
    /// # 错误
    /// 无；单个适配器的轮询失败只记日志，不中断其余适配器。
    pub async fn run_once(&self) -> Vec<(&'static str, usize)> {
        let mut counts = Vec::with_capacity(self.adapters.len());
        for adapter in &self.adapters {
            let started = std::time::Instant::now();
            let name = adapter.name();
            match adapter.run_due().await {
                Ok(count) => {
                    tracing::info!(
                        task_name = name,
                        duration_ms = started.elapsed().as_millis() as u64,
                        success_count = count,
                        "background task finished"
                    );
                    counts.push((name, count));
                },
                Err(error) => {
                    tracing::error!(
                        task_name = name,
                        duration_ms = started.elapsed().as_millis() as u64,
                        error = %error,
                        "background task failed"
                    );
                    counts.push((name, 0));
                },
            }
        }
        counts
    }

    /// 在后台启动轮询循环。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回后台任务句柄；调用方负责在进程退出时等待或终止。
    pub fn spawn(self: Arc<Self>) -> JoinHandle<()> {
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(self.interval).await;
                self.run_once().await;
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::BackgroundRunner;
    use crate::background::adapter::BackgroundTaskAdapter;

    /// 计数用测试适配器。
    struct CountingAdapter {
        /// 适配器名称。
        name: &'static str,
        /// 调用次数。
        calls: Arc<AtomicUsize>,
    }

    #[async_trait::async_trait]
    impl BackgroundTaskAdapter for CountingAdapter {
        /// 返回测试任务名称。
        ///
        /// # 参数
        /// 无。
        ///
        /// # 返回
        /// 返回固定的测试任务名。
        fn name(&self) -> &'static str {
            self.name
        }

        /// 记录一次调用并返回固定数量。
        ///
        /// # 参数
        /// 无。
        ///
        /// # 返回
        /// 返回本轮认领数 1。
        ///
        /// # 错误
        /// 无。
        async fn run_due(&self) -> crate::Result<usize> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        }
    }

    /// 执行器按顺序调用全部适配器。
    #[tokio::test]
    async fn run_once_calls_adapters_in_order() {
        let calls = Arc::new(AtomicUsize::new(0));
        let runner = BackgroundRunner::new(Duration::from_secs(2))
            .register(CountingAdapter { name: "first", calls: Arc::clone(&calls) })
            .register(CountingAdapter { name: "second", calls: Arc::clone(&calls) });
        let counts = runner.run_once().await;
        assert_eq!(counts, vec![("first", 1), ("second", 1)]);
        assert_eq!(calls.load(Ordering::SeqCst), 2);
    }
}
