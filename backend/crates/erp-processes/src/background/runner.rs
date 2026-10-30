//! 统一后台任务轮询循环：顺序执行已注册适配器，轮次之间不重叠。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

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
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 错误
    /// 不返回错误。
    pub fn register<A>(mut self, adapter: A) -> Self
    where
        A: BackgroundTaskAdapter + 'static,
    {
        self.adapters.push(Arc::new(adapter));
        self
    }

    /// 注册需要与外部调度共享状态的适配器。
    /// # 参数
    /// adapter 为共享适配器。
    /// # 返回
    /// 已登记适配器的执行器。
    /// # 错误
    /// 无。
    pub fn register_shared(mut self, adapter: Arc<dyn BackgroundTaskAdapter>) -> Self {
        self.adapters.push(adapter);
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
        self.run_round(None).await
    }

    /// 顺序执行适配器，在停止通知后不进入下一个适配器。
    async fn run_round(&self, stop: Option<&watch::Receiver<bool>>) -> Vec<(&'static str, usize)> {
        let mut counts = Vec::with_capacity(self.adapters.len());
        for adapter in &self.adapters {
            if stop.is_some_and(|stop| *stop.borrow() || stop.has_changed().is_err()) {
                break;
            }
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

    /// 运行可停止的轮询并排空适配器派生任务。
    /// # 参数
    /// `stop` 为进程停止通知。
    /// # 返回
    /// 所有轮次及其派生工作均已结束。
    /// # 错误
    /// 单轮失败记录日志；总退出预算由进程监督器负责。
    pub async fn run(&self, stop: watch::Receiver<bool>) {
        super::periodic(stop.clone(), self.interval, || async {
            self.run_round(Some(&stop)).await;
        })
        .await;
        for adapter in &self.adapters {
            adapter.drain().await;
        }
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
    struct StoppingAdapter {
        stop: tokio::sync::watch::Sender<bool>,
        draining: Arc<tokio::sync::Notify>,
        finish: Arc<tokio::sync::Notify>,
    }

    #[async_trait::async_trait]
    impl BackgroundTaskAdapter for StoppingAdapter {
        fn name(&self) -> &'static str {
            "stopping"
        }
        async fn run_due(&self) -> crate::Result<usize> {
            self.stop.send_replace(true);
            Ok(1)
        }
        async fn drain(&self) {
            self.draining.notify_one();
            self.finish.notified().await;
        }
    }

    #[tokio::test]
    async fn stop_skips_next_adapter_and_waits_for_spawned_work_to_drain() {
        let (stop, receive) = tokio::sync::watch::channel(false);
        let draining = Arc::new(tokio::sync::Notify::new());
        let finish = Arc::new(tokio::sync::Notify::new());
        let calls = Arc::new(AtomicUsize::new(0));
        let runner = BackgroundRunner::new(Duration::ZERO)
            .register(StoppingAdapter { stop, draining: draining.clone(), finish: finish.clone() })
            .register(CountingAdapter { name: "must_skip", calls: calls.clone() });
        let task = tokio::spawn(async move {
            runner.run(receive).await;
        });
        draining.notified().await;
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(!task.is_finished());
        finish.notify_one();
        tokio::time::timeout(Duration::from_secs(1), task).await.unwrap().unwrap();
    }
}
