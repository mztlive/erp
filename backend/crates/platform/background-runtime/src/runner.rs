//! 统一后台任务轮询循环：顺序执行已注册适配器，轮次之间不重叠。

use std::sync::Arc;
use std::time::Duration;

use tokio::sync::watch;

use super::adapter::BackgroundTaskAdapter;
use super::periodic::should_stop;

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
    /// 在装配后台进程时创建空执行器，`interval` 是每轮结束后到下一轮开始前的等待时长。
    /// 启动前通过 [`Self::register`] 或 [`Self::register_shared`] 登记任务。
    pub fn new(interval: Duration) -> Self {
        Self { adapters: Vec::new(), interval }
    }

    /// 装配 worker 时将具体任务交给执行器，并追加到队尾；登记顺序就是每轮的执行顺序。
    pub fn register<A>(self, adapter: A) -> Self
    where
        A: BackgroundTaskAdapter + 'static,
    {
        self.register_shared(Arc::new(adapter))
    }

    /// 任务适配器已由应用持有或还要供其他调度入口复用时，登记现有的 `Arc`，使双方共享同一实例。
    pub fn register_shared(mut self, adapter: Arc<dyn BackgroundTaskAdapter>) -> Self {
        self.adapters.push(adapter);
        self
    }

    /// 手动触发单轮轮询，按登记顺序返回各任务名和认领数，不等待下一个周期。
    /// 某个适配器轮询失败时记录原错误、将该项计为 0，并继续轮询其余任务。
    pub async fn run_once(&self) -> Vec<(&'static str, usize)> {
        let mut counts = Vec::with_capacity(self.adapters.len());
        self.run_round(None, |name, count| counts.push((name, count))).await;
        counts
    }

    // 单次与常驻轮询共用这段顺序执行逻辑；只有常驻调用传入停止信号，
    // 在当前适配器返回后、开始下一个适配器前检查，避免退出时再认领新工作。
    async fn run_round(
        &self,
        stop: Option<&watch::Receiver<bool>>,
        mut report: impl FnMut(&'static str, usize) + Send,
    ) {
        for adapter in &self.adapters {
            if stop.is_some_and(should_stop) {
                break;
            }

            let started = std::time::Instant::now();
            let name = adapter.name();
            let count = match adapter.run_due().await {
                Ok(count) => {
                    tracing::info!(
                        task_name = name,
                        duration_ms = started.elapsed().as_millis() as u64,
                        success_count = count,
                        "background task finished"
                    );
                    count
                },
                Err(error) => {
                    tracing::error!(
                        task_name = name,
                        duration_ms = started.elapsed().as_millis() as u64,
                        error = %error,
                        "background task failed"
                    );
                    0
                },
            };
            report(name, count);
        }
    }

    /// 后台进程启动后持续轮询已登记任务，收到停止信号后不再调用下一个适配器，
    /// 并等待各适配器已派生的工作结束。轮询错误单独记日志，不退出循环；
    /// 整个排空过程的退出预算由进程监督器控制。
    pub async fn run(&self, stop: watch::Receiver<bool>) {
        super::periodic(stop.clone(), self.interval, || async {
            self.run_round(Some(&stop), |_, _| {}).await;
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
    use crate::{BackgroundTaskAdapter, TaskResult};

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
        async fn run_due(&self) -> TaskResult<usize> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(1)
        }
    }

    // 用两个各认领一项的适配器确认单轮结果按登记顺序汇报，且每个适配器只调用一次。
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
    struct FailingAdapter;

    #[async_trait::async_trait]
    impl BackgroundTaskAdapter for FailingAdapter {
        fn name(&self) -> &'static str {
            "failing"
        }

        async fn run_due(&self) -> TaskResult<usize> {
            Err(std::io::Error::other("expected polling failure").into())
        }
    }

    #[tokio::test]
    async fn failed_poll_keeps_original_error_and_allows_the_next_adapter() {
        let error = FailingAdapter.run_due().await.unwrap_err();
        assert!(error.downcast_ref::<std::io::Error>().is_some());
        assert_eq!(error.to_string(), "expected polling failure");

        let calls = Arc::new(AtomicUsize::new(0));
        let runner = BackgroundRunner::new(Duration::ZERO)
            .register(FailingAdapter)
            .register(CountingAdapter { name: "next", calls: calls.clone() });
        assert_eq!(runner.run_once().await, [("failing", 0), ("next", 1)]);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
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
        async fn run_due(&self) -> TaskResult<usize> {
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
