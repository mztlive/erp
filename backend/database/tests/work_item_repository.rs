//! 域 D03 `work_item` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test work_item_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::WorkItemExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::common::time::Instant;
use entities::work_item::{WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 待办列表筛选条件类型（经 `WorkItemExt` 关联类型跨 crate 可达）。
type WorkItemFilter = <Database as WorkItemExt>::WorkItemFilter;

/// 构造可复用的待办实体。
fn sample_work_item(id: &str, object_id: &str) -> WorkItem {
    WorkItem::new(
        entities::ids::WorkItemId::new(id),
        WorkItemData {
            work_item_type: WorkItemType::ImportBusinessConfirmation,
            business_object_type: "LEGACY_IMPORT_BATCH".to_string(),
            business_object_id: object_id.to_string(),
            subject_version: Some("v3".to_string()),
            owner_role: Some("sales".to_string()),
            owner_user_id: None,
            priority: WorkItemPriority::Normal,
            due_at: Some(Instant::from_unix_secs(1_700_086_400)),
            reason_code: Some("IMPORT_READY".to_string()),
            impact_summary: Some("待确认导入范围".to_string()),
            completion_action: "COMPLETE_IMPORT_BUSINESS_CONFIRMATION".to_string(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as WorkItemExt>::WORK_ITEMS,
        &["uk_work_items_active", "idx_work_items_queue"],
    )
    .await
    .expect("work_items 索引缺失");
}

#[tokio::test]
#[ignore]
async fn create_read_update_roundtrip_with_optimistic_lock() {
    require_mongo!(async {
        let test_db = TestDb::new("workitem_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let item = sample_work_item("wi-1", "batch-1");
        db.work_items().create(&item, &mut NoTransaction).await.unwrap();
        assert_eq!(item.base.version, 1);

        let found = db
            .work_items()
            .find_by_id(&item.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .expect("创建后应可读回");
        assert_eq!(found.business_object_type, "LEGACY_IMPORT_BATCH");
        assert_eq!(found.status, WorkItemStatus::Unclaimed);
        assert_eq!(found.due_at, Some(Instant::from_unix_secs(1_700_086_400)));

        let mut claimed = found.clone();
        claimed.claim("user-1").expect("实体层领取迁移应成功");
        db.work_items()
            .claim(&mut claimed, &mut NoTransaction)
            .await
            .unwrap();
        claimed
            .complete("user-1", Instant::from_unix_secs(1_700_100_000))
            .unwrap();
        db.work_items()
            .update(&mut claimed, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(claimed.base.version, 3, "领取 + 更新后 version 递增");
        assert_eq!(claimed.status, WorkItemStatus::Completed);
        assert_eq!(claimed.owner_user_id.as_deref(), Some("user-1"));
    })
}

#[tokio::test]
#[ignore]
async fn claim_is_atomic_and_stale_claim_returns_optimistic_locking_error() {
    require_mongo!(async {
        let test_db = TestDb::new("workitem_claim").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut item = sample_work_item("wi-1", "batch-1");
        db.work_items().create(&item, &mut NoTransaction).await.unwrap();
        let mut stale = item.clone();
        let mut concurrent = item.clone();

        item.claim("alice").unwrap();
        db.work_items()
            .claim(&mut item, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(item.base.version, 2);
        assert_eq!(item.status, WorkItemStatus::InProgress);

        let error = db
            .work_items()
            .claim(&mut concurrent, &mut NoTransaction)
            .await
            .expect_err("并发领取必须被条件更新拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );

        stale.claim("bob").unwrap();
        let stale_error = db
            .work_items()
            .claim(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 领取必须被条件更新拒绝");
        assert!(
            matches!(stale_error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {stale_error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");

        let winner = db
            .work_items()
            .find_by_id(&item.base.id, &mut NoTransaction)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(winner.owner_user_id.as_deref(), Some("alice"));
    })
}

#[tokio::test]
#[ignore]
async fn partial_unique_index_allows_new_task_after_terminal_state() {
    require_mongo!(async {
        let test_db = TestDb::new("workitem_uniq").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut first = sample_work_item("wi-1", "batch-1");
        db.work_items().create(&first, &mut NoTransaction).await.unwrap();

        let duplicate = sample_work_item("wi-2", "batch-1");
        let error = db
            .work_items()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同一业务对象同一任务类型同时两个有效任务必须被拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        first.claim("alice").unwrap();
        db.work_items()
            .claim(&mut first, &mut NoTransaction)
            .await
            .unwrap();
        first
            .complete("alice", Instant::from_unix_secs(1_700_100_000))
            .unwrap();
        db.work_items()
            .update(&mut first, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(first.status, WorkItemStatus::Completed);

        let reopened = sample_work_item("wi-3", "batch-1");
        db.work_items()
            .create(&reopened, &mut NoTransaction)
            .await
            .expect("历史任务进入终态后允许重新派发");
        let active = db
            .work_items()
            .list_active_by_object("LEGACY_IMPORT_BATCH", "batch-1", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(active.len(), 1, "有效任务只有一个");
        assert_eq!(active[0].base.id, "wi-3");
    })
}

#[tokio::test]
#[ignore]
async fn soft_delete_and_restore_match_deleted_state() {
    require_mongo!(async {
        let test_db = TestDb::new("workitem_soft").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut item = sample_work_item("wi-1", "batch-1");
        db.work_items().create(&item, &mut NoTransaction).await.unwrap();

        db.work_items()
            .soft_delete(&mut item, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .work_items()
            .find_by_id(&item.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        db.work_items()
            .restore(&mut item, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .work_items()
            .find_by_id(&item.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");

        db.work_items()
            .restore(&mut item, &mut NoTransaction)
            .await
            .expect_err("未删除实体不可重复恢复");
    })
}

#[tokio::test]
#[ignore]
async fn search_respects_pagination_boundary_sort_whitelist_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("workitem_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.work_items()
            .create(&sample_work_item("wi-1", "batch-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.work_items()
            .create(&sample_work_item("wi-2", "batch-2"), &mut NoTransaction)
            .await
            .unwrap();
        db.work_items()
            .create(&sample_work_item("wi-3", "batch-3"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = WorkItemFilter {
            work_item_type: Some(WorkItemType::ImportBusinessConfirmation),
            status: Some(WorkItemStatus::Unclaimed),
            owner_role: Some("sales".to_string()),
            owner_user_id: None,
            priority: None,
            page: 1,
            page_size: 2,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .work_items()
            .search_work_items(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 3, "筛选条件命中三条");
        assert_eq!(page.items.len(), 2, "第一页两条");
        let row = &page.items[0];
        assert!(matches!(row.id.as_str(), "wi-1" | "wi-2" | "wi-3"));
        assert_eq!(row.work_item_type, WorkItemType::ImportBusinessConfirmation);
        assert_eq!(row.business_object_type, "LEGACY_IMPORT_BATCH");
        assert_eq!(row.status, WorkItemStatus::Unclaimed);
        assert_eq!(row.priority, WorkItemPriority::Normal);
        assert_eq!(row.due_at, Some(1_700_086_400));
        assert!(row.version >= 1);

        let second = WorkItemFilter {
            page: 2,
            page_size: 2,
            ..filter.clone()
        };
        let page_two = db
            .work_items()
            .search_work_items(&second, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page_two.items.len(), 1, "第二页一条");
        assert!(
            !page.items.iter().any(|item| item.id == page_two.items[0].id),
            "同一秒创建的任务顺序不确定，两页不得重叠"
        );

        let unsorted = WorkItemFilter {
            sort_by: Some("business_object_id".to_string()),
            sort_ascending: false,
            ..filter.clone()
        };
        let fallback = db
            .work_items()
            .search_work_items(&unsorted, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(fallback.total, 3, "白名单外排序字段回落默认排序，不报错");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_participation_rolls_back_both_writes() {
    require_mongo!(async {
        let test_db = TestDb::new("workitem_tx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let first = sample_work_item("wi-1", "batch-1");
        let second = sample_work_item("wi-2", "batch-2");

        let db_clone = db.clone();
        let first_for_tx = first.clone();
        let second_for_tx = second.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone.work_items().create(&first_for_tx, session).await?;
                    db_clone.work_items().create(&second_for_tx, session).await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let first_found = db
            .work_items()
            .find_by_id(&first.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(first_found.is_none(), "回滚后第一笔不得残留");
        let second_found = db
            .work_items()
            .find_by_id(&second.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(second_found.is_none(), "回滚后第二笔不得残留");

        let db_clone = db.clone();
        let first_for_tx = first.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone.work_items().create(&first_for_tx, session).await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");
        let committed = db
            .work_items()
            .find_by_id(&first.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(committed.is_some(), "事务提交后写入可见");
    })
}
