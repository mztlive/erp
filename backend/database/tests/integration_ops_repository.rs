//! 域 D34 `integration_ops` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test integration_ops_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。
//!
//! 覆盖 P2 §3.2 验收矩阵：创建+读取往返、乐观锁成功/冲突、唯一索引冲突、
//! 索引存在性、事务参与与回滚、列表查询（分页/排序白名单/投影）、
//! 多步骤方法（事务提交 / 冲突整体回滚 / `NoTransaction` 可预期行为）。
//! 本域四张集合均为事实类或不可变记录，不提供软删除方法（base 泛型
//! `soft_delete`/`restore` 不在本域调用面暴露），以「写入后 `deleted_at` 恒为 0」
//! 验证无软删除语义。

use database::repository::extensions::IntegrationOpsExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::integration_ops::{
    ErrorClass, ErrorTaskStatus, InboxMessage, InboxMessageData, InboxMessageId, InboxMessageStatus,
    InboxMessageUpdate, IntegrationErrorTask, IntegrationErrorTaskData, IntegrationErrorTaskId, MessageType,
    ReconciliationDifference, ReconciliationDifferenceData, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionData,
    ReconciliationDifferenceResolutionId, ResolutionAction, ResolutionType, ResultingStatus, SourceSystemId,
};
use mongodb::bson::{doc, Document};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 入站消息列表筛选条件类型（经 `IntegrationOpsExt` 关联类型跨 crate 可达）。
type InboxMessageFilter = <Database as IntegrationOpsExt>::InboxMessageFilter;
/// 错误任务列表筛选条件类型。
type IntegrationErrorTaskFilter = <Database as IntegrationOpsExt>::IntegrationErrorTaskFilter;
/// 对账差异列表筛选条件类型。
type ReconciliationDifferenceFilter = <Database as IntegrationOpsExt>::ReconciliationDifferenceFilter;

/// 构造可复用的入站消息实体（接收时间由 `suffix` 区分，保证列表排序确定）。
fn sample_message(suffix: &str) -> InboxMessage {
    InboxMessage::new(
        InboxMessageId::new(format!("msg-{suffix}")),
        InboxMessageData {
            source_system_id: SourceSystemId::new("sys-mall-1"),
            source_event_id: format!("evt-{suffix}"),
            message_type: MessageType::PaymentSucceeded,
            business_fact_key: format!("mall-1|PAYMENT_SUCCEEDED|SO-{suffix}|v3"),
            payload_schema_version: "v1.2".to_string(),
            payload_reference: Some(format!("archive://msg-{suffix}")),
            status: InboxMessageStatus::Received,
            source_sent_at: Some(Instant::from_unix_secs(1_699_999_900)),
            received_at: Instant::from_unix_secs(1_700_000_000 + suffix.len() as i64),
            processed_at: None,
        },
    )
    .unwrap()
}

/// 构造可复用的集成错误任务实体。
fn sample_task(message_id: &InboxMessageId, suffix: &str, error_class: ErrorClass) -> IntegrationErrorTask {
    IntegrationErrorTask::new(
        IntegrationErrorTaskId::new(format!("task-{suffix}")),
        IntegrationErrorTaskData {
            message_id: Some(message_id.clone()),
            business_object_id: None,
            error_class,
            owner_role: Some("ops".to_string()),
            owner_user_id: Some("u-1".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的对账差异实体。
fn sample_difference(suffix: &str) -> ReconciliationDifference {
    ReconciliationDifference::new(
        ReconciliationDifferenceId::new(format!("diff-{suffix}")),
        ReconciliationDifferenceData {
            business_object_type: "mall_order".to_string(),
            business_object_id: format!("MO-{suffix}"),
            difference_type: "amount_mismatch".to_string(),
            left_fact_reference: Some(format!("mall_order_fact://f-{suffix}")),
            right_fact_reference: Some("invoice://inv-88".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的差异解决记录实体（`resolution_no` 从 1 起递增）。
fn sample_resolution(
    difference_id: &ReconciliationDifferenceId,
    no: u32,
    action: ResolutionAction,
) -> ReconciliationDifferenceResolution {
    ReconciliationDifferenceResolution::new(
        ReconciliationDifferenceResolutionId::new(format!("res-{}-{no}", difference_id.as_ref())),
        ReconciliationDifferenceResolutionData {
            reconciliation_difference_id: difference_id.clone(),
            resolution_no: no,
            resolution_action: action,
            resulting_status: action.derived_status(),
            evidence_reference: (action == ResolutionAction::CreateCorrection)
                .then(|| "sales_change_order://co-1".to_string()),
            replacement_task_id: None,
            handled_by: format!("ops-{no}"),
            handled_at: Instant::from_unix_secs(1_700_000_000 + i64::from(no)),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位（§6.21 逐条对照）。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as IntegrationOpsExt>::INBOX_MESSAGES,
        &[
            "uk_inbox_messages_identity",
            "uk_inbox_messages_business_fact",
            "idx_inbox_messages_backlog",
        ],
    )
    .await
    .expect("inbox_messages 索引缺失");
    assert_indexes(
        db,
        <Database as IntegrationOpsExt>::INTEGRATION_ERROR_TASKS,
        &[
            "uk_integration_error_tasks_message_class",
            "idx_integration_error_tasks_work_queue",
        ],
    )
    .await
    .expect("integration_error_tasks 索引缺失");
    assert_indexes(
        db,
        <Database as IntegrationOpsExt>::RECONCILIATION_DIFFERENCES,
        &[
            "uk_reconciliation_differences_object",
            "idx_reconciliation_differences_object_time",
        ],
    )
    .await
    .expect("reconciliation_differences 索引缺失");
    assert_indexes(
        db,
        <Database as IntegrationOpsExt>::RECONCILIATION_DIFFERENCE_RESOLUTIONS,
        &[
            "uk_reconciliation_difference_resolutions_no",
            "idx_reconciliation_difference_resolutions_difference",
        ],
    )
    .await
    .expect("reconciliation_difference_resolutions 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_and_read_roundtrip_covers_all_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_roundtrip").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let message = sample_message("rt");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        let task = sample_task(
            &message.base.id.clone().into(),
            "rt",
            ErrorClass::TransientFailure,
        );
        db.integration_error_tasks()
            .create(&task, &mut NoTransaction)
            .await
            .unwrap();
        let difference = sample_difference("rt");
        db.reconciliation_differences()
            .create(&difference, &mut NoTransaction)
            .await
            .unwrap();
        let resolution = sample_resolution(&difference.base.id.clone().into(), 1, ResolutionAction::Claim);
        db.reconciliation_difference_resolutions()
            .create(&resolution, &mut NoTransaction)
            .await
            .unwrap();

        let found_message = db
            .inbox_messages()
            .find_by_id(&message.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found_message, message, "消息往返一致");
        assert_eq!(found_message.source_system_id, SourceSystemId::new("sys-mall-1"));
        assert_eq!(found_message.received_at, Instant::from_unix_secs(1_700_000_002));
        assert_eq!(
            found_message.source_sent_at,
            Some(Instant::from_unix_secs(1_699_999_900))
        );
        assert_eq!(found_message.base.version, 1);

        let found_task = db
            .integration_error_tasks()
            .find_by_id(&task.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found_task, task);
        assert_eq!(found_task.status, ErrorTaskStatus::Pending);
        assert_eq!(found_task.attempt_count, 0);

        let found_difference = db
            .reconciliation_differences()
            .find_by_id(&difference.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found_difference, difference);
        assert_eq!(found_difference.business_object_id, "MO-rt");

        let found_resolution = db
            .reconciliation_difference_resolutions()
            .find_by_id(&resolution.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found_resolution, resolution);
        assert_eq!(found_resolution.resulting_status, ResultingStatus::InProgress);
    })
}

#[tokio::test]
#[ignore]
async fn message_dedup_via_unique_indexes() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_dedup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let message = sample_message("dup");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();

        let same_identity = sample_message("dup");
        let error = db
            .inbox_messages()
            .create(&same_identity, &mut NoTransaction)
            .await
            .expect_err("同 (来源系统, 来源事件) 重复接收必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let found = db
            .inbox_messages()
            .find_by_identity(&message.source_system_id, "evt-dup", &mut NoTransaction)
            .await
            .unwrap()
            .expect("消息层去重判定应命中原记录");
        assert_eq!(found.base.id, message.base.id);

        let same_fact = InboxMessage::new(
            InboxMessageId::new("msg-dup-fact"),
            InboxMessageData {
                source_event_id: "evt-dup-fact".to_string(),
                ..inbox_data_like(&message)
            },
        )
        .unwrap();
        let error = db
            .inbox_messages()
            .create(&same_fact, &mut NoTransaction)
            .await
            .expect_err("同一事实类型下相同业务事实键必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let found = db
            .inbox_messages()
            .find_by_business_fact_key(
                MessageType::PaymentSucceeded,
                &message.business_fact_key,
                &mut NoTransaction,
            )
            .await
            .unwrap()
            .expect("业务事实去重判定应命中原记录");
        assert_eq!(found.base.id, message.base.id);

        let second = sample_message("batch-2");
        db.inbox_messages()
            .create(&second, &mut NoTransaction)
            .await
            .unwrap();
        let ids = [
            InboxMessageId::new(message.base.id.clone()),
            InboxMessageId::new(second.base.id.clone()),
        ];
        let batched = db
            .inbox_messages()
            .find_messages_by_ids(&ids, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(batched.len(), 2, "$in 批量查询一次取回，禁止 N+1");
    })
}

/// 复用样本消息字段构造同业务事实键的新消息。
fn inbox_data_like(message: &InboxMessage) -> InboxMessageData {
    InboxMessageData {
        source_system_id: message.source_system_id.clone(),
        source_event_id: message.source_event_id.clone(),
        message_type: message.message_type,
        business_fact_key: message.business_fact_key.clone(),
        payload_schema_version: message.payload_schema_version.clone(),
        payload_reference: message.payload_reference.clone(),
        status: message.status,
        source_sent_at: message.source_sent_at,
        received_at: message.received_at,
        processed_at: message.processed_at,
    }
}

#[tokio::test]
#[ignore]
async fn optimistic_lock_update_success_and_stale_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut message = sample_message("opt");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        let created_at = message.base.created_at;
        let updated_before = message.base.updated_at;

        let mut stale = message.clone();
        message
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Processing),
                processed_at: None,
            })
            .unwrap();
        db.inbox_messages()
            .update(&mut message, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(message.base.version, 2, "乐观锁成功后 version 递增");
        assert_eq!(message.base.created_at, created_at, "created_at 不因更新改变");
        assert!(
            message.base.updated_at >= updated_before,
            "updated_at 应随更新刷新"
        );
        assert_eq!(message.status, InboxMessageStatus::Processing);

        stale
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::ToManual),
                processed_at: None,
            })
            .unwrap();
        let error = db
            .inbox_messages()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");

        let in_db = db
            .inbox_messages()
            .find_by_id(&message.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            in_db.status,
            InboxMessageStatus::Processing,
            "数据库状态不受失败更新影响"
        );
    })
}

#[tokio::test]
#[ignore]
async fn error_task_active_uniqueness_and_reopen_after_resolution() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_task_uniq").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let message = sample_message("task");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        let message_id = message.base.id.clone().into();

        let mut task = sample_task(&message_id, "t-1", ErrorClass::TransientFailure);
        db.integration_error_tasks()
            .create(&task, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_task(&message_id, "t-dup", ErrorClass::TransientFailure);
        let error = db
            .integration_error_tasks()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一 (消息, 错误分类) 的进行中任务必须唯一");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let different_class = sample_task(&message_id, "t-class", ErrorClass::MappingError);
        db.integration_error_tasks()
            .create(&different_class, &mut NoTransaction)
            .await
            .unwrap();

        task.transition(
            ErrorTaskStatus::Resolved,
            Some(ResolutionType::QueryConfirm),
            Some("查询确认原请求已成功".to_string()),
            Instant::from_unix_secs(1_700_000_100),
        )
        .unwrap();
        db.integration_error_tasks()
            .update(&mut task, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(task.status, ErrorTaskStatus::Resolved);

        let reopened = sample_task(&message_id, "t-reopen", ErrorClass::TransientFailure);
        db.integration_error_tasks()
            .create(&reopened, &mut NoTransaction)
            .await
            .expect("终态任务不占用唯一键，重试应可重新开单");

        let active = db
            .integration_error_tasks()
            .find_active_by_message(&message_id, ErrorClass::TransientFailure, &mut NoTransaction)
            .await
            .unwrap()
            .expect("进行中任务应可定位");
        assert_eq!(active.base.id, reopened.base.id);
        let active_mapping = db
            .integration_error_tasks()
            .find_active_by_message(&message_id, ErrorClass::MappingError, &mut NoTransaction)
            .await
            .unwrap()
            .expect("不同错误分类的进行中任务互不影响");
        assert_eq!(active_mapping.base.id, different_class.base.id);
    })
}

#[tokio::test]
#[ignore]
async fn reconciliation_unique_keys_and_append_only_history() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_diff_uniq").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let difference = sample_difference("uniq");
        db.reconciliation_differences()
            .create(&difference, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_difference("uniq");
        let error = db
            .reconciliation_differences()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一对象唯一键重复必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let found = db
            .reconciliation_differences()
            .find_by_object_key("mall_order", "MO-uniq", "amount_mismatch", &mut NoTransaction)
            .await
            .unwrap()
            .expect("唯一键查询应命中原记录");
        assert_eq!(found.base.id, difference.base.id);

        let difference_id = difference.base.id.clone().into();
        db.reconciliation_difference_resolutions()
            .create(
                &sample_resolution(&difference_id, 1, ResolutionAction::Claim),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.reconciliation_difference_resolutions()
            .create(
                &sample_resolution(&difference_id, 2, ResolutionAction::Processing),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.reconciliation_difference_resolutions()
            .create(
                &sample_resolution(&difference_id, 3, ResolutionAction::Resolved),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let duplicate_no = sample_resolution(&difference_id, 2, ResolutionAction::Processing);
        let error = db
            .reconciliation_difference_resolutions()
            .create(&duplicate_no, &mut NoTransaction)
            .await
            .expect_err("同一差异的重复处理序号必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let history = db
            .reconciliation_difference_resolutions()
            .search_resolutions(&difference_id, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(history.len(), 3, "追加历史应包含全部处理记录");
        let nos: Vec<u32> = history.iter().map(|row| row.resolution_no).collect();
        assert_eq!(nos, vec![1, 2, 3], "历史按处理序号升序");

        let latest = db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(&difference_id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("最新处理记录应存在");
        assert_eq!(latest.resolution_no, 3);
        assert_eq!(latest.resulting_status, ResultingStatus::Resolved);
    })
}

#[tokio::test]
#[ignore]
async fn fact_like_collections_never_soft_deleted() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_fact").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut message = sample_message("fact");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        message
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Processing),
                processed_at: None,
            })
            .unwrap();
        db.inbox_messages()
            .update(&mut message, &mut NoTransaction)
            .await
            .unwrap();
        let task = sample_task(&message.base.id.clone().into(), "fact", ErrorClass::RateLimited);
        db.integration_error_tasks()
            .create(&task, &mut NoTransaction)
            .await
            .unwrap();
        let difference = sample_difference("fact");
        db.reconciliation_differences()
            .create(&difference, &mut NoTransaction)
            .await
            .unwrap();
        let resolution = sample_resolution(&difference.base.id.clone().into(), 1, ResolutionAction::Claim);
        db.reconciliation_difference_resolutions()
            .create(&resolution, &mut NoTransaction)
            .await
            .unwrap();

        for entity in [
            message.base.deleted_at,
            task.base.deleted_at,
            difference.base.deleted_at,
            resolution.base.deleted_at,
        ] {
            assert_eq!(entity, 0, "事实类/不可变记录写入后 deleted_at 恒为 0");
        }
        assert!(
            db.inbox_messages()
                .find_by_id(&message.base.id, &mut NoTransaction)
                .await
                .unwrap()
                .is_some(),
            "无软删除语义下按 ID 恒可读"
        );
        assert!(db
            .reconciliation_differences()
            .find_by_id(&difference.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .is_some());
    })
}

#[tokio::test]
#[ignore]
async fn list_queries_projection_pagination_and_sort_whitelist() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut message_a = sample_message("a");
        message_a.received_at = Instant::from_unix_secs(1_700_000_000);
        let mut message_b = sample_message("b");
        message_b.received_at = Instant::from_unix_secs(1_700_000_001);
        let mut message_c = sample_message("c");
        message_c.received_at = Instant::from_unix_secs(1_700_000_002);
        message_c.message_type = MessageType::RefundSucceeded;
        message_c
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Processed),
                processed_at: Some(Instant::from_unix_secs(1_700_000_003)),
            })
            .unwrap();
        for message in [&message_a, &message_b, &message_c] {
            db.inbox_messages()
                .create(message, &mut NoTransaction)
                .await
                .unwrap();
        }

        let filter = InboxMessageFilter {
            source_system_id: None,
            message_type: None,
            status: Some(InboxMessageStatus::Received),
            source_event_id: None,
            received_at_from: None,
            received_at_to: None,
            page: 1,
            page_size: 1,
            sort_by: Some("received_at".to_string()),
            sort_ascending: true,
        };
        let page_one = db
            .inbox_messages()
            .search_inbox_messages(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page_one.total, 2, "Received 消息共两条");
        assert_eq!(page_one.items.len(), 1, "分页边界：第一页一条");
        assert_eq!(page_one.items[0].source_event_id, "evt-a", "升序首条应为最早接收");

        let page_two = InboxMessageFilter {
            page: 2,
            ..filter.clone()
        };
        let second = db
            .inbox_messages()
            .search_inbox_messages(&page_two, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(second.items.len(), 1);
        assert_eq!(second.items[0].source_event_id, "evt-b");
        let page_three = InboxMessageFilter {
            page: 3,
            ..filter.clone()
        };
        let empty = db
            .inbox_messages()
            .search_inbox_messages(&page_three, &mut NoTransaction)
            .await
            .unwrap();
        assert!(empty.items.is_empty(), "分页边界：越界页为空");
        assert_eq!(empty.total, 2, "total 与页码无关");

        let row = &page_one.items[0];
        assert_eq!(row.source_system_id, SourceSystemId::new("sys-mall-1"));
        assert_eq!(row.message_type, MessageType::PaymentSucceeded);
        assert_eq!(row.received_at, Instant::from_unix_secs(1_700_000_000));
        assert_eq!(row.status, InboxMessageStatus::Received);
        assert!(row.version >= 1);
        assert!(row.created_at > 0);

        let type_filter = InboxMessageFilter {
            message_type: Some(MessageType::RefundSucceeded),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
            ..filter.clone()
        };
        let typed = db
            .inbox_messages()
            .search_inbox_messages(&type_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(typed.total, 1);
        assert_eq!(typed.items[0].source_event_id, "evt-c");

        let regex_filter = InboxMessageFilter {
            source_event_id: Some("EVT-B".to_string()),
            status: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
            ..filter.clone()
        };
        let matched = db
            .inbox_messages()
            .search_inbox_messages(&regex_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(matched.total, 1, "来源事件 ID 字面量正则（忽略大小写）命中");
        assert_eq!(matched.items[0].source_event_id, "evt-b");

        let raw = db
            .collection::<Document>(<Database as IntegrationOpsExt>::INBOX_MESSAGES)
            .find_one(doc! { "id": &message_a.base.id })
            .await
            .unwrap()
            .expect("原始文档应存在");
        assert!(
            raw.contains_key("payload_reference"),
            "存储层保留 payload_reference，仅列表投影剔除"
        );

        let fallback = InboxMessageFilter {
            sort_by: Some("payload_schema_version".to_string()),
            status: None,
            page: 1,
            page_size: 20,
            sort_ascending: true,
            ..filter
        };
        let all = db
            .inbox_messages()
            .search_inbox_messages(&fallback, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(all.total, 3, "白名单外排序字段回退 created_at，不报错");
    })
}

#[tokio::test]
#[ignore]
async fn error_task_and_difference_lists_filter_and_project() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_task_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let message = sample_message("list");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        let message_id = message.base.id.clone().into();
        let task_ops = sample_task(&message_id, "ops", ErrorClass::TransientFailure);
        db.integration_error_tasks()
            .create(&task_ops, &mut NoTransaction)
            .await
            .unwrap();
        let mut task_manual = sample_task(&message_id, "manual", ErrorClass::MappingError);
        task_manual.status = ErrorTaskStatus::ManualRequired;
        db.integration_error_tasks()
            .create(&task_manual, &mut NoTransaction)
            .await
            .unwrap();
        let mut task_finance = sample_task(&message_id, "fin", ErrorClass::RateLimited);
        task_finance.owner_role = Some("finance".to_string());
        db.integration_error_tasks()
            .create(&task_finance, &mut NoTransaction)
            .await
            .unwrap();

        let filter = IntegrationErrorTaskFilter {
            message_id: None,
            business_object_id: None,
            error_class: Some(ErrorClass::TransientFailure),
            status: None,
            owner_role: None,
            owner_user_id: None,
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: false,
        };
        let page = db
            .integration_error_tasks()
            .search_error_tasks(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(
            page.total, 1,
            "TransientFailure 任务一条（同消息同分类唯一约束生效）"
        );
        let row = page
            .items
            .iter()
            .find(|row| row.owner_role.as_deref() == Some("ops"))
            .expect("应包含 ops 任务行");
        assert_eq!(row.error_class, ErrorClass::TransientFailure);
        assert_eq!(row.status, ErrorTaskStatus::Pending);
        assert_eq!(row.owner_role.as_deref(), Some("ops"));
        assert_eq!(row.attempt_count, 0);

        let work_queue = IntegrationErrorTaskFilter {
            error_class: None,
            status: Some(ErrorTaskStatus::ManualRequired),
            owner_role: Some("ops".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
            ..filter.clone()
        };
        let queue = db
            .integration_error_tasks()
            .search_error_tasks(&work_queue, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(queue.total, 1, "工作队列筛选：ops 的待人工任务一条");
        assert_eq!(queue.items[0].id, task_manual.base.id);

        let rate_limited = IntegrationErrorTaskFilter {
            error_class: Some(ErrorClass::RateLimited),
            status: None,
            owner_role: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
            ..filter.clone()
        };
        let finance_tasks = db
            .integration_error_tasks()
            .search_error_tasks(&rate_limited, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(finance_tasks.total, 1);
        assert_eq!(finance_tasks.items[0].owner_role.as_deref(), Some("finance"));

        let difference_a = sample_difference("la");
        db.reconciliation_differences()
            .create(&difference_a, &mut NoTransaction)
            .await
            .unwrap();
        let difference_b = sample_difference("lb");
        db.reconciliation_differences()
            .create(&difference_b, &mut NoTransaction)
            .await
            .unwrap();
        let supplier_difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new("diff-sup"),
            ReconciliationDifferenceData {
                business_object_type: "supplier_order".to_string(),
                business_object_id: "PO-sup".to_string(),
                difference_type: "amount_mismatch".to_string(),
                left_fact_reference: Some("mall_order_fact://f-sup".to_string()),
                right_fact_reference: Some("invoice://inv-sup".to_string()),
            },
        )
        .unwrap();
        db.reconciliation_differences()
            .create(&supplier_difference, &mut NoTransaction)
            .await
            .unwrap();

        let difference_filter = ReconciliationDifferenceFilter {
            business_object_type: Some("mall_order".to_string()),
            business_object_id: None,
            difference_type: None,
            created_at_from: None,
            created_at_to: None,
            page: 1,
            page_size: 1,
            sort_by: None,
            sort_ascending: false,
        };
        let first = db
            .reconciliation_differences()
            .search_differences(&difference_filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(first.total, 2, "mall_order 差异两条");
        assert_eq!(first.items.len(), 1, "分页边界：第一页一条");
        assert_eq!(first.items[0].business_object_type, "mall_order");
        assert!(
            first.items[0].left_fact_reference.is_some(),
            "证据引用应进入列表投影"
        );
        let second = ReconciliationDifferenceFilter {
            page: 2,
            ..difference_filter
        };
        let second_page = db
            .reconciliation_differences()
            .search_differences(&second, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(second_page.items.len(), 1, "分页边界：第二页一条");
        assert!(second_page.items[0].business_object_id.starts_with("MO-l"));
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut message = sample_message("tx-commit");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        let task = sample_task(
            &message.base.id.clone().into(),
            "tx-commit",
            ErrorClass::BusinessRejected,
        );
        message
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Failed),
                processed_at: None,
            })
            .unwrap();

        let db_clone = db.clone();
        let task_for_tx = task.clone();
        let mut message_for_tx = message.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .integration_ops()
                        .create_error_task_with_message_failure(&task_for_tx, &mut message_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let task_found = db
            .integration_error_tasks()
            .find_by_id(&task.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(task_found.is_some(), "事务提交后错误任务必须可见");
        let message_found = db
            .inbox_messages()
            .find_by_id(&message.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("消息应存在");
        assert_eq!(
            message_found.status,
            InboxMessageStatus::Failed,
            "事务提交后消息必须置为失败"
        );
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_conflict_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut message = sample_message("tx-conflict");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        let mut stale = message.clone();
        message
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Processing),
                processed_at: None,
            })
            .unwrap();
        db.inbox_messages()
            .update(&mut message, &mut NoTransaction)
            .await
            .unwrap();
        stale
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Failed),
                processed_at: None,
            })
            .unwrap();
        let task = sample_task(
            &message.base.id.clone().into(),
            "tx-conflict",
            ErrorClass::BusinessRejected,
        );

        let db_clone = db.clone();
        let task_for_tx = task.clone();
        let mut stale_for_tx = stale.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .integration_ops()
                        .create_error_task_with_message_failure(&task_for_tx, &mut stale_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::OptimisticLockingError)),
            "陈旧消息版本必须使事务失败，实际为 {result:?}"
        );

        let task_found = db
            .integration_error_tasks()
            .find_by_id(&task.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(task_found.is_none(), "回滚后错误任务不得残留");
        let message_found = db
            .inbox_messages()
            .find_by_id(&message.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            message_found.status,
            InboxMessageStatus::Processing,
            "回滚后消息状态保持事务前"
        );
        assert_eq!(message_found.base.version, 2);
    })
}

#[tokio::test]
#[ignore]
async fn multi_step_without_transaction_commits_steps_independently() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut message = sample_message("notx");
        db.inbox_messages()
            .create(&message, &mut NoTransaction)
            .await
            .unwrap();
        let mut stale = message.clone();
        message
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Processing),
                processed_at: None,
            })
            .unwrap();
        db.inbox_messages()
            .update(&mut message, &mut NoTransaction)
            .await
            .unwrap();
        stale
            .update(InboxMessageUpdate {
                status: Some(InboxMessageStatus::Failed),
                processed_at: None,
            })
            .unwrap();
        let task = sample_task(
            &message.base.id.clone().into(),
            "notx",
            ErrorClass::BusinessRejected,
        );

        let error = db
            .integration_ops()
            .create_error_task_with_message_failure(&task, &mut stale, &mut NoTransaction)
            .await
            .expect_err("无事务时消息 CAS 失败仍返回错误");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );

        let task_found = db
            .integration_error_tasks()
            .find_by_id(&task.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            task_found.is_some(),
            "NoTransaction 下两笔写入各自自动提交：任务已独立落库（文档化的可预期半成品）"
        );
        let message_found = db
            .inbox_messages()
            .find_by_id(&message.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            message_found.status,
            InboxMessageStatus::Processing,
            "消息更新未命中 CAS，保持数据库状态"
        );
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("intops_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let difference = sample_difference("tx-abort");
        db.reconciliation_differences()
            .create(&difference, &mut NoTransaction)
            .await
            .unwrap();
        let difference_id = difference.base.id.clone().into();
        let resolution = sample_resolution(&difference_id, 1, ResolutionAction::Claim);
        let message = sample_message("tx-abort");
        let task = sample_task(
            &message.base.id.clone().into(),
            "tx-abort",
            ErrorClass::RateLimited,
        );

        let db_clone = db.clone();
        let resolution_for_tx = resolution.clone();
        let task_for_tx = task.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .reconciliation_difference_resolutions()
                        .create(&resolution_for_tx, session)
                        .await?;
                    db_clone
                        .integration_error_tasks()
                        .create(&task_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let resolution_found = db
            .reconciliation_difference_resolutions()
            .find_by_id(&resolution.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(resolution_found.is_none(), "回滚后解决记录不得残留");
        let task_found = db
            .integration_error_tasks()
            .find_by_id(&task.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(task_found.is_none(), "回滚后错误任务不得残留");
    })
}
