use super::DefinitionCatalogStatusFact;
use crate::repository::extensions::BpmExt;
use crate::{ensure_indexes, NoTransaction, Transactional};
use bpm::graph::DefinitionGraph;
use bpm::ids::{ApprovalNodeDefinitionId, ApprovalProcessDefinitionId, ApprovalTransitionDefinitionId};
use bpm::model::types::ApprovalDefinitionStatus;
use bpm::model::{
    ApprovalNodeDefinition, ApprovalProcessDefinition, NewNodeDefinition, ParticipantId, Timestamp,
};
use bpm::ProcessKind;
use test_support::{require_mongo, TestDb};

fn draft(id: &str, kind: ProcessKind, version: u32) -> ApprovalProcessDefinition {
    ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(id),
        kind,
        version,
        "测试定义",
        "n1",
        ParticipantId::new("admin").unwrap(),
        Timestamp::from_unix_secs(1).unwrap(),
    )
    .unwrap()
}

fn one_node_graph(id: &str, kind: ProcessKind, version: u32) -> DefinitionGraph {
    let nodes = vec![ApprovalNodeDefinition::new(NewNodeDefinition {
        id: ApprovalNodeDefinitionId::new(format!("{id}-n1")),
        process_definition_id: ApprovalProcessDefinitionId::new(id),
        node_key: "n1".into(),
        node_name: "仓储".into(),
        node_purpose: None,
        display_order: 1,
        assignee_participant_id: ParticipantId::new("u1").unwrap(),
        assignee_label_snapshot: "仓储".into(),
        at: Timestamp::from_unix_secs(1).unwrap(),
    })
    .unwrap()];
    DefinitionGraph::new_populated_draft(
        ApprovalProcessDefinitionId::new(id),
        kind,
        version,
        "测试定义",
        ParticipantId::new("admin").unwrap(),
        nodes,
        (1..=2)
            .map(|index| ApprovalTransitionDefinitionId::new(format!("{id}-t{index}")))
            .collect(),
        Timestamp::from_unix_secs(1).unwrap(),
    )
    .unwrap()
}

/// 批量目录覆盖 published-only、draft-only、并存、缺失、退役、软删、空输入、去重与重复状态失败关闭。
#[tokio::test]
#[ignore = "requires MongoDB replica set"]
async fn definition_catalog_facts_covers_batch_matrix_on_mongo() {
    require_mongo!(async {
        let fixture = TestDb::new("app-r03-catalog").await.expect("测试库");
        ensure_indexes(fixture.db()).await.expect("索引");
        let repo = fixture.db().bpm_workflow();

        let empty = repo
            .definition_catalog_facts(&[], &mut NoTransaction)
            .await
            .expect("空输入");
        assert!(empty.is_empty());

        let mut published_only = draft("pub-only", ProcessKind::SalesOrder, 1);
        published_only
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        fixture
            .db()
            .approval_process_definitions()
            .create(&published_only, &mut NoTransaction)
            .await
            .expect("写入发布");

        let draft_only = draft("draft-only", ProcessKind::VoucherSalesOrder, 1);
        fixture
            .db()
            .approval_process_definitions()
            .create(&draft_only, &mut NoTransaction)
            .await
            .expect("写入草稿");

        let mut coexist_published = draft("coexist-p", ProcessKind::StockAdjustment, 1);
        coexist_published
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        let coexist_draft = draft("coexist-d", ProcessKind::StockAdjustment, 2);
        fixture
            .db()
            .approval_process_definitions()
            .create(&coexist_published, &mut NoTransaction)
            .await
            .expect("并存发布");
        fixture
            .db()
            .approval_process_definitions()
            .create(&coexist_draft, &mut NoTransaction)
            .await
            .expect("并存草稿");

        let mut retired = draft("retired", ProcessKind::Delivery, 1);
        retired
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        retired
            .retire(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(3).unwrap(),
            )
            .unwrap();
        fixture
            .db()
            .approval_process_definitions()
            .create(&retired, &mut NoTransaction)
            .await
            .expect("写入退役");

        let mut soft = draft("soft", ProcessKind::Invoice, 1);
        soft.publish(
            ParticipantId::new("admin").unwrap(),
            Timestamp::from_unix_secs(2).unwrap(),
        )
        .unwrap();
        fixture
            .db()
            .approval_process_definitions()
            .create(&soft, &mut NoTransaction)
            .await
            .expect("写入待软删");
        let mut loaded = fixture
            .db()
            .approval_process_definitions()
            .find_by_id("soft", &mut NoTransaction)
            .await
            .expect("读取待软删")
            .expect("存在");
        fixture
            .db()
            .approval_process_definitions()
            .soft_delete(&mut loaded, &mut NoTransaction)
            .await
            .expect("软删");

        let kinds = [
            ProcessKind::SalesOrder,
            ProcessKind::VoucherSalesOrder,
            ProcessKind::StockAdjustment,
            ProcessKind::Delivery,
            ProcessKind::Invoice,
            ProcessKind::PurchaseOrder,
            ProcessKind::SalesOrder,
        ];
        let facts = repo
            .definition_catalog_facts(&kinds, &mut NoTransaction)
            .await
            .expect("批量目录");
        assert_eq!(
            facts,
            vec![
                DefinitionCatalogStatusFact {
                    process_kind: ProcessKind::SalesOrder,
                    published_version: Some(1),
                    draft_version: None,
                },
                DefinitionCatalogStatusFact {
                    process_kind: ProcessKind::VoucherSalesOrder,
                    published_version: None,
                    draft_version: Some(1),
                },
                DefinitionCatalogStatusFact {
                    process_kind: ProcessKind::StockAdjustment,
                    published_version: Some(1),
                    draft_version: Some(2),
                },
                DefinitionCatalogStatusFact {
                    process_kind: ProcessKind::Delivery,
                    published_version: None,
                    draft_version: None,
                },
                DefinitionCatalogStatusFact {
                    process_kind: ProcessKind::Invoice,
                    published_version: None,
                    draft_version: None,
                },
                DefinitionCatalogStatusFact {
                    process_kind: ProcessKind::PurchaseOrder,
                    published_version: None,
                    draft_version: None,
                },
            ]
        );

        let dirty = TestDb::new("app-r03-dup").await.expect("脏数据库");
        let mut first = draft("dup-1", ProcessKind::SalesOrder, 1);
        first
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        let mut second = draft("dup-2", ProcessKind::SalesOrder, 2);
        second
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(3).unwrap(),
            )
            .unwrap();
        dirty
            .db()
            .approval_process_definitions()
            .create(&first, &mut NoTransaction)
            .await
            .expect("脏发布1");
        dirty
            .db()
            .approval_process_definitions()
            .create(&second, &mut NoTransaction)
            .await
            .expect("脏发布2");
        assert!(dirty
            .db()
            .bpm_workflow()
            .definition_catalog_facts(&[ProcessKind::SalesOrder], &mut NoTransaction)
            .await
            .is_err());
    });
}

/// 发布图加载覆盖无发布、草稿、退役、完整图、同 session 与重复发布失败关闭。
#[tokio::test]
#[ignore = "requires MongoDB replica set"]
async fn load_published_definition_graph_covers_row_cases_on_mongo() {
    require_mongo!(async {
        let fixture = TestDb::new("app-r04-graph").await.expect("测试库");
        ensure_indexes(fixture.db()).await.expect("索引");
        let repo = fixture.db().bpm_workflow();
        assert!(repo
            .load_published_definition_graph(ProcessKind::StockAdjustment, &mut NoTransaction)
            .await
            .expect("无发布")
            .is_none());

        let draft_only = draft("draft-g", ProcessKind::StockAdjustment, 1);
        fixture
            .db()
            .approval_process_definitions()
            .create(&draft_only, &mut NoTransaction)
            .await
            .expect("草稿");
        assert!(repo
            .load_published_definition_graph(ProcessKind::StockAdjustment, &mut NoTransaction)
            .await
            .expect("草稿不命中")
            .is_none());

        let mut graph = one_node_graph("pub-g", ProcessKind::PurchaseOrder, 1);
        graph
            .definition
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        fixture
            .db()
            .approval_process_definitions()
            .create(&graph.definition, &mut NoTransaction)
            .await
            .expect("发布定义");
        for node in &graph.nodes {
            fixture
                .db()
                .approval_node_definitions()
                .create(node, &mut NoTransaction)
                .await
                .expect("节点");
        }
        for transition in &graph.transitions {
            fixture
                .db()
                .approval_transition_definitions()
                .create(transition, &mut NoTransaction)
                .await
                .expect("连线");
        }

        let client = fixture.client().clone();
        let db = fixture.db().clone();
        let loaded = client
            .with_transaction(|session| {
                let db = db.clone();
                Box::pin(async move {
                    db.bpm_workflow()
                        .load_published_definition_graph(ProcessKind::PurchaseOrder, session)
                        .await
                })
            })
            .await
            .expect("同会话加载")
            .expect("完整图");
        assert_eq!(loaded.definition.status, ApprovalDefinitionStatus::Published);
        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.transitions.len(), 2);
        assert_eq!(loaded.definition.base.id, "pub-g");

        let mut retired = draft("ret-g", ProcessKind::Delivery, 1);
        retired
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        retired
            .retire(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(3).unwrap(),
            )
            .unwrap();
        fixture
            .db()
            .approval_process_definitions()
            .create(&retired, &mut NoTransaction)
            .await
            .expect("退役");
        assert!(repo
            .load_published_definition_graph(ProcessKind::Delivery, &mut NoTransaction)
            .await
            .expect("退役不命中")
            .is_none());

        let dirty = TestDb::new("app-r04-dup").await.expect("脏数据库");
        let mut first = draft("dup-g1", ProcessKind::SalesOrder, 1);
        first
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        let mut second = draft("dup-g2", ProcessKind::SalesOrder, 2);
        second
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(3).unwrap(),
            )
            .unwrap();
        dirty
            .db()
            .approval_process_definitions()
            .create(&first, &mut NoTransaction)
            .await
            .expect("脏发布1");
        dirty
            .db()
            .approval_process_definitions()
            .create(&second, &mut NoTransaction)
            .await
            .expect("脏发布2");
        assert!(dirty
            .db()
            .bpm_workflow()
            .load_published_definition_graph(ProcessKind::SalesOrder, &mut NoTransaction)
            .await
            .is_err());
    });
}
