//! D03 审批运行时责任事实的 MongoDB 集成回归测试。

use std::sync::Arc;

use database::{
    ensure_indexes, ApprovalExt, Error as DatabaseError, NoTransaction, StartProcessingEligibility,
    StartProcessingOutcome, WorkItemExt,
};
use entities::{
    approval::{ApprovalDecision, ApprovalStepInstance, ApprovalStepInstanceData, ApprovalStepStatus},
    common::time::Instant,
    ids::{ApprovalInstanceId, ApprovalStepInstanceId, WorkItemId},
    work_item::{AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType},
};
use test_support::{require_mongo, TestDb};
use tokio::sync::Barrier;

/// 构造确定性的审批任务数据。
fn work_item_data(
    assignment_mode: AssignmentMode,
    owner_user_id: Option<&str>,
    approval_step_instance_id: &str,
    business_object_id: &str,
) -> WorkItemData {
    WorkItemData {
        work_item_type: WorkItemType::CardSalesManagerApproval,
        approval_step_instance_id: Some(approval_step_instance_id.to_string()),
        business_object_type: "SALES_ORDER_SUBMISSION".to_string(),
        business_object_id: business_object_id.to_string(),
        subject_version: "submission-v1".to_string(),
        assignment_mode,
        owner_role: "role-sales-manager".to_string(),
        owner_organization_id: "company".to_string(),
        owner_user_id: owner_user_id.map(str::to_string),
        assignment_source: AssignmentSource::StepResolver,
        priority: WorkItemPriority::Normal,
        due_at: None,
        reason_code: None,
        impact_summary: None,
    }
}

/// 构造未分派的 POOL 任务。
fn pool_work_item(id: &str, step_id: &str, object_id: &str, at: Instant) -> WorkItem {
    WorkItem::new_at(
        WorkItemId::new(id),
        work_item_data(AssignmentMode::Pool, None, step_id, object_id),
        at,
    )
    .expect("POOL 任务夹具构造失败")
}

/// 构造已直接分派的 DIRECT 任务。
fn direct_work_item(id: &str, step_id: &str, object_id: &str, owner: &str, at: Instant) -> WorkItem {
    WorkItem::new_at(
        WorkItemId::new(id),
        work_item_data(AssignmentMode::Direct, Some(owner), step_id, object_id),
        at,
    )
    .expect("DIRECT 任务夹具构造失败")
}

/// 构造审批步骤实例。
fn approval_step(
    id: &str,
    approval_instance_id: &str,
    sequence_no: u32,
    initial_status: ApprovalStepStatus,
) -> ApprovalStepInstance {
    ApprovalStepInstance::new(
        ApprovalStepInstanceId::new(id),
        ApprovalStepInstanceData {
            approval_instance_id: ApprovalInstanceId::new(approval_instance_id),
            step_key: format!("step-{sequence_no}"),
            sequence_no,
            initial_status,
            external_activity_id: None,
        },
    )
    .expect("审批步骤夹具构造失败")
}

#[test]
fn direct_and_pool_assignment_invariants_are_enforced() {
    let created_at = Instant::from_unix_secs(100);
    let direct = direct_work_item(
        "work-direct",
        "step-direct",
        "submission-direct",
        "alice",
        created_at,
    );
    assert_eq!(direct.owner_user_id.as_deref(), Some("alice"));
    assert_eq!(direct.responsibility_actor_ids, vec!["alice".to_string()]);
    assert_eq!(direct.assigned_at, Some(created_at));
    assert_eq!(direct.current_assignment_at, Some(created_at));
    assert!(direct.started_at.is_none());

    let pool = pool_work_item("work-pool", "step-pool", "submission-pool", created_at);
    assert!(pool.owner_user_id.is_none());
    assert!(pool.responsibility_actor_ids.is_empty());
    assert!(pool.assigned_at.is_none());
    assert!(pool.current_assignment_at.is_none());
    assert!(pool.started_at.is_none());

    let missing_direct_owner = work_item_data(
        AssignmentMode::Direct,
        None,
        "step-invalid-direct",
        "submission-invalid-direct",
    );
    assert!(WorkItem::new_at(
        WorkItemId::new("work-invalid-direct"),
        missing_direct_owner,
        created_at,
    )
    .is_err());

    let preassigned_pool = work_item_data(
        AssignmentMode::Pool,
        Some("alice"),
        "step-invalid-pool",
        "submission-invalid-pool",
    );
    assert!(WorkItem::new_at(WorkItemId::new("work-invalid-pool"), preassigned_pool, created_at,).is_err());
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn pool_start_is_atomic_idempotent_and_preserves_first_times() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_pool_start")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");

        let created_at = Instant::from_unix_secs(100);
        let started_at = Instant::from_unix_secs(110);
        let item = pool_work_item("work-pool-1", "step-pool-1", "submission-pool-1", created_at);
        fixture
            .db()
            .work_items()
            .create(&item, &mut NoTransaction)
            .await
            .expect("POOL 任务写入失败");

        let barrier = Arc::new(Barrier::new(2));
        let start = |actor: &'static str| {
            let db = fixture.db().clone();
            let barrier = Arc::clone(&barrier);
            tokio::spawn(async move {
                barrier.wait().await;
                let outcome = db
                    .work_items()
                    .start_processing(
                        "work-pool-1",
                        1,
                        StartProcessingEligibility {
                            owner_role: "role-sales-manager",
                            owner_organization_id: "company",
                        },
                        actor,
                        started_at,
                        &mut NoTransaction,
                    )
                    .await
                    .expect("开始处理失败");
                (actor, outcome)
            })
        };
        let alice = start("alice");
        let bob = start("bob");
        let attempts = [
            alice.await.expect("alice 并发任务失败"),
            bob.await.expect("bob 并发任务失败"),
        ];

        let mut winner = None;
        let mut ownership_conflicts = 0;
        for (actor, outcome) in attempts {
            match outcome {
                StartProcessingOutcome::Started(item) => {
                    assert_eq!(item.owner_user_id.as_deref(), Some(actor));
                    assert_eq!(item.responsibility_actor_ids, vec![actor.to_string()]);
                    assert_eq!(item.base.version, 2);
                    winner = Some(actor);
                }
                StartProcessingOutcome::OwnershipConflict(item) => {
                    assert_ne!(item.owner_user_id.as_deref(), Some(actor));
                    ownership_conflicts += 1;
                }
                other => panic!("并发开始处理返回了非预期结果: {other:?}"),
            }
        }
        let winner = winner.expect("两名处理人中必须有且仅有一人成功");
        assert_eq!(ownership_conflicts, 1);

        let retried = fixture
            .db()
            .work_items()
            .start_processing(
                "work-pool-1",
                1,
                StartProcessingEligibility {
                    owner_role: "role-sales-manager",
                    owner_organization_id: "company",
                },
                winner,
                Instant::from_unix_secs(120),
                &mut NoTransaction,
            )
            .await
            .expect("同人重试失败");
        let mut current = match retried {
            StartProcessingOutcome::AlreadyOwned(item) => item,
            other => panic!("同人重试未按幂等成功处理: {other:?}"),
        };
        assert_eq!(current.base.version, 2);
        assert_eq!(current.assigned_at, Some(started_at));
        assert_eq!(current.started_at, Some(started_at));
        assert_eq!(current.current_assignment_at, Some(started_at));
        assert_eq!(current.responsibility_actor_ids, vec![winner.to_string()]);

        current
            .release_to_pool(Instant::from_unix_secs(130))
            .expect("退回责任池失败");
        fixture
            .db()
            .work_items()
            .update(&mut current, &mut NoTransaction)
            .await
            .expect("退回责任池持久化失败");
        assert_eq!(current.base.version, 3);
        assert_eq!(current.assigned_at, Some(started_at));
        assert_eq!(current.started_at, Some(started_at));
        assert!(current.current_assignment_at.is_none());
        assert_eq!(current.responsibility_actor_ids, vec![winner.to_string()]);

        let next_actor = if winner == "alice" { "bob" } else { "alice" };
        let restarted_at = Instant::from_unix_secs(140);
        let restarted = fixture
            .db()
            .work_items()
            .start_processing(
                "work-pool-1",
                3,
                StartProcessingEligibility {
                    owner_role: "role-sales-manager",
                    owner_organization_id: "company",
                },
                next_actor,
                restarted_at,
                &mut NoTransaction,
            )
            .await
            .expect("再次开始处理失败");
        let mut current = match restarted {
            StartProcessingOutcome::Started(item) => item,
            other => panic!("退回后再次开始处理返回了非预期结果: {other:?}"),
        };
        assert_eq!(current.base.version, 4);
        assert_eq!(current.assigned_at, Some(started_at));
        assert_eq!(current.started_at, Some(started_at));
        assert_eq!(current.current_assignment_at, Some(restarted_at));
        assert_eq!(
            current.responsibility_actor_ids,
            vec![winner.to_string(), next_actor.to_string()]
        );

        let reassigned_at = Instant::from_unix_secs(150);
        current.reassign("charlie", reassigned_at).expect("任务转交失败");
        fixture
            .db()
            .work_items()
            .update(&mut current, &mut NoTransaction)
            .await
            .expect("任务转交持久化失败");
        assert_eq!(current.assigned_at, Some(started_at));
        assert_eq!(current.started_at, Some(started_at));
        assert_eq!(current.current_assignment_at, Some(reassigned_at));
        assert_eq!(
            current.responsibility_actor_ids,
            vec![winner.to_string(), next_actor.to_string(), "charlie".to_string(),]
        );
    });
}

#[tokio::test]
#[ignore = "需要 ERP_TEST_MONGO_URI 指向 MongoDB 副本集"]
async fn open_work_item_and_current_step_uniqueness_release_after_terminal_state() {
    require_mongo!(async {
        let fixture = TestDb::new("approval_open_unique")
            .await
            .expect("测试数据库创建失败");
        ensure_indexes(fixture.db()).await.expect("索引创建失败");
        let at = Instant::from_unix_secs(200);

        let mut first_item = direct_work_item(
            "work-current-1",
            "work-step-current",
            "submission-current-1",
            "alice",
            at,
        );
        let second_item = direct_work_item(
            "work-current-2",
            "work-step-current",
            "submission-current-2",
            "bob",
            at,
        );
        fixture
            .db()
            .work_items()
            .create(&first_item, &mut NoTransaction)
            .await
            .expect("首个开放任务写入失败");
        let duplicate = fixture
            .db()
            .work_items()
            .create(&second_item, &mut NoTransaction)
            .await
            .expect_err("同一审批步骤不得存在两个开放任务");
        assert!(matches!(duplicate, DatabaseError::DuplicateKey(_)));
        assert_eq!(
            duplicate.duplicate_index_name(),
            Some("uk_work_items_open_approval_step")
        );

        first_item
            .complete_by_domain_command("alice", Instant::from_unix_secs(210))
            .expect("任务完成失败");
        fixture
            .db()
            .work_items()
            .update(&mut first_item, &mut NoTransaction)
            .await
            .expect("任务终态持久化失败");
        fixture
            .db()
            .work_items()
            .create(&second_item, &mut NoTransaction)
            .await
            .expect("原任务终态后应允许创建新的开放任务");

        let mut first_step = approval_step(
            "approval-step-current-1",
            "approval-instance-current",
            1,
            ApprovalStepStatus::Active,
        );
        let second_step = approval_step(
            "approval-step-current-2",
            "approval-instance-current",
            2,
            ApprovalStepStatus::Active,
        );
        fixture
            .db()
            .approval_step_instances()
            .create(&first_step, &mut NoTransaction)
            .await
            .expect("首个当前步骤写入失败");
        let duplicate = fixture
            .db()
            .approval_step_instances()
            .create(&second_step, &mut NoTransaction)
            .await
            .expect_err("同一审批实例不得存在两个当前步骤");
        assert!(matches!(duplicate, DatabaseError::DuplicateKey(_)));
        assert_eq!(
            duplicate.duplicate_index_name(),
            Some("uk_approval_step_instances_current")
        );

        first_step
            .decide(
                ApprovalDecision::Approve,
                None,
                "alice",
                Instant::from_unix_secs(220),
            )
            .expect("审批步骤形成决定失败");
        fixture
            .db()
            .approval_step_instances()
            .update(&mut first_step, &mut NoTransaction)
            .await
            .expect("审批步骤终态持久化失败");
        fixture
            .db()
            .approval_step_instances()
            .create(&second_step, &mut NoTransaction)
            .await
            .expect("原步骤终态后应允许激活下一步骤");
    });
}
