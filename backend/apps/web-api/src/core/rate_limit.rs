//! 进程内请求准入计数。

use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use tokio::sync::{OwnedSemaphorePermit, Semaphore};

/// 同时支持按 key、全局滑动窗口和全局并发上限的进程内限流器。
///
/// 该类型只负责准入计数，不解析 HTTP、账号或业务语义。多实例部署时每个
/// 进程独立计数；需要集群级配额时应在网关或共享存储层补充。
#[derive(Clone)]
pub(crate) struct RateLimiter {
    state: Arc<Mutex<WindowState>>,
    slots: Arc<Semaphore>,
    policy: Policy,
}

#[derive(Clone)]
struct Policy {
    key_limits: Arc<[usize]>,
    max_global: usize,
    window: Duration,
}

#[derive(Default)]
struct WindowState {
    global: VecDeque<Instant>,
    keyed: HashMap<String, VecDeque<Instant>>,
}

impl RateLimiter {
    /// 创建滑动窗口限流器。
    ///
    /// # 参数
    /// * `max_per_key` - 单个 key 在窗口内允许的请求数
    /// * `max_global` - 全部 key 在窗口内允许的请求总数
    /// * `window` - 统计窗口
    /// * `max_concurrent` - 同时处理的最大请求数
    ///
    /// # 返回值
    /// 返回可在线程间共享的进程内限流器。
    ///
    /// # Panics
    /// 任一限制为零，或全局窗口小于单 key 窗口时 panic；调用点使用编译期常量。
    pub(crate) fn new(
        max_per_key: usize,
        max_global: usize,
        window: Duration,
        max_concurrent: usize,
    ) -> Self {
        Self::with_key_limits(&[max_per_key], max_global, window, max_concurrent)
    }

    /// 创建支持多层 key 配额的滑动窗口限流器。
    ///
    /// # 参数
    /// * `key_limits` - 各层 key 按顺序对应的窗口请求上限
    /// * `max_global` - 全部 key 在窗口内允许的请求总数
    /// * `window` - 统计窗口
    /// * `max_concurrent` - 同时处理的最大请求数
    ///
    /// # 返回值
    /// 返回可原子预留多层 key 配额的进程内限流器。
    ///
    /// # Panics
    /// key 层级为空、任一限制为零、全局窗口小于任一 key 窗口，或并发数为零时 panic。
    pub(crate) fn with_key_limits(
        key_limits: &[usize],
        max_global: usize,
        window: Duration,
        max_concurrent: usize,
    ) -> Self {
        assert!(!key_limits.is_empty(), "at least one key limit is required");
        assert!(
            key_limits.iter().all(|limit| *limit > 0),
            "per-key rate limits must be positive"
        );
        assert!(
            key_limits.iter().all(|limit| max_global >= *limit),
            "global rate limit must cover every key"
        );
        assert!(!window.is_zero(), "rate limit window must be positive");
        assert!(max_concurrent > 0, "concurrency limit must be positive");

        Self {
            state: Arc::new(Mutex::new(WindowState::default())),
            slots: Arc::new(Semaphore::new(max_concurrent)),
            policy: Policy {
                key_limits: Arc::from(key_limits),
                max_global,
                window,
            },
        }
    }

    /// 为给定 key 原子预留窗口配额与全局并发槽位。
    ///
    /// # 参数
    /// * `key` - 已由调用方规范化的稳定限流 key
    ///
    /// # 返回值
    /// 返回必须持有到请求处理结束的并发许可。
    ///
    /// # 错误
    /// 单 key 或全局窗口超限、并发槽位已满或计数状态不可用时返回错误。
    pub(crate) fn admit(&self, key: &str) -> Result<OwnedSemaphorePermit, Error> {
        self.admit_at(key, Instant::now())
    }

    /// 为多层 key 原子预留窗口配额与一个全局并发槽位。
    ///
    /// # 参数
    /// * `keys` - 与构造时 `key_limits` 顺序和数量一致、且互不相同的层级 key
    ///
    /// # 返回值
    /// 返回必须持有到请求处理结束的并发许可。
    ///
    /// # 错误
    /// 任一层 key 或全局窗口超限、并发槽位已满、key 结构不匹配或计数状态不可用时
    /// 返回错误；失败不会部分占用其他层级配额。
    pub(crate) fn admit_hierarchy(&self, keys: &[&str]) -> Result<OwnedSemaphorePermit, Error> {
        self.admit_hierarchy_at(keys, Instant::now())
    }

    /// 在显式时间点执行准入，便于确定性验证窗口边界。
    fn admit_at(&self, key: &str, now: Instant) -> Result<OwnedSemaphorePermit, Error> {
        self.admit_hierarchy_at(&[key], now)
    }

    /// 在显式时间点原子执行多层 key 准入。
    fn admit_hierarchy_at(&self, keys: &[&str], now: Instant) -> Result<OwnedSemaphorePermit, Error> {
        if !self.keys_match_policy(keys) {
            return Err(Error::Unavailable);
        }
        let permit = Arc::clone(&self.slots)
            .try_acquire_owned()
            .map_err(|_| Error::ConcurrencyExceeded)?;
        self.reserve_window(keys, now)?;
        Ok(permit)
    }

    /// 检查 key 数量与唯一性是否符合构造时策略。
    fn keys_match_policy(&self, keys: &[&str]) -> bool {
        keys.len() == self.policy.key_limits.len()
            && !keys
                .iter()
                .enumerate()
                .any(|(index, key)| keys[..index].contains(key))
    }

    /// 清理过期计数并同时预留全局与所有层级 key 配额。
    fn reserve_window(&self, keys: &[&str], now: Instant) -> Result<(), Error> {
        let mut state = self.state.lock().map_err(|_| Error::Unavailable)?;
        remove_expired(&mut state.global, now, self.policy.window);
        if state.global.len() >= self.policy.max_global {
            return Err(Error::GlobalExceeded {
                retry_after_secs: retry_after(&state.global, now, self.policy.window),
            });
        }

        state.keyed.retain(|_, requests| {
            remove_expired(requests, now, self.policy.window);
            !requests.is_empty()
        });
        for (key, limit) in keys.iter().zip(self.policy.key_limits.iter()) {
            let Some(requests) = state.keyed.get(*key) else {
                continue;
            };
            if requests.len() >= *limit {
                return Err(Error::KeyExceeded {
                    retry_after_secs: retry_after(requests, now, self.policy.window),
                });
            }
        }

        state.global.push_back(now);
        for key in keys {
            state.keyed.entry((*key).to_string()).or_default().push_back(now);
        }
        Ok(())
    }
}

/// 请求未获得限流准入的原因。
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum Error {
    /// 单个 key 已用完当前滑动窗口配额。
    #[error("key rate limit exceeded")]
    KeyExceeded { retry_after_secs: u64 },
    /// 当前进程已用完全局滑动窗口配额。
    #[error("global rate limit exceeded")]
    GlobalExceeded { retry_after_secs: u64 },
    /// 当前进程的并发槽位已满。
    #[error("concurrency limit exceeded")]
    ConcurrencyExceeded,
    /// 内部计数状态不可用。
    #[error("rate limit state unavailable")]
    Unavailable,
}

impl Error {
    /// 返回客户端可以重试前需要等待的秒数。
    ///
    /// # 返回值
    /// 配额或并发超限时返回秒数；内部状态不可用时返回 `None`。
    pub(crate) fn retry_after_secs(&self) -> Option<u64> {
        match self {
            Self::KeyExceeded { retry_after_secs } | Self::GlobalExceeded { retry_after_secs } => {
                Some(*retry_after_secs)
            }
            Self::ConcurrencyExceeded => Some(1),
            Self::Unavailable => None,
        }
    }
}

fn remove_expired(requests: &mut VecDeque<Instant>, now: Instant, window: Duration) {
    while requests
        .front()
        .is_some_and(|started_at| now.saturating_duration_since(*started_at) >= window)
    {
        requests.pop_front();
    }
}

fn retry_after(requests: &VecDeque<Instant>, now: Instant, window: Duration) -> u64 {
    let remaining = requests
        .front()
        .map(|started_at| window.saturating_sub(now.saturating_duration_since(*started_at)))
        .unwrap_or(window);
    duration_ceil_secs(remaining)
}

fn duration_ceil_secs(duration: Duration) -> u64 {
    let rounded = duration.as_secs() + u64::from(duration.subsec_nanos() > 0);
    rounded.max(1)
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::{Error, RateLimiter};

    #[test]
    fn per_key_window_allows_limit_and_rejects_next_request() {
        let limiter = RateLimiter::new(2, 4, Duration::from_secs(60), 3);
        let now = Instant::now();

        let first = limiter.admit_at("account-a", now).unwrap();
        let second = limiter
            .admit_at("account-a", now + Duration::from_secs(1))
            .unwrap();
        drop((first, second));

        assert!(matches!(
            limiter.admit_at("account-a", now + Duration::from_secs(2)),
            Err(Error::KeyExceeded { retry_after_secs: 58 })
        ));
    }

    #[test]
    fn global_window_rejects_rotating_keys() {
        let limiter = RateLimiter::new(2, 2, Duration::from_secs(60), 2);
        let now = Instant::now();
        drop(limiter.admit_at("account-a", now).unwrap());
        drop(
            limiter
                .admit_at("account-b", now + Duration::from_secs(1))
                .unwrap(),
        );

        assert!(matches!(
            limiter.admit_at("account-c", now + Duration::from_secs(2)),
            Err(Error::GlobalExceeded { retry_after_secs: 58 })
        ));
    }

    #[test]
    fn quota_is_shared_across_clones_and_expires_at_window_boundary() {
        let limiter = RateLimiter::new(1, 2, Duration::from_secs(60), 2);
        let cloned = limiter.clone();
        let now = Instant::now();
        drop(limiter.admit_at("account-a", now).unwrap());

        assert!(matches!(
            cloned.admit_at("account-a", now),
            Err(Error::KeyExceeded { .. })
        ));
        assert!(cloned
            .admit_at("account-a", now + Duration::from_secs(60))
            .is_ok());
    }

    #[test]
    fn concurrency_rejection_does_not_consume_window_quota() {
        let limiter = RateLimiter::new(1, 2, Duration::from_secs(60), 1);
        let now = Instant::now();
        let permit = limiter.admit_at("account-a", now).unwrap();

        assert!(matches!(
            limiter.admit_at("account-b", now),
            Err(Error::ConcurrencyExceeded)
        ));
        drop(permit);

        assert!(limiter.admit_at("account-b", now).is_ok());
    }

    #[test]
    fn hierarchy_reserves_all_key_levels_atomically() {
        let limiter = RateLimiter::with_key_limits(&[2, 1], 4, Duration::from_secs(60), 2);
        let now = Instant::now();
        drop(
            limiter
                .admit_hierarchy_at(&["source-a", "source-a|account-a"], now)
                .unwrap(),
        );

        assert!(matches!(
            limiter.admit_hierarchy_at(&["source-a", "source-a|account-a"], now + Duration::from_secs(1)),
            Err(Error::KeyExceeded { .. })
        ));
        assert!(limiter
            .admit_hierarchy_at(&["source-a", "source-a|account-b"], now + Duration::from_secs(2))
            .is_ok());
    }

    #[test]
    fn hierarchy_rejects_mismatched_or_duplicate_keys() {
        let limiter = RateLimiter::with_key_limits(&[2, 1], 4, Duration::from_secs(60), 2);

        assert!(matches!(
            limiter.admit_hierarchy(&["source-a"]),
            Err(Error::Unavailable)
        ));
        assert!(matches!(
            limiter.admit_hierarchy(&["source-a", "source-a"]),
            Err(Error::Unavailable)
        ));
    }
}
