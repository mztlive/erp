//! 通知 outbox worker：租约、幂等发送、退避与死信。

use entities::approval_integration::{ApprovalNotificationOutbox, MAX_DELIVERY_ATTEMPTS, RETRY_BACKOFF_SECS};
use entities::common::time::Instant;

use crate::errors::{Error, Result};

/// 发送结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeliveryAttempt {
    /// 投递成功。
    Delivered,
    /// 可重试失败。
    Retryable,
    /// 不可重试失败。
    Fatal,
}

/// 租约秒数。
pub const LEASE_SECS: i64 = 30;

/// 计算下一次退避秒数。
///
/// # 参数
/// * `attempt_count` - 已失败次数，从 1 开始
///
/// # 返回
/// 仍可重试时返回退避秒数。
pub fn retry_backoff_secs(attempt_count: u32) -> Option<i64> {
    let index = usize::try_from(attempt_count.saturating_sub(1)).ok()?;
    RETRY_BACKOFF_SECS.get(index).copied()
}

/// 判断是否应进入死信。
///
/// # 参数
/// * `attempt_count` - 累计尝试次数
///
/// # 返回
/// 达到最大次数时返回 `true`。
pub fn should_dead_letter(attempt_count: u32) -> bool {
    attempt_count >= MAX_DELIVERY_ATTEMPTS
}

/// 在已持有租约的消息上应用一次发送结果。
///
/// # 参数
/// * `item` - 租约中的 outbox
/// * `attempt` - 发送结果
/// * `failed_at` - 失败时间
///
/// # 错误
/// 消息不处于投递中时返回错误。
pub fn apply_delivery_attempt(
    item: &mut ApprovalNotificationOutbox,
    attempt: DeliveryAttempt,
    failed_at: Instant,
) -> Result<()> {
    match attempt {
        DeliveryAttempt::Delivered => item.mark_delivered().map_err(Error::from),
        DeliveryAttempt::Retryable | DeliveryAttempt::Fatal => item
            .mark_failure(error_class(attempt), failed_at)
            .map_err(Error::from),
    }
}

/// 失败分类，不得包含敏感载荷。
fn error_class(attempt: DeliveryAttempt) -> &'static str {
    match attempt {
        DeliveryAttempt::Delivered => "delivered",
        DeliveryAttempt::Retryable => "retryable",
        DeliveryAttempt::Fatal => "fatal",
    }
}

/// 计算租约截止时间。
///
/// # 参数
/// * `now` - 当前时间
///
/// # 返回
/// 返回租约截止。
pub fn lease_until(now: Instant) -> Instant {
    Instant::from_unix_secs(now.unix_secs().saturating_add(LEASE_SECS))
}

/// 幂等发送接口。必须按去重键调用。
pub trait NotificationSender {
    /// 以 outbox 去重键发送。
    ///
    /// # 参数
    /// * `dedup_key` - 业务去重键
    ///
    /// # 返回
    /// 返回投递结果。
    fn send_idempotent(&self, dedup_key: &str) -> DeliveryAttempt;
}

/// 有界租约与 CAS 投递状态。
pub trait OutboxLeaseStore {
    /// 原子领取一批可投递消息。
    ///
    /// # 参数
    /// * `worker_id` - 租约持有者
    /// * `now` - 当前时间
    /// * `until` - 租约截止
    /// * `limit` - 批次上限
    ///
    /// # 返回
    /// 返回已写入 worker ID 的消息。
    fn lease_batch(
        &mut self,
        worker_id: &str,
        now: Instant,
        until: Instant,
        limit: u32,
    ) -> Result<Vec<ApprovalNotificationOutbox>>;

    /// 按租约 owner CAS 标记成功。
    ///
    /// # 参数
    /// * `id` - outbox 主键
    /// * `worker_id` - 租约持有者
    ///
    /// # 错误
    /// owner 不匹配时返回冲突。
    fn mark_delivered(&mut self, id: &str, worker_id: &str) -> Result<()>;

    /// 按租约 owner 写回失败后的实体。
    ///
    /// # 参数
    /// * `item` - 已更新的 outbox
    /// * `worker_id` - 租约持有者
    ///
    /// # 错误
    /// owner 不匹配时返回冲突。
    fn save_after_failure(&mut self, item: ApprovalNotificationOutbox, worker_id: &str) -> Result<()>;
}

/// 批次处理结果。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkerBatchOutcome {
    /// 成功投递。
    pub delivered: u32,
    /// 安排重试。
    pub retried: u32,
    /// 进入死信。
    pub dead_lettered: u32,
}

/// 处理一批已租约消息：事务外发送，成功 CAS，失败退避或死信。
///
/// # 参数
/// * `store` - 租约存储
/// * `sender` - 幂等发送器
/// * `worker_id` - 本实例 worker
/// * `now` - 当前时间
/// * `limit` - 批次上限
///
/// # 错误
/// 租约或 CAS 失败时返回错误。
pub fn process_outbox_batch<S, N>(
    store: &mut S,
    sender: &N,
    worker_id: &str,
    now: Instant,
    limit: u32,
) -> Result<WorkerBatchOutcome>
where
    S: OutboxLeaseStore,
    N: NotificationSender,
{
    let until = lease_until(now);
    let leased = store.lease_batch(worker_id, now, until, limit)?;
    let mut outcome = WorkerBatchOutcome::default();
    for mut item in leased {
        let attempt = sender.send_idempotent(&item.dedup_key);
        match attempt {
            DeliveryAttempt::Delivered => {
                store.mark_delivered(&item.base.id, worker_id)?;
                outcome.delivered = outcome.delivered.saturating_add(1);
            }
            DeliveryAttempt::Retryable | DeliveryAttempt::Fatal => {
                apply_delivery_attempt(&mut item, attempt, now)?;
                if item.delivery_status
                    == entities::approval_integration::ApprovalNotificationDeliveryStatus::DeadLetter
                {
                    outcome.dead_lettered = outcome.dead_lettered.saturating_add(1);
                } else {
                    outcome.retried = outcome.retried.saturating_add(1);
                }
                store.save_after_failure(item, worker_id)?;
            }
        }
    }
    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::{
        apply_delivery_attempt, retry_backoff_secs, should_dead_letter, DeliveryAttempt,
        MAX_DELIVERY_ATTEMPTS,
    };
    use entities::approval_integration::{
        ApprovalNotificationDeliveryStatus, ApprovalNotificationEventKind, ApprovalNotificationOutbox,
        ApprovalNotificationTemplateParams,
    };
    use entities::common::time::Instant;
    use entities::ids::ApprovalNotificationOutboxId;

    /// 退避表与死信次数符合合同。
    #[test]
    fn execution_worker_backoff_and_dead_letter() {
        assert_eq!(retry_backoff_secs(1), Some(60));
        assert_eq!(retry_backoff_secs(5), Some(21_600));
        assert_eq!(retry_backoff_secs(6), None);
        assert!(!should_dead_letter(5));
        assert!(should_dead_letter(MAX_DELIVERY_ATTEMPTS));
    }

    /// 租约成功后按 owner CAS 标记 delivered；失败进入重试。
    #[test]
    fn execution_worker_lease_success_and_retry() {
        let now = Instant::from_unix_secs(1_700_000_000);
        let mut item = enqueue(now);
        item.acquire_lease("worker-1", now, Instant::from_unix_secs(1_700_000_030))
            .unwrap();
        apply_delivery_attempt(&mut item, DeliveryAttempt::Retryable, now).unwrap();
        assert_eq!(item.delivery_status, ApprovalNotificationDeliveryStatus::Pending);
        assert_eq!(item.attempt_count, 1);
        item.acquire_lease(
            "worker-1",
            Instant::from_unix_secs(1_700_000_100),
            Instant::from_unix_secs(1_700_000_130),
        )
        .unwrap();
        apply_delivery_attempt(&mut item, DeliveryAttempt::Delivered, now).unwrap();
        assert_eq!(
            item.delivery_status,
            ApprovalNotificationDeliveryStatus::Delivered
        );
    }

    /// 超过最大次数进入死信。
    #[test]
    fn execution_worker_dead_letters_after_max_attempts() {
        let now = Instant::from_unix_secs(1_700_000_000);
        let mut item = enqueue(now);
        for _ in 0..MAX_DELIVERY_ATTEMPTS {
            let at = item.next_attempt_at;
            item.acquire_lease("worker-1", at, Instant::from_unix_secs(at.unix_secs() + 30))
                .unwrap();
            apply_delivery_attempt(&mut item, DeliveryAttempt::Retryable, at).unwrap();
        }
        assert_eq!(
            item.delivery_status,
            ApprovalNotificationDeliveryStatus::DeadLetter
        );
        assert_eq!(item.attempt_count, MAX_DELIVERY_ATTEMPTS);
    }

    /// 有界租约后按去重键幂等发送，成功 CAS，失败退避。
    #[test]
    fn execution_worker_processes_leased_batch() {
        use super::{process_outbox_batch, OutboxLeaseStore, WorkerBatchOutcome};

        struct AlwaysOk;
        impl super::NotificationSender for AlwaysOk {
            fn send_idempotent(&self, _dedup_key: &str) -> DeliveryAttempt {
                DeliveryAttempt::Delivered
            }
        }

        struct MemoryOutbox {
            items: Vec<ApprovalNotificationOutbox>,
        }

        impl OutboxLeaseStore for MemoryOutbox {
            fn lease_batch(
                &mut self,
                worker_id: &str,
                now: Instant,
                until: Instant,
                limit: u32,
            ) -> crate::errors::Result<Vec<ApprovalNotificationOutbox>> {
                let mut leased = Vec::new();
                for item in self.items.iter_mut() {
                    if leased.len() as u32 >= limit {
                        break;
                    }
                    if item.acquire_lease(worker_id, now, until).is_ok() {
                        leased.push(item.clone());
                    }
                }
                Ok(leased)
            }

            fn mark_delivered(&mut self, id: &str, worker_id: &str) -> crate::errors::Result<()> {
                let item = self
                    .items
                    .iter_mut()
                    .find(|item| item.base.id == id)
                    .ok_or_else(|| crate::errors::Error::NotFound("outbox 不存在".to_string()))?;
                if item.lease_owner.as_deref() != Some(worker_id) {
                    return Err(crate::errors::Error::ConflictError("租约已易主".to_string()));
                }
                item.mark_delivered().map_err(crate::errors::Error::from)
            }

            fn save_after_failure(
                &mut self,
                item: ApprovalNotificationOutbox,
                worker_id: &str,
            ) -> crate::errors::Result<()> {
                if item.lease_owner.as_deref() != Some(worker_id) && item.lease_owner.is_some() {
                    return Err(crate::errors::Error::ConflictError("租约已易主".to_string()));
                }
                if let Some(slot) = self
                    .items
                    .iter_mut()
                    .find(|current| current.base.id == item.base.id)
                {
                    *slot = item;
                }
                Ok(())
            }
        }

        let now = Instant::from_unix_secs(1_700_000_000);
        let mut store = MemoryOutbox {
            items: vec![enqueue(now)],
        };
        let outcome = process_outbox_batch(&mut store, &AlwaysOk, "worker-1", now, 10).unwrap();
        assert_eq!(
            outcome,
            WorkerBatchOutcome {
                delivered: 1,
                retried: 0,
                dead_lettered: 0,
            }
        );
        assert_eq!(
            store.items[0].delivery_status,
            ApprovalNotificationDeliveryStatus::Delivered
        );
    }

    fn enqueue(at: Instant) -> ApprovalNotificationOutbox {
        ApprovalNotificationOutbox::enqueue(
            ApprovalNotificationOutboxId::new("ob-1"),
            "started:inst",
            ApprovalNotificationEventKind::Started,
            vec!["u1".into()],
            ApprovalNotificationTemplateParams {
                document_type_label: "库存调整单".into(),
                document_no: "ADJ-1".into(),
                current_node_name: "仓储复核".into(),
                current_approver_display_name: "张三".into(),
                round_no: 1,
                reject_reason_summary: None,
            },
            at,
        )
        .unwrap()
    }
}
