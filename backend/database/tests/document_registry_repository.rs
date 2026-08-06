//! 域 D02 `document_registry` 仓储集成测试（真实 MongoDB 单节点副本集）。
//!
//! 统一 `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（test-support `require_mongo!`）：
//! 无数据库环境 `cargo test` 跳过；CI 与验收执行
//! `cargo test -p database --test document_registry_repository -- --include-ignored`。
//! 每个测试使用独立随机数据库名，结束自动 drop（TestDb）。

use database::repository::extensions::DocumentRegistryExt;
use database::{ensure_indexes, NoTransaction, Transactional};
use entities::document_registry::{
    BusinessDocument, BusinessDocumentData, BusinessDocumentId, DocumentParticipant, DocumentParticipantData,
    DocumentRelation, DocumentRelationData, DocumentRelationType, DocumentType, ParticipantRole,
    WorkflowAction, WorkflowActionData, WorkflowActionType,
};
use mongodb::Database;
use test_support::{assert_indexes, require_mongo, TestDb};

/// 单据注册列表筛选条件类型（经 `DocumentRegistryExt` 关联类型跨 crate 可达）。
type BusinessDocumentFilter = <Database as DocumentRegistryExt>::BusinessDocumentFilter;
/// 工作流动作列表筛选条件类型。
type WorkflowActionFilter = <Database as DocumentRegistryExt>::WorkflowActionFilter;

/// 构造可复用的单据注册实体。
fn sample_document(id: &str, document_no: &str) -> BusinessDocument {
    BusinessDocument::new(
        BusinessDocumentId::new(id),
        BusinessDocumentData {
            document_type: DocumentType::SalesOrder,
            document_no: document_no.to_string(),
        },
    )
    .unwrap()
}

/// 构造可复用的工作流动作实体。
fn sample_action(id: &str, document_id: &str) -> WorkflowAction {
    WorkflowAction::new(
        entities::document_registry::WorkflowActionId::new(id),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(document_id),
            action_type: WorkflowActionType::Approve,
            from_status: "PENDING_REVIEW".to_string(),
            to_status: "EFFECTIVE".to_string(),
            actor_id: "user-1".to_string(),
            actor_role: "sales-manager".to_string(),
            comment: Some("同意".to_string()),
        },
    )
    .unwrap()
}

/// 构造可复用的单据关系实体。
fn sample_relation(id: &str, from: &str, to: &str) -> DocumentRelation {
    DocumentRelation::new(
        entities::document_registry::DocumentRelationId::new(id),
        DocumentRelationData {
            from_document_id: BusinessDocumentId::new(from),
            to_document_id: BusinessDocumentId::new(to),
            relation_type: DocumentRelationType::Changes,
        },
    )
    .unwrap()
}

/// 构造可复用的参与人记录。
fn sample_participant(id: &str, document_id: &str, user_id: &str) -> DocumentParticipant {
    DocumentParticipant::new(
        entities::document_registry::DocumentParticipantId::new(id),
        DocumentParticipantData {
            document_id: BusinessDocumentId::new(document_id),
            participant_role: ParticipantRole::OwnerSales,
            participant_user_id: user_id.to_string(),
            participant_name: "张三".to_string(),
            recorded_by: "admin-1".to_string(),
        },
    )
    .unwrap()
}

/// 执行 `ensure_indexes` 并断言本域全部必需索引就位。
async fn assert_domain_indexes(db: &Database) {
    ensure_indexes(db).await.expect("ensure_indexes 应成功");
    assert_indexes(
        db,
        <Database as DocumentRegistryExt>::BUSINESS_DOCUMENTS,
        &["uk_business_documents_identity", "idx_business_documents_no"],
    )
    .await
    .expect("business_documents 索引缺失");
    assert_indexes(
        db,
        <Database as DocumentRegistryExt>::DOCUMENT_RELATIONS,
        &["uk_document_relations_link", "idx_document_relations_reverse"],
    )
    .await
    .expect("document_relations 索引缺失");
    assert_indexes(
        db,
        <Database as DocumentRegistryExt>::DOCUMENT_PARTICIPANTS,
        &[
            "uk_document_participants_id",
            "idx_document_participants_document_user",
            "idx_document_participants_user",
        ],
    )
    .await
    .expect("document_participants 索引缺失");
    assert_indexes(
        db,
        <Database as DocumentRegistryExt>::WORKFLOW_ACTIONS,
        &[
            "uk_workflow_actions_id",
            "idx_workflow_actions_document_created",
            "idx_workflow_actions_actor_created",
        ],
    )
    .await
    .expect("workflow_actions 索引缺失");
}

#[tokio::test]
#[ignore]
async fn register_roundtrip_and_idempotent_same_identity() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_crud").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let doc = sample_document("bd-1", "SO-2025-001");
        let outcome = db
            .business_documents()
            .register(&doc, &mut NoTransaction)
            .await
            .unwrap();
        assert!(outcome.is_none(), "首次注册写入新行");

        let found = db
            .business_documents()
            .find_by_type_and_no(DocumentType::SalesOrder, "SO-2025-001", &mut NoTransaction)
            .await
            .unwrap()
            .expect("注册后应按身份读回");
        assert_eq!(found.base.id, "bd-1");
        assert_eq!(found.document_no, "SO-2025-001");
        assert_eq!(found.document_type, DocumentType::SalesOrder);

        let idempotent = sample_document("bd-1", "SO-2025-001");
        let again = db
            .business_documents()
            .register(&idempotent, &mut NoTransaction)
            .await
            .unwrap()
            .expect("同身份同 ID 幂等命中");
        assert_eq!(again.base.id, "bd-1");
        assert_eq!(again.base.version, 1, "幂等命中不得产生第二行");

        let count = db
            .business_documents()
            .search_business_documents(
                &BusinessDocumentFilter {
                    document_type: Some(DocumentType::SalesOrder),
                    document_no: None,
                    page: 1,
                    page_size: 20,
                    sort_by: None,
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(count.total, 1, "重复注册不产生第二条");
    })
}

#[tokio::test]
#[ignore]
async fn duplicate_identity_with_different_id_surfaces_duplicate_key() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_dup").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let doc = sample_document("bd-1", "SO-2025-001");
        db.business_documents()
            .register(&doc, &mut NoTransaction)
            .await
            .unwrap();

        let duplicate = sample_document("bd-other", "SO-2025-001");
        let error = db
            .business_documents()
            .register(&duplicate, &mut NoTransaction)
            .await
            .expect_err("同身份不同 ID 必须被唯一索引拒绝");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );
    })
}

#[tokio::test]
#[ignore]
async fn soft_delete_keeps_identity_and_restore_recovers() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_soft").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut doc = sample_document("bd-1", "SO-2025-001");
        db.business_documents()
            .register(&doc, &mut NoTransaction)
            .await
            .unwrap();

        db.business_documents()
            .soft_delete(&mut doc, &mut NoTransaction)
            .await
            .unwrap();
        let after_delete = db
            .business_documents()
            .find_by_id(&doc.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_delete.is_none(), "软删除后按 ID 不可见");

        let rebind = sample_document("bd-other", "SO-2025-001");
        let error = db
            .business_documents()
            .register(&rebind, &mut NoTransaction)
            .await
            .expect_err("软删除后身份仍被占用，不得复用");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "软删除身份复用必须返回 DuplicateKey，实际为 {error:?}"
        );

        db.business_documents()
            .restore(&mut doc, &mut NoTransaction)
            .await
            .unwrap();
        let after_restore = db
            .business_documents()
            .find_by_id(&doc.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(after_restore.is_some(), "恢复后按 ID 重新可见");
    })
}

#[tokio::test]
#[ignore]
async fn stale_version_update_returns_optimistic_locking_error() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_optlock").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let mut doc = sample_document("bd-1", "SO-2025-001");
        db.business_documents()
            .register(&doc, &mut NoTransaction)
            .await
            .unwrap();
        let mut stale = doc.clone();

        doc.formalize(entities::common::time::Instant::from_unix_secs(1_700_000_000));
        db.business_documents()
            .update(&mut doc, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(doc.base.version, 2, "乐观锁成功后 version 递增");

        let error = db
            .business_documents()
            .update(&mut stale, &mut NoTransaction)
            .await
            .expect_err("陈旧 version 更新必须被 CAS 拒绝");
        assert!(
            matches!(error, database::Error::OptimisticLockingError),
            "期望 OptimisticLockingError，实际为 {error:?}"
        );
        assert_eq!(stale.base.version, 1, "CAS 失败不得改动内存版本");
    })
}

#[tokio::test]
#[ignore]
async fn relation_and_participant_lookups_respect_identity() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_rel").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.document_relations()
            .create(
                &sample_relation("rel-1", "change-1", "order-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.document_relations()
            .create(
                &sample_relation("rel-2", "change-2", "order-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();

        let incoming = db
            .document_relations()
            .list_by_to_document(&BusinessDocumentId::new("order-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(incoming.len(), 2, "反向查询命中两条");
        let outgoing = db
            .document_relations()
            .list_by_from_document(&BusinessDocumentId::new("change-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(outgoing.len(), 1);

        let duplicate = sample_relation("rel-3", "change-2", "order-1");
        let error = db
            .document_relations()
            .create(&duplicate, &mut NoTransaction)
            .await
            .expect_err("相同 (from, to, type) 关系必须被唯一索引拒绝");
        assert!(matches!(error, database::Error::DuplicateKey(_)));

        db.document_participants()
            .create(
                &sample_participant("dp-1", "order-1", "user-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        db.document_participants()
            .create(
                &sample_participant("dp-2", "order-2", "user-1"),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        let by_user = db
            .document_participants()
            .list_by_user("user-1", &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(by_user.len(), 2, "参与人维度命中两条");
        let by_doc = db
            .document_participants()
            .find_by_document_and_user(&BusinessDocumentId::new("order-1"), "user-1", &mut NoTransaction)
            .await
            .unwrap();
        assert!(by_doc.is_some(), "查看权依据可命中");
    })
}

#[tokio::test]
#[ignore]
async fn workflow_action_search_respects_pagination_sort_and_projection() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_list").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.workflow_actions()
            .create(&sample_action("wa-1", "order-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.workflow_actions()
            .create(&sample_action("wa-2", "order-1"), &mut NoTransaction)
            .await
            .unwrap();
        db.workflow_actions()
            .create(&sample_action("wa-3", "order-2"), &mut NoTransaction)
            .await
            .unwrap();

        let filter = WorkflowActionFilter {
            document_id: Some(BusinessDocumentId::new("order-1")),
            actor_id: None,
            action_type: None,
            page: 1,
            page_size: 1,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .workflow_actions()
            .search_workflow_actions(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "order-1 的动作共两条");
        assert_eq!(page.items.len(), 1, "第一页一条");
        let row = &page.items[0];
        assert_eq!(row.document_id, "order-1");
        assert_eq!(row.action_type, WorkflowActionType::Approve);
        assert_eq!(row.from_status, "PENDING_REVIEW");
        assert_eq!(row.to_status, "EFFECTIVE");
        assert_eq!(row.actor_id, "user-1");
        assert_eq!(row.actor_role, "sales-manager");
        assert!(row.created_at > 0);

        let second = WorkflowActionFilter {
            page: 2,
            page_size: 1,
            ..filter.clone()
        };
        let page_two = db
            .workflow_actions()
            .search_workflow_actions(&second, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page_two.items.len(), 1, "第二页一条");
        assert_ne!(
            page_two.items[0].id, row.id,
            "同一秒创建的两条动作顺序不确定，两页必须各占一条"
        );

        let actor = db
            .workflow_actions()
            .list_by_document(&BusinessDocumentId::new("order-1"), &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(actor.len(), 2, "单据历史索引命中两条");
    })
}

#[tokio::test]
#[ignore]
async fn business_document_search_matches_no_regex_and_type_filter() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_no_search").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.business_documents()
            .register(&sample_document("bd-1", "SO-2025-001"), &mut NoTransaction)
            .await
            .unwrap();
        db.business_documents()
            .register(&sample_document("bd-2", "SO-2025-002"), &mut NoTransaction)
            .await
            .unwrap();
        let purchase = BusinessDocument::new(
            BusinessDocumentId::new("bd-3"),
            BusinessDocumentData {
                document_type: DocumentType::PurchaseOrder,
                document_no: "PO-2025-001".to_string(),
            },
        )
        .unwrap();
        db.business_documents()
            .register(&purchase, &mut NoTransaction)
            .await
            .unwrap();

        let filter = BusinessDocumentFilter {
            document_type: Some(DocumentType::SalesOrder),
            document_no: Some("so-2025-00".to_string()),
            page: 1,
            page_size: 20,
            sort_by: Some("created_at".to_string()),
            sort_ascending: true,
        };
        let page = db
            .business_documents()
            .search_business_documents(&filter, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(page.total, 2, "类型 + 编号模糊匹配命中两条");
        let mut document_nos: Vec<&str> = page.items.iter().map(|row| row.document_no.as_str()).collect();
        document_nos.sort_unstable();
        assert_eq!(document_nos, vec!["SO-2025-001", "SO-2025-002"]);
        assert_eq!(page.items[0].document_type, DocumentType::SalesOrder);
        assert!(page.items[0].formalized_at.is_none());

        let no_match = BusinessDocumentFilter {
            document_type: None,
            document_no: Some("PO-2025-001".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        let purchase_only = db
            .business_documents()
            .search_business_documents(&no_match, &mut NoTransaction)
            .await
            .unwrap();
        assert_eq!(purchase_only.total, 1);
        assert_eq!(purchase_only.items[0].document_no, "PO-2025-001");
    })
}

#[tokio::test]
#[ignore]
async fn create_document_with_action_commits_atomically_inside_transaction() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_tx_commit").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let doc = sample_document("bd-1", "SO-2025-001");
        let action = sample_action("wa-1", "bd-1");

        let db_clone = db.clone();
        let doc_for_tx = doc.clone();
        let action_for_tx = action.clone();
        test_db
            .client()
            .with_transaction::<_, (), database::Error>(move |session| {
                Box::pin(async move {
                    db_clone
                        .document_registry()
                        .create_document_with_action(&doc_for_tx, &action_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await
            .expect("事务提交应成功");

        let doc_found = db
            .business_documents()
            .find_by_type_and_no(DocumentType::SalesOrder, "SO-2025-001", &mut NoTransaction)
            .await
            .unwrap();
        assert!(doc_found.is_some(), "事务提交后注册行可见");
        let action_found = db
            .workflow_actions()
            .find_by_id(&action.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(action_found.is_some(), "事务提交后动作可见");
    })
}

#[tokio::test]
#[ignore]
async fn create_document_with_action_rolls_back_both_collections_on_conflict() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_tx_conflict").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.business_documents()
            .register(&sample_document("bd-1", "SO-2025-001"), &mut NoTransaction)
            .await
            .unwrap();

        let conflicting = sample_document("bd-other", "SO-2025-001");
        let action = sample_action("wa-1", "bd-other");

        let db_clone = db.clone();
        let doc_for_tx = conflicting.clone();
        let action_for_tx = action.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .document_registry()
                        .create_document_with_action(&doc_for_tx, &action_for_tx, session)
                        .await?;
                    Ok(())
                })
            })
            .await;
        assert!(
            matches!(result, Err(database::Error::DuplicateKey(_))),
            "身份冲突必须整体回滚并透出 DuplicateKey，实际为 {result:?}"
        );

        let action_found = db
            .workflow_actions()
            .find_by_id(&action.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(action_found.is_none(), "冲突回滚后动作不得残留");
        let second = db
            .business_documents()
            .find_by_id("bd-other", &mut NoTransaction)
            .await
            .unwrap();
        assert!(second.is_none(), "冲突回滚后注册行不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn transaction_abort_rolls_back_both_collections() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_tx_abort").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        let doc = sample_document("bd-1", "SO-2025-001");
        let action = sample_action("wa-1", "bd-1");

        let db_clone = db.clone();
        let doc_for_tx = doc.clone();
        let action_for_tx = action.clone();
        let result: std::result::Result<(), database::Error> = test_db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    db_clone
                        .document_registry()
                        .create_document_with_action(&doc_for_tx, &action_for_tx, session)
                        .await?;
                    Err(database::Error::OptimisticLockingError)
                })
            })
            .await;
        assert!(result.is_err(), "闭包返回错误必须整体回滚");

        let doc_found = db
            .business_documents()
            .find_by_type_and_no(DocumentType::SalesOrder, "SO-2025-001", &mut NoTransaction)
            .await
            .unwrap();
        assert!(doc_found.is_none(), "回滚后注册行不得残留");
        let action_found = db
            .workflow_actions()
            .find_by_id(&action.base.id, &mut NoTransaction)
            .await
            .unwrap();
        assert!(action_found.is_none(), "回滚后动作不得残留");
    })
}

#[tokio::test]
#[ignore]
async fn create_document_with_action_no_transaction_leaves_partial_write() {
    require_mongo!(async {
        let test_db = TestDb::new("docreg_notx").await.unwrap();
        let db = test_db.db();
        assert_domain_indexes(db).await;

        db.workflow_actions()
            .create(&sample_action("wa-1", "bd-other"), &mut NoTransaction)
            .await
            .unwrap();

        let doc = sample_document("bd-1", "SO-2025-001");
        let duplicated_action = sample_action("wa-1", "bd-1");
        let error = db
            .document_registry()
            .create_document_with_action(&doc, &duplicated_action, &mut NoTransaction)
            .await
            .expect_err("第二笔写入冲突必须返回错误");
        assert!(
            matches!(error, database::Error::DuplicateKey(_)),
            "期望 DuplicateKey，实际为 {error:?}"
        );

        let doc_found = db
            .business_documents()
            .find_by_type_and_no(DocumentType::SalesOrder, "SO-2025-001", &mut NoTransaction)
            .await
            .unwrap();
        assert!(
            doc_found.is_some(),
            "NoTransaction 下第一笔已自动提交，留下半成品（方法注释已声明该行为）"
        );
    })
}
