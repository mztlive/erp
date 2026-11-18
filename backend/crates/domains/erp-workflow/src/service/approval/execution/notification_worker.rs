//! 通知 outbox worker：租约、幂等发送、退避与死信。

use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::entity::approval_integration::{
    ApprovalNotificationDeliveryStatus, ApprovalNotificationOutbox, MAX_DELIVERY_ATTEMPTS, RETRY_BACKOFF_SECS,
};
use crate::error::{Error, Result};
use crate::repository::ApprovalIntegrationExt;
use crate::repository::prelude::*;

/// 单次 outbox 领取上限。
const OUTBOX_BATCH_LIMIT: u32 = 20;

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
/// 下标仍落在退避表内时返回秒数；超出表长时返回 `None`。
///
/// `attempt_count`：已失败次数，从 1 开始。
pub fn retry_backoff_secs(attempt_count: u32) -> Option<i64> {
    let index = usize::try_from(attempt_count.saturating_sub(1)).ok()?;
    RETRY_BACKOFF_SECS.get(index).copied()
}

/// 判断是否应进入死信。
/// 达到最大次数时返回 `true`。
pub fn should_dead_letter(attempt_count: u32) -> bool {
    attempt_count >= MAX_DELIVERY_ATTEMPTS
}

/// 在已持有租约的消息上应用一次发送结果。
/// 消息不处于投递中时返回错误。
///
/// `item`：租约中的 outbox。
pub fn apply_delivery_attempt(
    item: &mut ApprovalNotificationOutbox,
    attempt: DeliveryAttempt,
    failed_at: Instant,
) -> Result<()> {
    match attempt {
        DeliveryAttempt::Delivered => item.mark_delivered().map_err(Error::from),
        DeliveryAttempt::Retryable => item.mark_failure(error_class(attempt), failed_at).map_err(Error::from),
        DeliveryAttempt::Fatal => {
            item.mark_fatal_failure(error_class(attempt), failed_at).map_err(Error::from)
        },
    }
}

// 将发送失败归为可持久化的稳定分类；不把收件人或敏感通知载荷写入失败信息。
fn error_class(attempt: DeliveryAttempt) -> &'static str {
    match attempt {
        DeliveryAttempt::Delivered => "delivered",
        DeliveryAttempt::Retryable => "retryable",
        DeliveryAttempt::Fatal => "fatal",
    }
}

/// 计算租约截止时间。
/// 返回租约截止。
///
/// `now`：当前时间。
pub fn lease_until(now: Instant) -> Instant {
    Instant::from_unix_secs(now.unix_secs().saturating_add(LEASE_SECS))
}

/// 幂等发送接口。必须按去重键调用。
pub trait NotificationSender {
    /// 当实际发送提供方已接入时返回 `true`。
    /// 未接入时返回 `false`，worker 不得领取消息或消耗重试次数。
    fn is_ready(&self) -> bool;

    /// 以 outbox 去重键发送。
    /// 返回投递结果。
    fn send_idempotent(&self, dedup_key: &str) -> DeliveryAttempt;
}

/// 有界租约与 CAS 投递状态。
pub trait OutboxLeaseStore {
    /// 原子领取一批可投递消息。
    /// 返回已写入 worker ID 的消息。
    ///
    /// 领取租约失败时返回实现方错误。
    ///
    /// `now`：当前时间。
    fn lease_batch(
        &mut self,
        worker_id: &str,
        now: Instant,
        until: Instant,
        limit: u32,
    ) -> Result<Vec<ApprovalNotificationOutbox>>;

    /// 按租约 owner CAS 标记成功。
    /// 标记成功时无返回值。
    ///
    /// owner 不匹配时返回冲突。
    fn mark_delivered(&mut self, id: &str, worker_id: &str) -> Result<()>;

    /// 按租约 owner 写回失败后的实体。
    /// 写回成功时无返回值。
    ///
    /// owner 不匹配时返回冲突。
    ///
    /// `item`：已更新的 outbox。
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
/// 返回本批成功、重试与死信计数。
///
/// 租约、发送结果落库或 CAS 失败时返回错误。
///
/// `worker_id`：本实例 worker；`now`：当前时间。
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
    if !sender.is_ready() {
        return Ok(WorkerBatchOutcome::default());
    }

    let until = lease_until(now);
    let leased = store.lease_batch(worker_id, now, until, limit)?;

    let mut outcome = WorkerBatchOutcome::default();
    for mut item in leased {
        let attempt = sender.send_idempotent(&item.dedup_key);
        match attempt {
            DeliveryAttempt::Delivered => {
                store.mark_delivered(&item.base.id, worker_id)?;
                outcome.delivered = outcome.delivered.saturating_add(1);
            },
            DeliveryAttempt::Retryable | DeliveryAttempt::Fatal => {
                apply_delivery_attempt(&mut item, attempt, now)?;
                if item.delivery_status
                    == crate::entity::approval_integration::ApprovalNotificationDeliveryStatus::DeadLetter
                {
                    outcome.dead_lettered = outcome.dead_lettered.saturating_add(1);
                } else {
                    outcome.retried = outcome.retried.saturating_add(1);
                }
                store.save_after_failure(item, worker_id)?;
            },
        }
    }
    Ok(outcome)
}

/// 未接入通知提供方时失败关闭，不得伪造成功投递。
#[derive(Clone, Copy, Debug, Default)]
pub struct FailClosedNotificationSender;

impl NotificationSender for FailClosedNotificationSender {
    fn is_ready(&self) -> bool {
        false
    }

    // 以去重键调用发送口；提供方未接线时返回致命失败。
    // 始终返回 `Fatal`；正常 worker 必须在领取前检查就绪状态。
    //
    // `dedup_key`：outbox 业务去重键。
    fn send_idempotent(&self, dedup_key: &str) -> DeliveryAttempt {
        tracing::error!(dedup_key = %dedup_key, "审批通知提供方未接入，投递失败关闭");
        DeliveryAttempt::Fatal
    }
}

/// 已提交审批通知的异步投递端口；领域不依赖具体通知实现。
#[async_trait]
pub trait ApprovalNotificationDelivery: Send + Sync {
    /// 使用完整不可变内容及去重键投递。
    /// 成功、暂时失败或永久失败分类。
    ///
    /// 失败以 DeliveryAttempt 返回，交给现有 outbox 退避及死信。
    ///
    /// item 为本轮已领取的通知。
    async fn deliver(&self, item: &ApprovalNotificationOutbox) -> DeliveryAttempt;
}

/// Mongo 审批通知 outbox 应用端口。HTTP 只持有并启停，不得直连仓储。
pub struct ApprovalNotificationOutboxPort {
    db: Database,
    sender: Option<Arc<dyn ApprovalNotificationDelivery>>,
}

impl ApprovalNotificationOutboxPort {
    /// 构造失败关闭的通知 outbox 端口。
    /// 返回真实仓储端口，发送口未接入时失败关闭。
    pub fn new(db: Database) -> Self {
        Self { db, sender: None }
    }

    /// 装配已接入的异步通知提供方。
    /// 可领取及处理通知的 outbox 服务。
    ///
    /// 不执行 I/O。
    ///
    /// db 为仓储数据库，sender 为组合根注入的投递端口。
    pub fn with_sender(db: Database, sender: Arc<dyn ApprovalNotificationDelivery>) -> Self {
        Self { db, sender: Some(sender) }
    }

    /// 当实际通知提供方已接入时返回 `true`。
    /// 未接入时返回 `false`，调用方应暂停 worker。
    pub fn is_ready(&self) -> bool {
        self.sender.is_some()
    }

    /// 领取并处理一批到期消息；未接入提供方时直接返回，不读写队列。
    /// 租约或 CAS 落库失败时返回服务错误。
    pub async fn process_tick(&self, worker_id: &str) -> Result<()> {
        if !self.is_ready() {
            return Ok(());
        }

        let now = Instant::now();
        let until = lease_until(now);
        let leased = self
            .db
            .approval_notification_outbox()
            .lease_outbox_batch(worker_id, now, until, OUTBOX_BATCH_LIMIT, &mut NoTransaction)
            .await?;

        for item in leased {
            self.persist_attempt(worker_id, item).await?;
        }
        Ok(())
    }

    // 对已租约消息应用一次发送结果并 CAS 写回。
    // 实体状态或仓储 CAS 失败时返回错误。
    //
    // `item`：已领取消息。
    async fn persist_attempt(&self, worker_id: &str, mut item: ApprovalNotificationOutbox) -> Result<()> {
        let Some(sender) = &self.sender else {
            return Ok(());
        };
        let attempt = sender.deliver(&item).await;
        let now = Instant::now();

        if attempt == DeliveryAttempt::Delivered {
            self.mark_delivered(&item.base.id, worker_id, now).await?;
            return Ok(());
        }
        apply_delivery_attempt(&mut item, attempt, now)?;
        self.save_failure(worker_id, item, attempt, now).await
    }

    // 按租约 owner CAS 标记投递成功。
    // 仓储写入失败时返回错误。
    async fn mark_delivered(&self, outbox_id: &str, worker_id: &str, now: Instant) -> Result<()> {
        self.db
            .approval_notification_outbox()
            .mark_outbox_delivered(outbox_id, worker_id, now, &mut NoTransaction)
            .await?;
        Ok(())
    }

    // 按发送结果写回退避或死信。
    // 仓储 CAS 失败时返回错误。
    async fn save_failure(
        &self,
        worker_id: &str,
        item: ApprovalNotificationOutbox,
        attempt: DeliveryAttempt,
        failed_at: Instant,
    ) -> Result<()> {
        let repo = self.db.approval_notification_outbox();
        if item.delivery_status == ApprovalNotificationDeliveryStatus::DeadLetter {
            repo.dead_letter_outbox(
                &item.base.id,
                worker_id,
                failed_at,
                error_class(attempt),
                &mut NoTransaction,
            )
            .await?;
            return Ok(());
        }
        repo.reschedule_outbox(
            &item.base.id,
            worker_id,
            item.next_attempt_at,
            failed_at,
            error_class(attempt),
            &mut NoTransaction,
        )
        .await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use async_trait::async_trait;
    use erp_core::common::time::Instant;
    use erp_core::ids::ApprovalNotificationOutboxId;

    use super::{
        DeliveryAttempt, MAX_DELIVERY_ATTEMPTS, apply_delivery_attempt, retry_backoff_secs,
        should_dead_letter,
    };
    use crate::entity::approval_integration::{
        ApprovalNotificationDeliveryStatus, ApprovalNotificationEventKind, ApprovalNotificationOutbox,
        ApprovalNotificationTemplateParams,
    };

    #[tokio::test]
    async fn async_delivery_port_is_explicitly_wired_and_unconfigured_tick_is_idle() {
        use mongodb::Client;

        use super::{ApprovalNotificationDelivery, ApprovalNotificationOutboxPort};

        struct Ready;
        #[async_trait]
        impl ApprovalNotificationDelivery for Ready {
            async fn deliver(&self, item: &ApprovalNotificationOutbox) -> DeliveryAttempt {
                assert_eq!(item.recipient_user_ids, ["u1"]);
                DeliveryAttempt::Delivered
            }
        }
        let db = Client::with_options(Default::default()).unwrap().database("unused");
        let paused = ApprovalNotificationOutboxPort::new(db.clone());
        assert!(!paused.is_ready());
        paused.process_tick("worker").await.unwrap();
        let port = ApprovalNotificationOutboxPort::with_sender(db, Arc::new(Ready));
        assert!(port.is_ready());
    }

    // 验证：未接线发送口必须失败关闭，不得伪造成功。
    #[test]
    fn unconfigured_sender_fails_closed() {
        let sender = super::FailClosedNotificationSender;
        assert!(!super::NotificationSender::is_ready(&sender));
        assert_eq!(
            super::NotificationSender::send_idempotent(&sender, "started:inst-1"),
            DeliveryAttempt::Fatal
        );
    }

    // 验证：退避表与死信次数符合合同。
    #[test]
    fn execution_worker_backoff_and_dead_letter() {
        assert_eq!(retry_backoff_secs(1), Some(60));
        assert_eq!(retry_backoff_secs(5), Some(21_600));
        assert_eq!(retry_backoff_secs(6), None);
        assert!(!should_dead_letter(5));
        assert!(should_dead_letter(MAX_DELIVERY_ATTEMPTS));
    }

    // 验证：租约成功后按 owner CAS 标记 delivered；失败进入重试。
    #[test]
    fn execution_worker_lease_success_and_retry() {
        let now = Instant::from_unix_secs(1_700_000_000);
        let mut item = enqueue(now);
        item.acquire_lease("worker-1", now, Instant::from_unix_secs(1_700_000_030)).unwrap();
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
        assert_eq!(item.delivery_status, ApprovalNotificationDeliveryStatus::Delivered);
    }

    // 验证：致命失败立即死信；没有租约时必须拒绝状态变更。
    #[test]
    fn execution_worker_fatal_failure_stops_retrying() {
        let now = Instant::from_unix_secs(1_700_000_000);
        let mut item = enqueue(now);
        let original = item.clone();
        assert!(apply_delivery_attempt(&mut item, DeliveryAttempt::Fatal, now).is_err());
        assert_eq!(item, original);

        item.acquire_lease("worker-1", now, super::lease_until(now)).unwrap();
        apply_delivery_attempt(&mut item, DeliveryAttempt::Fatal, now).unwrap();
        assert_eq!(item.delivery_status, ApprovalNotificationDeliveryStatus::DeadLetter);
        assert_eq!(item.attempt_count, 1);
        assert_eq!(item.dead_lettered_at, Some(now));
        assert_eq!(item.last_error_class.as_deref(), Some("fatal"));
        assert!(item.lease_owner.is_none());
        assert!(item.lease_until.is_none());
        assert!(item.acquire_lease("worker-2", now, super::lease_until(now)).is_err());
    }

    // 验证：超过最大次数进入死信。
    #[test]
    fn execution_worker_dead_letters_after_max_attempts() {
        let now = Instant::from_unix_secs(1_700_000_000);
        let mut item = enqueue(now);
        for _ in 0..MAX_DELIVERY_ATTEMPTS {
            let at = item.next_attempt_at;
            item.acquire_lease("worker-1", at, Instant::from_unix_secs(at.unix_secs() + 30)).unwrap();
            apply_delivery_attempt(&mut item, DeliveryAttempt::Retryable, at).unwrap();
        }
        assert_eq!(item.delivery_status, ApprovalNotificationDeliveryStatus::DeadLetter);
        assert_eq!(item.attempt_count, MAX_DELIVERY_ATTEMPTS);
    }

    // 验证：有界租约后按去重键幂等发送，成功 CAS，失败退避。
    #[test]
    fn execution_worker_processes_leased_batch() {
        use super::{OutboxLeaseStore, WorkerBatchOutcome, process_outbox_batch};

        struct AlwaysOk;
        impl super::NotificationSender for AlwaysOk {
            fn is_ready(&self) -> bool {
                true
            }
            fn send_idempotent(&self, _dedup_key: &str) -> DeliveryAttempt {
                DeliveryAttempt::Delivered
            }
        }

        struct MemoryOutbox {
            items: Vec<ApprovalNotificationOutbox>,
            lease_calls: u32,
        }

        impl OutboxLeaseStore for MemoryOutbox {
            fn lease_batch(
                &mut self,
                worker_id: &str,
                now: Instant,
                until: Instant,
                limit: u32,
            ) -> crate::error::Result<Vec<ApprovalNotificationOutbox>> {
                self.lease_calls += 1;
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

            fn mark_delivered(&mut self, id: &str, worker_id: &str) -> crate::error::Result<()> {
                let item = self
                    .items
                    .iter_mut()
                    .find(|item| item.base.id == id)
                    .ok_or_else(|| crate::error::Error::NotFound("outbox 不存在".to_string()))?;
                if item.lease_owner.as_deref() != Some(worker_id) {
                    return Err(crate::error::Error::ConflictError("租约已易主".to_string()));
                }
                item.mark_delivered().map_err(crate::error::Error::from)
            }

            fn save_after_failure(
                &mut self,
                item: ApprovalNotificationOutbox,
                worker_id: &str,
            ) -> crate::error::Result<()> {
                if item.lease_owner.as_deref() != Some(worker_id) && item.lease_owner.is_some() {
                    return Err(crate::error::Error::ConflictError("租约已易主".to_string()));
                }
                if let Some(slot) = self.items.iter_mut().find(|current| current.base.id == item.base.id) {
                    *slot = item;
                }
                Ok(())
            }
        }

        let now = Instant::from_unix_secs(1_700_000_000);
        let mut store = MemoryOutbox { items: vec![enqueue(now)], lease_calls: 0 };
        let original = store.items.clone();
        let paused =
            process_outbox_batch(&mut store, &super::FailClosedNotificationSender, "worker-1", now, 10)
                .unwrap();
        assert_eq!(paused, WorkerBatchOutcome::default());
        assert_eq!(store.items, original);
        assert_eq!(store.lease_calls, 0);

        let outcome = process_outbox_batch(&mut store, &AlwaysOk, "worker-1", now, 10).unwrap();
        assert_eq!(outcome, WorkerBatchOutcome { delivered: 1, retried: 0, dead_lettered: 0 });
        assert_eq!(store.items[0].delivery_status, ApprovalNotificationDeliveryStatus::Delivered);
        assert_eq!(store.lease_calls, 1);
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
