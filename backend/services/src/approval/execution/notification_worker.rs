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
