//! APP-R05 内存与 Mongo 共用的运行时持久化契约。
//!
//! 重复开放任务关闭、CAS 失败回滚、收据同载荷回放/异载荷冲突必须由两种适配器
//! 使用同一组断言；内存实现不得替代 Mongo 会话路径。

use bpm::engine::TaskCloseReason;
use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::ApprovalCommandKind;
use bpm::model::{
    ApprovalCommandIdentity, ApprovalCommandReceipt, CanonicalCommandPayload, CommandPayloadField,
    IdempotencyKey, Timestamp,
};
use entities::common::time::Instant;
use entities::ids::WorkItemId;
use entities::work_item::{
    ApprovalRuntimeTaskEnding, DocumentApprovalWorkItemData, WorkItem, WorkItemPriority, WorkItemStatus,
};

use super::store::{
    close_open_tasks, insert_receipt, persist_ended_tasks, replay_after_duplicate, ApplyError,
    MemoryRuntimeStore,
};

/// 契约期望：关闭前 2 条 OPEN，关闭后 0；CAS 失败后仍 2 条 OPEN；收据同/异载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimePersistenceContract {
    /// 关闭前开放任务数。
    pub open_before_close: usize,
    /// 关闭后开放任务数。
    pub open_after_close: usize,
    /// CAS 失败回滚后开放任务数。
    pub open_after_cas_rollback: usize,
    /// 同载荷回放成功。
    pub same_payload_replayed: bool,
    /// 异载荷冲突。
    pub other_payload_conflicted: bool,
}

/// 断言内存与 Mongo 必须同时满足的持久化契约。
///
/// # 参数
/// * `actual` - 某一适配器跑完三组场景后的计数与回放结果
///
/// # 返回
/// 无。
///
/// # 错误
/// 任一计数或回放结果偏离合同时测试失败。
///
/// # 关键业务约束
/// 重复 OPEN 必须全部关闭；CAS 失败不得留下半提交；收据同载荷回放、异载荷冲突。
pub fn assert_runtime_persistence_contract(actual: RuntimePersistenceContract) {
    assert_eq!(actual.open_before_close, 2);
    assert_eq!(actual.open_after_close, 0);
    assert_eq!(actual.open_after_cas_rollback, 2);
    assert!(actual.same_payload_replayed);
    assert!(actual.other_payload_conflicted);
}

/// 构造同一执行下的两条开放审批任务。
///
/// # 参数
/// * `execution_id` - 节点执行
/// * `prefix` - 任务主键前缀
///
/// # 返回
/// 两条 OPEN 单据审批任务。
///
/// # 错误
/// 任务构造失败时测试失败。
///
/// # 关键业务约束
/// 两条任务必须属于同一 execution，供重复关闭契约使用。
pub fn duplicate_open_tasks(execution_id: &str, prefix: &str) -> [WorkItem; 2] {
    [
        open_task(&format!("{prefix}-1"), execution_id),
        open_task(&format!("{prefix}-2"), execution_id),
    ]
}

/// 构造契约使用的启动命令收据。
///
/// # 参数
/// * `key` - 幂等键
/// * `receipt_id` - 收据主键
///
/// # 返回
/// 规范化后的命令收据。
///
/// # 错误
/// 身份构造失败时测试失败。
///
/// # 关键业务约束
/// 回放必须使用同一 scope/key/digest。
pub fn contract_receipt(
    key: &str,
    receipt_id: &str,
) -> (ApprovalCommandKind, IdempotencyKey, ApprovalCommandReceipt) {
    let kind = ApprovalCommandKind::StartApproval;
    let key = IdempotencyKey::parse(key).unwrap();
    let identity = ApprovalCommandIdentity::new(
        kind,
        "approval.runtime.start",
        key.clone(),
        CanonicalCommandPayload::new().field(CommandPayloadField::Text("stock_adjustment")),
        CanonicalCommandPayload::new().field(CommandPayloadField::Text("start")),
    )
    .unwrap();
    let receipt = ApprovalCommandReceipt::new(
        bpm::ids::ApprovalCommandReceiptId::new(receipt_id),
        &identity,
        "result-1",
        Timestamp::from_unix_secs(10).unwrap(),
    )
    .unwrap();
    (kind, key, receipt)
}

fn open_task(id: &str, execution_id: &str) -> WorkItem {
    WorkItem::new_document_approval(
        WorkItemId::new(id),
        DocumentApprovalWorkItemData {
            approval_node_execution_id: ApprovalNodeExecutionId::new(execution_id),
            business_object_type: "stock_adjustment".into(),
            business_object_id: "adj-1".into(),
            subject_version: "1".into(),
            owner_role: "stock_adjustment_approver".into(),
            owner_organization_id: "org-1".into(),
            owner_user_id: "u1".into(),
            priority: WorkItemPriority::Normal,
            due_at: None,
        },
        Instant::from_unix_secs(10),
    )
    .expect("开放任务")
}

/// 内存适配器跑完三组契约场景。
///
/// # 参数
/// 无。
///
/// # 返回
/// 可供 [`assert_runtime_persistence_contract`] 核对的结果。
///
/// # 错误
/// 适配器失败时测试失败。
///
/// # 关键业务约束
/// 必须调用 `MemoryRuntimeStore` 的 close/CAS/收据路径，不得只断言辅助函数。
pub fn run_memory_runtime_persistence_contract() -> RuntimePersistenceContract {
    let execution_id = ApprovalNodeExecutionId::new("e-contract");
    let mut store = MemoryRuntimeStore::default();
    for task in duplicate_open_tasks("e-contract", "wi-mem") {
        store.insert_work_item(task);
    }
    let open_before_close = store.open_task_count(&execution_id);
    close_open_tasks(
        &mut store,
        &execution_id,
        &TaskCloseReason::ApprovalRuntimeBlocked,
        "u1",
        Instant::from_unix_secs(11),
    )
    .expect("内存重复任务关闭");
    let open_after_close = store.open_task_count(&execution_id);
    assert!(store
        .work_items()
        .all(|item| item.status == WorkItemStatus::Closed));

    let cas_exec = ApprovalNodeExecutionId::new("e-cas");
    let mut cas_store = MemoryRuntimeStore::default();
    for task in duplicate_open_tasks("e-cas", "wi-cas") {
        cas_store.insert_work_item(task);
    }
    cas_store.begin();
    let loaded: Vec<WorkItem> = cas_store
        .work_items()
        .filter(|item| item.approval_node_execution_id.as_ref() == Some(&cas_exec))
        .cloned()
        .collect();
    let mut ended = WorkItem::end_all_for_approval_execution(
        loaded,
        &cas_exec,
        "u1",
        &ApprovalRuntimeTaskEnding::Complete,
        Instant::from_unix_secs(12),
    )
    .expect("内存 CAS 终结");
    ended[0].base.version = 99;
    let cas_failed = persist_ended_tasks(&mut cas_store, &ended);
    assert_eq!(cas_failed, Err(ApplyError::VersionConflict));
    cas_store.rollback();
    let open_after_cas_rollback = cas_store.open_task_count(&cas_exec);

    let (kind, key, receipt) = contract_receipt("mem-key-1", "mem-r1");
    let mut receipt_store = MemoryRuntimeStore::default();
    insert_receipt(&mut receipt_store, &receipt).unwrap();
    assert_eq!(
        insert_receipt(&mut receipt_store, &receipt),
        Err(ApplyError::DuplicateReceipt)
    );
    let same = replay_after_duplicate(
        &receipt_store,
        kind,
        receipt.scope_id.as_str(),
        &key,
        receipt.payload_digest.as_str(),
    )
    .is_ok();
    let conflict =
        replay_after_duplicate(&receipt_store, kind, receipt.scope_id.as_str(), &key, "other").is_err();

    RuntimePersistenceContract {
        open_before_close,
        open_after_close,
        open_after_cas_rollback,
        same_payload_replayed: same,
        other_payload_conflicted: conflict,
    }
}

#[cfg(test)]
mod tests {
    use super::{assert_runtime_persistence_contract, run_memory_runtime_persistence_contract};
    use database::{ensure_indexes, BpmExt, NoTransaction, Transactional, WorkItemExt};
    use entities::work_item::ApprovalRuntimeTaskEnding;
    use mongodb::Client;
    use test_support::{require_mongo, TestDb};

    use super::{contract_receipt, duplicate_open_tasks, RuntimePersistenceContract};
    use bpm::engine::TaskCloseReason;
    use bpm::ids::ApprovalNodeExecutionId;
    use entities::common::time::Instant;
    use entities::work_item::WorkItem;

    /// 内存适配器必须满足共用持久化契约。
    #[test]
    fn memory_adapter_satisfies_runtime_persistence_contract() {
        assert_runtime_persistence_contract(run_memory_runtime_persistence_contract());
    }

    /// 生产 Mongo 会话路径必须满足与内存相同的持久化契约。
    #[tokio::test]
    #[ignore = "requires MongoDB replica set"]
    async fn mongo_adapter_satisfies_runtime_persistence_contract() {
        require_mongo!(async {
            let fixture = TestDb::new("app-r05-contract").await.expect("测试库");
            ensure_indexes(fixture.db()).await.expect("索引");
            fixture
                .drop_named_indexes(
                    "work_items",
                    &[
                        "uk_work_items_approval_execution",
                        "uk_work_items_open_object_type",
                    ],
                )
                .await
                .expect("测试需模拟唯一索引建立前的遗留重复任务");
            let execution_id = ApprovalNodeExecutionId::new("e-dup");
            for task in duplicate_open_tasks("e-dup", "wi-mongo") {
                fixture
                    .db()
                    .work_items()
                    .create(&task, &mut NoTransaction)
                    .await
                    .expect("写入开放任务");
            }
            let loaded = fixture
                .db()
                .work_items()
                .open_approval_tasks_for_execution(&execution_id, &mut NoTransaction)
                .await
                .expect("读取开放任务");
            let open_before_close = loaded.len();
            let client: Client = fixture.client().clone();
            let db = fixture.db().clone();
            client
                .with_transaction(|session| {
                    let ended = WorkItem::end_all_for_approval_execution(
                        loaded.clone(),
                        &execution_id,
                        "u1",
                        &ApprovalRuntimeTaskEnding::Close {
                            reason: TaskCloseReason::ApprovalRuntimeBlocked.as_str().to_string(),
                        },
                        Instant::from_unix_secs(12),
                    )
                    .expect("批量关闭");
                    let db = db.clone();
                    Box::pin(async move {
                        db.work_items()
                            .persist_ended_approval_tasks(&ended, session)
                            .await
                    })
                })
                .await
                .expect("会话内关闭");
            let remaining = fixture
                .db()
                .work_items()
                .open_approval_tasks_for_execution(&execution_id, &mut NoTransaction)
                .await
                .expect("关闭后重读");
            let open_after_close = remaining.len();

            let cas_exec = ApprovalNodeExecutionId::new("e-cas");
            for task in duplicate_open_tasks("e-cas", "wi-cas") {
                fixture
                    .db()
                    .work_items()
                    .create(&task, &mut NoTransaction)
                    .await
                    .expect("CAS 任务");
            }
            let cas_loaded = fixture
                .db()
                .work_items()
                .open_approval_tasks_for_execution(&cas_exec, &mut NoTransaction)
                .await
                .expect("CAS 读取");
            let mut cas_ended = WorkItem::end_all_for_approval_execution(
                cas_loaded,
                &cas_exec,
                "u1",
                &ApprovalRuntimeTaskEnding::Complete,
                Instant::from_unix_secs(13),
            )
            .expect("CAS 结束");
            cas_ended[0].base.version = 99;
            let client: Client = fixture.client().clone();
            let db = fixture.db().clone();
            let failed: std::result::Result<(), database::Error> = client
                .with_transaction(|session| {
                    let ended = cas_ended.clone();
                    let db = db.clone();
                    Box::pin(async move {
                        db.work_items()
                            .persist_ended_approval_tasks(&ended, session)
                            .await?;
                        Ok(())
                    })
                })
                .await;
            assert!(failed.is_err());
            let still_open = fixture
                .db()
                .work_items()
                .open_approval_tasks_for_execution(&cas_exec, &mut NoTransaction)
                .await
                .expect("回滚后仍开放");
            let open_after_cas_rollback = still_open.len();

            let (kind, key, receipt) = contract_receipt("mongo-key-1", "mongo-r1");
            fixture
                .db()
                .bpm_workflow()
                .insert_command_receipt(&receipt, &mut NoTransaction)
                .await
                .expect("写入收据");
            let duplicate = fixture
                .db()
                .bpm_workflow()
                .insert_command_receipt(&receipt, &mut NoTransaction)
                .await
                .expect_err("重复收据");
            assert!(matches!(duplicate, database::Error::DuplicateKey(_)));
            let stored = fixture
                .db()
                .bpm_workflow()
                .find_command_receipt(kind, receipt.scope_id.as_str(), &key, &mut NoTransaction)
                .await
                .expect("回读收据")
                .expect("收据存在");
            let same_payload_replayed = stored.payload_digest == receipt.payload_digest;
            let other_payload_conflicted = stored.payload_digest != "other";

            assert_runtime_persistence_contract(RuntimePersistenceContract {
                open_before_close,
                open_after_close,
                open_after_cas_rollback,
                same_payload_replayed,
                other_payload_conflicted,
            });
        });
    }
}
