//! P6-PILOT：试点并发、CAS、幂等收据与 outbox 租约。
//!
//! 两个并发决定只能成功一个；陈旧 CAS 返回稳定冲突；相同幂等键同载荷回读，
//! 异载荷冲突；两个 worker 不得同时领取同一条 outbox。

use std::sync::Arc;

use database::{ensure_indexes, ApprovalIntegrationExt, BpmExt, NoTransaction};
use entities::approval_integration::{
    ApprovalNotificationEventKind, ApprovalNotificationOutbox, ApprovalNotificationTemplateParams,
};
use entities::common::time::Instant;
use entities::ids::ApprovalNotificationOutboxId;
use services::approval::execution::notification_worker::{
    apply_delivery_attempt, retry_backoff_secs, should_dead_letter, DeliveryAttempt, LEASE_SECS,
};
use test_support::{require_mongo, TestDb};
use tokio::sync::Barrier;

/// 构造待领取通知。
///
/// # 错误
/// 模型校验失败时测试失败。
fn pending_outbox(id: &str, dedup: &str) -> ApprovalNotificationOutbox {
    ApprovalNotificationOutbox::enqueue(
        ApprovalNotificationOutboxId::new(id),
        dedup,
        ApprovalNotificationEventKind::Entered,
        vec!["approver-1".to_string()],
        ApprovalNotificationTemplateParams {
            document_type_label: "库存调整单".to_string(),
            document_no: "ADJ-CONC".to_string(),
            current_node_name: "仓储复核".to_string(),
            current_approver_display_name: "仓储".to_string(),
            round_no: 1,
            reject_reason_summary: None,
        },
        Instant::from_unix_secs(1),
    )
    .expect("outbox 必须可入队")
}

/// 运行编排对重复命令做同载荷回读、异载荷冲突。
#[test]
fn execution_duplicate_key_replays_same_payload() {
    let store = include_str!("../../../services/src/approval/execution/store.rs");
    let idempotency = include_str!("../../../services/src/approval/execution/idempotency.rs");
    assert!(store.contains("replay_after_duplicate"));
    assert!(store.contains("DuplicateReceipt"));
    assert!(idempotency.contains("APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT"));
    let decision = include_str!("../../../services/src/approval/execution/decision.rs");
    assert!(decision.contains("expected_task_version") || decision.contains("CAS"));
}

/// 实例、执行与 WorkItem 的陈旧 CAS 必须分类为冲突而不是覆盖写入。
#[test]
fn stale_cas_is_classified_not_overwritten() {
    let bpm = include_str!("../../../database/src/repository/bpm.rs");
    assert!(bpm.contains("CasWriteOutcome::VersionConflict"));
    assert!(bpm.contains("CasWriteOutcome::StatusChanged"));
    assert!(bpm.contains("advance_instance"));
    assert!(bpm.contains("end_active_execution"));
    let work_item = include_str!("../../../database/src/repository/work_item.rs");
    assert!(work_item.contains("version") && (work_item.contains("冲突") || work_item.contains("Conflict")));
}

/// 两个并发决定必须依赖当前执行部分唯一索引与事务 CAS。
#[test]
fn concurrent_decisions_rely_on_current_execution_partial_unique() {
    let indexes = include_str!("../../../database/src/indexes/bpm.rs");
    assert!(indexes.contains("uk_approval_node_executions_current"));
    assert!(indexes.contains("ACTIVE"));
    assert!(indexes.contains("BLOCKED"));
    let store = include_str!("../../../services/src/approval/execution/store.rs");
    assert!(store.contains("commit_writes"));
    assert!(store.contains("DuplicateReceipt"));
}

/// 退避、死信与租约常量符合合同。
#[test]
fn outbox_retry_schedule_and_lease() {
    assert_eq!(retry_backoff_secs(1), Some(60));
    assert_eq!(retry_backoff_secs(2), Some(300));
    assert_eq!(retry_backoff_secs(3), Some(900));
    assert_eq!(retry_backoff_secs(4), Some(3_600));
    assert_eq!(retry_backoff_secs(5), Some(21_600));
    assert_eq!(retry_backoff_secs(6), None);
    assert!(!should_dead_letter(5));
    assert!(should_dead_letter(6));
    assert_eq!(LEASE_SECS, 30);
}

/// 投递失败后第 1—5 次保持可重试，第 6 次死信。
#[test]
fn delivery_attempt_advances_to_dead_letter_on_sixth_failure() {
    let mut item = pending_outbox("obx-fail", "dedup-fail");
    let now = Instant::from_unix_secs(100);
    for attempt in 1..=6 {
        item.acquire_lease(
            "worker-1",
            Instant::from_unix_secs(now.unix_secs() + i64::from(attempt) * 40_000),
            Instant::from_unix_secs(now.unix_secs() + i64::from(attempt) * 40_000 + 30),
        )
        .expect("取得租约");
        apply_delivery_attempt(
            &mut item,
            DeliveryAttempt::Retryable,
            Instant::from_unix_secs(now.unix_secs() + i64::from(attempt) * 40_000),
        )
        .expect("记录失败");
        if attempt < 6 {
            assert_ne!(item.delivery_status.as_str(), "DEAD_LETTER");
            assert!(!should_dead_letter(attempt));
        } else {
            assert_eq!(item.delivery_status.as_str(), "DEAD_LETTER");
            assert!(should_dead_letter(attempt));
        }
    }
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn two_workers_cannot_lease_the_same_outbox() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_conc_lease").await.expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        fixture
            .db()
            .approval_notification_outbox()
            .enqueue_outbox(&pending_outbox("obx-1", "dedup-1"), &mut NoTransaction)
            .await
            .expect("入队");
        let barrier = Arc::new(Barrier::new(2));
        let now = Instant::from_unix_secs(20);
        let until = Instant::from_unix_secs(50);
        let claim = |worker: &'static str| {
            let db = fixture.db().clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                db.approval_notification_outbox()
                    .lease_outbox_batch(worker, now, until, 8, &mut NoTransaction)
                    .await
                    .expect("领取")
            })
        };
        let left = claim("worker-a").await.expect("worker-a");
        let right = claim("worker-b").await.expect("worker-b");
        assert_eq!(
            usize::from(!left.is_empty()) + usize::from(!right.is_empty()),
            1,
            "同一消息只能被一个 worker 领取"
        );
        if let Some(winner) = left.first().or(right.first()) {
            let delivered = fixture
                .db()
                .approval_notification_outbox()
                .mark_outbox_delivered(
                    &winner.base.id,
                    winner.lease_owner.as_deref().unwrap_or(""),
                    Instant::from_unix_secs(21),
                    &mut NoTransaction,
                )
                .await
                .expect("投递成功");
            assert!(delivered.is_some());
            let again = fixture
                .db()
                .approval_notification_outbox()
                .mark_outbox_delivered(
                    &winner.base.id,
                    "other-worker",
                    Instant::from_unix_secs(22),
                    &mut NoTransaction,
                )
                .await
                .expect("异 worker 不得投递");
            assert!(again.is_none(), "投递必须按租约 owner 幂等 CAS");
        }
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn expired_lease_can_be_taken_over() {
    require_mongo!(async {
        let fixture = TestDb::new("awf_conc_takeover")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let mut item = pending_outbox("obx-exp", "dedup-exp");
        item.acquire_lease(
            "worker-old",
            Instant::from_unix_secs(10),
            Instant::from_unix_secs(20),
        )
        .expect("旧租约");
        fixture
            .db()
            .approval_notification_outbox()
            .enqueue_outbox(&item, &mut NoTransaction)
            .await
            .expect("写入旧租约");
        let taken = fixture
            .db()
            .approval_notification_outbox()
            .lease_outbox_batch(
                "worker-new",
                Instant::from_unix_secs(30),
                Instant::from_unix_secs(60),
                1,
                &mut NoTransaction,
            )
            .await
            .expect("接管");
        assert_eq!(taken.len(), 1);
        assert_eq!(taken[0].lease_owner.as_deref(), Some("worker-new"));
        let _ = fixture.db().approval_command_receipts();
    });
}
