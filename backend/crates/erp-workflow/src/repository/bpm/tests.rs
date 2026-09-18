use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
use bpm::model::types::{
    ApprovalCommandKind, ApprovalDefinitionStatus, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
};
use bpm::model::{ApprovalProcessInstance, IdempotencyKey, NewProcessInstance, ParticipantId, Timestamp};
use bpm::{ProcessKind, SubjectRef};
use entity_core::{BaseModel, HasBaseModel};
use mongodb::bson::{Bson, doc, serialize_to_document};

use super::cas::execution_end_filter;
use super::definition_query::{
    definition_catalog_filter, definition_catalog_options, definition_child_filter,
    definition_graph_transition_limit, definition_versions_filter, group_definition_catalog_rows,
    latest_definition_version_from_rows, latest_definition_version_options, published_kind_docs_filter,
    unique_process_kinds, unique_published_definition,
};
use super::execution_query::{current_execution_filter, execution_history_filter, execution_history_limit};
use super::instance_query::{
    cancellation_subject_filter, instance_text_query_or, latest_subject_filter, non_terminal_subject_filter,
};
use super::runtime_write::{
    cancellable_execution_end_filter, cancelled_instance_projection, instance_advance_filter,
    instance_insert_document, previous_version, receipt_key_filter, require_cas_applied,
};
use super::{
    ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListProjection,
    ApprovalInstanceListView, ApprovalInstanceTextQuery, AssignDocumentNoOutcome, CasWriteOutcome,
    DefinitionCatalogRow, DefinitionCatalogStatusFact, LatestDefinitionVersionProjection,
    MAX_CATALOG_STATUS_ROWS, MAX_DEFINITION_GRAPH_DOCS, MAX_EXECUTION_HISTORY, MAX_INSTANCE_PAGE,
    approval_task_cas_filter, assign_document_no_filter, clamp_limit, classify_assign_document_no_miss,
    classify_cas_miss, instance_list_filter_doc, instance_list_scope_empty, instance_list_sort,
    instance_summary_projection, merge_documents,
};

#[derive(Clone)]
struct LockProbe {
    base: BaseModel,
    status_ok: bool,
}

impl HasBaseModel for LockProbe {
    fn base(&self) -> &BaseModel {
        &self.base
    }

    fn base_mut(&mut self) -> &mut BaseModel {
        &mut self.base
    }
}

fn probe(version: u64, status_ok: bool) -> LockProbe {
    let mut base = BaseModel::new("doc-1".to_string());
    base.version = version;
    LockProbe { base, status_ok }
}

/// 验证最高定义版本查询保留未删除过滤、降序、单条限制和最小投影。
///
/// 测试只断言构造出的过滤与选项，不连接 MongoDB；任一查询约束漂移时失败。
#[test]
fn latest_definition_version_query_is_minimal_and_bounded() {
    assert_eq!(
        definition_versions_filter(ProcessKind::SalesOrder),
        doc! {
            "process_kind": "sales_order",
            "deleted_at": 0_i64,
        }
    );
    let options = latest_definition_version_options();
    assert_eq!(options.sort, Some(doc! { "definition_version": -1 }));
    assert_eq!(options.limit, Some(1));
    assert_eq!(options.projection, Some(doc! { "definition_version": 1, "_id": 0 }));
}

/// 目录批量查询保留软删除、草稿/发布状态、种类 `$in` 与固定上限。
#[test]
fn definition_catalog_query_is_bounded_and_filters_retired() {
    assert!(unique_process_kinds(&[]).is_empty());
    assert_eq!(
        unique_process_kinds(&[
            ProcessKind::SalesOrder,
            ProcessKind::StockAdjustment,
            ProcessKind::SalesOrder
        ]),
        vec![ProcessKind::SalesOrder, ProcessKind::StockAdjustment]
    );
    let filter = definition_catalog_filter(&[ProcessKind::SalesOrder, ProcessKind::StockAdjustment]);
    assert_eq!(
        filter.get_document("process_kind").unwrap().get_array("$in").unwrap(),
        &vec![Bson::String("sales_order".into()), Bson::String("stock_adjustment".into())]
    );
    assert_eq!(
        filter.get_document("status").unwrap().get_array("$in").unwrap(),
        &vec![Bson::String("DRAFT".into()), Bson::String("PUBLISHED".into())]
    );
    assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
    let options = definition_catalog_options(20);
    assert_eq!(options.limit, Some(80));
    assert_eq!(options.limit.unwrap(), MAX_CATALOG_STATUS_ROWS);
    assert_eq!(
        options.projection,
        Some(doc! { "process_kind": 1, "status": 1, "definition_version": 1, "_id": 0 })
    );
    let published_filter = published_kind_docs_filter(ProcessKind::StockAdjustment);
    assert_eq!(published_filter.get_str("status").unwrap(), "PUBLISHED");
    assert_eq!(published_filter.get_i64("deleted_at").unwrap(), 0);
}

/// 目录归组覆盖 published-only、draft-only、并存、缺失，并拒绝重复状态与退役投影。
#[test]
fn definition_catalog_grouping_covers_status_matrix_and_duplicate_fail_closed() {
    let kinds = [
        ProcessKind::SalesOrder,
        ProcessKind::VoucherSalesOrder,
        ProcessKind::StockAdjustment,
        ProcessKind::Delivery,
        ProcessKind::Invoice,
    ];
    let facts = group_definition_catalog_rows(
        &kinds,
        vec![
            DefinitionCatalogRow {
                process_kind: ProcessKind::SalesOrder,
                status: ApprovalDefinitionStatus::Published,
                definition_version: 3,
            },
            DefinitionCatalogRow {
                process_kind: ProcessKind::VoucherSalesOrder,
                status: ApprovalDefinitionStatus::Draft,
                definition_version: 1,
            },
            DefinitionCatalogRow {
                process_kind: ProcessKind::StockAdjustment,
                status: ApprovalDefinitionStatus::Published,
                definition_version: 2,
            },
            DefinitionCatalogRow {
                process_kind: ProcessKind::StockAdjustment,
                status: ApprovalDefinitionStatus::Draft,
                definition_version: 4,
            },
        ],
    )
    .unwrap();
    assert_eq!(
        facts,
        vec![
            DefinitionCatalogStatusFact {
                process_kind: ProcessKind::SalesOrder,
                published_version: Some(3),
                draft_version: None,
            },
            DefinitionCatalogStatusFact {
                process_kind: ProcessKind::VoucherSalesOrder,
                published_version: None,
                draft_version: Some(1),
            },
            DefinitionCatalogStatusFact {
                process_kind: ProcessKind::StockAdjustment,
                published_version: Some(2),
                draft_version: Some(4),
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
        ]
    );
    assert!(
        group_definition_catalog_rows(
            &[ProcessKind::SalesOrder],
            vec![
                DefinitionCatalogRow {
                    process_kind: ProcessKind::SalesOrder,
                    status: ApprovalDefinitionStatus::Published,
                    definition_version: 1,
                },
                DefinitionCatalogRow {
                    process_kind: ProcessKind::SalesOrder,
                    status: ApprovalDefinitionStatus::Published,
                    definition_version: 2,
                },
            ],
        )
        .is_err()
    );
    assert!(
        group_definition_catalog_rows(
            &[ProcessKind::SalesOrder],
            vec![DefinitionCatalogRow {
                process_kind: ProcessKind::SalesOrder,
                status: ApprovalDefinitionStatus::Retired,
                definition_version: 1,
            }],
        )
        .is_err()
    );
    assert!(unique_published_definition(Vec::new()).unwrap().is_none());
    let mut published_a = dummy_definition("a");
    published_a.publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(2).unwrap()).unwrap();
    let mut published_b = dummy_definition("b");
    published_b.publish(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(2).unwrap()).unwrap();
    assert_eq!(unique_published_definition(vec![published_a.clone()]).unwrap().unwrap().base.id, "a");
    assert!(unique_published_definition(vec![published_a, published_b]).is_err());
}

fn dummy_definition(id: &str) -> bpm::model::ApprovalProcessDefinition {
    bpm::model::ApprovalProcessDefinition::new_draft(
        ApprovalProcessDefinitionId::new(id),
        ProcessKind::StockAdjustment,
        1,
        "库存调整",
        "n1",
        ParticipantId::new("admin").unwrap(),
        Timestamp::from_unix_secs(1).unwrap(),
    )
    .unwrap()
}

/// 验证最高版本投影读取首条记录，并在没有历史定义时返回空边界。
///
/// 测试覆盖命中与空历史两条纯映射路径，不执行数据库访问。
#[test]
fn latest_definition_version_projection_handles_hit_and_empty_history() {
    assert_eq!(latest_definition_version_from_rows(Vec::new()), None);
    assert_eq!(
        latest_definition_version_from_rows(vec![LatestDefinitionVersionProjection {
            definition_version: 7,
        }]),
        Some(7)
    );
}

#[test]
fn non_terminal_subject_filter_excludes_definition_id() {
    let subject = SubjectRef::new("stock_adjustment", "adj-1").unwrap();
    let filter = non_terminal_subject_filter(&subject, 2);
    assert!(!filter.contains_key("process_definition_id"));
    assert_eq!(filter.get_str("subject.subject_kind").unwrap(), "stock_adjustment");
    assert_eq!(filter.get_document("status").unwrap(), &doc! { "$in": ["RUNNING", "BLOCKED"] });
}

/// 取消候选查询保留提交版本但不得预先过滤实例状态。
///
/// 终态实例也必须交给 BPM 模型给出确定的不可取消结果。
#[test]
fn cancellation_subject_filter_defers_status_rule_to_model() {
    let subject = SubjectRef::new("sales_order", "so-1").unwrap();
    let filter = cancellation_subject_filter(&subject, 3);
    assert_eq!(filter.get_str("subject.subject_kind").unwrap(), "sales_order");
    assert_eq!(filter.get_str("subject.subject_id").unwrap(), "so-1");
    assert_eq!(filter.get_i64("subject_version").unwrap(), 3);
    assert!(!filter.contains_key("status"));
    assert!(filter.contains_key("deleted_at"));
}

/// 详情投影按主体查最近实例，不得附带状态或提交版本。
#[test]
fn latest_subject_filter_omits_status_and_version() {
    let subject = SubjectRef::new("sales_order", "so-1").unwrap();
    let filter = latest_subject_filter(&subject);
    assert!(!filter.contains_key("status"));
    assert!(!filter.contains_key("subject_version"));
    assert_eq!(filter.get_str("subject.subject_kind").unwrap(), "sales_order");
    assert_eq!(filter.get_str("subject.subject_id").unwrap(), "so-1");
    assert!(filter.contains_key("deleted_at"));
}

#[test]
fn instance_and_execution_cas_filters_include_token_and_status() {
    let execution = ApprovalNodeExecutionId::new("exec-1");
    let advance = instance_advance_filter("inst-1", 4, &execution).unwrap();
    assert_eq!(advance.get_i64("version").unwrap(), 4);
    assert_eq!(advance.get_str("current_node_execution_id").unwrap(), "exec-1");
    assert_eq!(advance.get_document("status").unwrap(), &doc! { "$in": ["RUNNING", "BLOCKED"] });
    let ended = execution_end_filter("exec-1", 2, ApprovalNodeExecutionStatus::Active).unwrap();
    assert_eq!(ended.get_str("status").unwrap(), "ACTIVE");
    let blocked = execution_end_filter("exec-1", 2, ApprovalNodeExecutionStatus::Blocked).unwrap();
    assert_eq!(blocked.get_str("status").unwrap(), "BLOCKED");
    assert_eq!(blocked.get_i64("version").unwrap(), 2);
    let current = current_execution_filter(&ApprovalProcessInstanceId::new("inst-1"));
    assert_eq!(current.get_document("status").unwrap(), &doc! { "$in": ["ACTIVE", "BLOCKED"] });
}

/// 取消执行 CAS 同时允许活动和受阻状态，并保持版本与软删除约束。
///
/// 其他结束状态不得进入取消写入过滤条件。
#[test]
fn cancellable_execution_filter_accepts_current_states_only() {
    let filter = cancellable_execution_end_filter("exec-1", 2).unwrap();
    assert_eq!(filter.get_i64("version").unwrap(), 2);
    assert_eq!(filter.get_document("status").unwrap(), &doc! { "$in": ["ACTIVE", "BLOCKED"] });
    assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
}

/// 取消投影清空当前节点与审批人，并使用实例终态时间。
///
/// 版本反推和 CAS 分类在非应用结果上统一失败关闭。
#[test]
fn cancelled_projection_and_write_guards_are_deterministic() {
    let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
        id: ApprovalProcessInstanceId::new("inst-cancelled"),
        process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
        definition_version: 1,
        process_kind: ProcessKind::StockAdjustment,
        subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
        subject_version: 1,
        started_by: ParticipantId::new("u1").unwrap(),
        at: Timestamp::from_unix_secs(10).unwrap(),
    })
    .unwrap();
    instance.cancel(Timestamp::from_unix_secs(20).unwrap()).unwrap();

    let projection = cancelled_instance_projection(&instance);
    assert_eq!(projection.last_status_changed_at, Some(20));
    assert!(projection.current_node_key.is_none());
    assert!(projection.current_assignee_participant_id.is_none());
    assert_eq!(previous_version(instance.base.version).unwrap(), 1);
    assert!(previous_version(0).is_err());
    assert!(require_cas_applied(CasWriteOutcome::Applied(())).is_ok());
    assert!(require_cas_applied(CasWriteOutcome::<()>::NotFound).is_err());
}

#[test]
fn start_instance_insert_includes_bounded_list_projection() {
    let instance = ApprovalProcessInstance::start_running(NewProcessInstance {
        id: ApprovalProcessInstanceId::new("inst-1"),
        process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
        definition_version: 1,
        process_kind: ProcessKind::StockAdjustment,
        subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
        subject_version: 1,
        started_by: ParticipantId::new("u1").unwrap(),
        at: Timestamp::from_unix_secs(10).unwrap(),
    })
    .unwrap();
    let projection = ApprovalInstanceListProjection {
        current_node_key: Some("n1".into()),
        current_node_name: Some("仓储复核".into()),
        current_assignee_participant_id: Some("u1".into()),
        current_assignee_name: Some("张三".into()),
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: Some(10),
    };
    let document = instance_insert_document(&instance, &projection).unwrap();
    assert_eq!(document.get_str("id").unwrap(), "inst-1");
    assert_eq!(document.get_str("current_node_key").unwrap(), "n1");
    assert_eq!(document.get_str("current_assignee_participant_id").unwrap(), "u1");
    assert_eq!(document.get_i64("last_status_changed_at").unwrap(), 10);
    assert_eq!(serialize_to_document(&projection).unwrap().get_str("current_node_name").unwrap(), "仓储复核");
    let mut merged = doc! { "id": "inst-1" };
    merge_documents(&mut merged, serialize_to_document(&projection).unwrap());
    assert_eq!(merged.get_str("current_assignee_name").unwrap(), "张三");
}

#[test]
fn cas_miss_classifies_not_found_version_and_status() {
    assert!(matches!(
        classify_cas_miss::<LockProbe>(None, 1, |item| item.status_ok),
        CasWriteOutcome::NotFound
    ));
    assert!(matches!(
        classify_cas_miss(Some(probe(2, true)), 1, |item| item.status_ok),
        CasWriteOutcome::VersionConflict(_)
    ));
    assert!(matches!(
        classify_cas_miss(Some(probe(1, false)), 1, |item| item.status_ok),
        CasWriteOutcome::StatusChanged(_)
    ));
}

#[test]
fn document_no_assignment_distinguishes_same_payload_and_race() {
    let filter = assign_document_no_filter("bd-1", 3).unwrap();
    assert_eq!(filter.get_str("id").unwrap(), "bd-1");
    assert_eq!(
        filter.get_array("$or").unwrap(),
        &vec![Bson::Document(doc! { "document_no": "" }), Bson::Document(doc! { "document_no": Bson::Null }),]
    );

    assert!(matches!(
        classify_assign_document_no_miss(None::<(u64, String)>, 1, "SO-1", |row| row.0, |row| row.1.as_str()),
        AssignDocumentNoOutcome::NotFound
    ));
    assert!(matches!(
        classify_assign_document_no_miss(
            Some((1_u64, "SO-1".to_string())),
            1,
            "SO-1",
            |row| row.0,
            |row| row.1.as_str()
        ),
        AssignDocumentNoOutcome::SamePayload(_)
    ));
    assert!(matches!(
        classify_assign_document_no_miss(
            Some((1_u64, "SO-2".to_string())),
            1,
            "SO-1",
            |row| row.0,
            |row| row.1.as_str()
        ),
        AssignDocumentNoOutcome::NumberConflict(_)
    ));
    assert!(matches!(
        classify_assign_document_no_miss(
            Some((2_u64, String::new())),
            1,
            "SO-1",
            |row| row.0,
            |row| row.1.as_str()
        ),
        AssignDocumentNoOutcome::VersionConflict(_)
    ));
    assert!(matches!(
        classify_assign_document_no_miss(
            Some((1_u64, String::new())),
            1,
            "SO-1",
            |row| row.0,
            |row| row.1.as_str()
        ),
        AssignDocumentNoOutcome::VersionConflict(_)
    ));
}

#[test]
fn approval_task_cas_requires_open_and_execution() {
    let filter = approval_task_cas_filter("wi-1", 7, &ApprovalNodeExecutionId::new("exec-1")).unwrap();
    assert_eq!(filter.get_str("status").unwrap(), "OPEN");
    assert_eq!(filter.get_str("approval_node_execution_id").unwrap(), "exec-1");
    assert_eq!(filter.get_i64("version").unwrap(), 7);
}

#[test]
fn instance_list_views_use_matching_sort_and_scope() {
    let managed = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Managed,
        process_kind: Some(ProcessKind::StockAdjustment),
        status: Some(ApprovalProcessInstanceStatus::Running),
        started_by: None,
        subject_kind: Some("stock_adjustment".into()),
        authorized_instance_ids: None,
        subject_ids: Some(vec!["adj-1".into()]),
        text_query: None,
        cursor: Some(ApprovalInstanceListCursor { sort_time: 10, id: "inst-9".into() }),
        limit: 20,
    };
    let mut constrained = managed.clone();
    constrained.authorized_instance_ids = Some(vec![]);
    let denied = instance_list_filter_doc(&constrained);
    assert_eq!(denied.get_document("id").unwrap(), &doc! { "$in": [] });
    constrained.authorized_instance_ids = Some(vec!["allowed-instance".into()]);
    let allowed = instance_list_filter_doc(&constrained);
    assert_eq!(allowed.get_document("id").unwrap(), &doc! { "$in": ["allowed-instance"] });
    assert!(allowed.contains_key("$or"), "scope IDs must intersect the cursor");
    let document = instance_list_filter_doc(&managed);
    assert_eq!(document.get_str("process_kind").unwrap(), "stock_adjustment");
    assert_eq!(document.get_str("status").unwrap(), "RUNNING");
    assert_eq!(instance_list_sort(&managed), doc! { "status": 1, "updated_at": -1, "id": -1 });
    assert_eq!(
        document.get_array("$or").unwrap(),
        &vec![
            Bson::Document(doc! { "updated_at": { "$lt": 10_i64 } }),
            Bson::Document(doc! { "updated_at": 10_i64, "id": { "$lt": "inst-9" } }),
        ]
    );

    let started = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Started,
        process_kind: Some(ProcessKind::StockAdjustment),
        status: None,
        started_by: Some("u1".into()),
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: None,
        cursor: Some(ApprovalInstanceListCursor { sort_time: 20, id: "inst-2".into() }),
        limit: 20,
    };
    let started_doc = instance_list_filter_doc(&started);
    assert_eq!(started_doc.get_str("started_by").unwrap(), "u1");
    assert!(!started_doc.contains_key("status"));
    assert_eq!(instance_list_sort(&started), doc! { "started_at": -1, "id": -1 });
    assert_eq!(
        started_doc.get_array("$or").unwrap(),
        &vec![
            Bson::Document(doc! { "started_at": { "$lt": 20_i64 } }),
            Bson::Document(doc! { "started_at": 20_i64, "id": { "$lt": "inst-2" } }),
        ]
    );

    let managed_open = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Managed,
        process_kind: None,
        status: None,
        started_by: None,
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: None,
        cursor: Some(ApprovalInstanceListCursor { sort_time: 8, id: "inst-3".into() }),
        limit: 20,
    };
    let managed_open_doc = instance_list_filter_doc(&managed_open);
    assert!(!managed_open_doc.contains_key("status"));
    assert_eq!(instance_list_sort(&managed_open), doc! { "updated_at": -1, "id": -1 });
    assert_eq!(
        managed_open_doc.get_array("$or").unwrap(),
        &vec![
            Bson::Document(doc! { "updated_at": { "$lt": 8_i64 } }),
            Bson::Document(doc! { "updated_at": 8_i64, "id": { "$lt": "inst-3" } }),
        ]
    );

    let blocked = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Blocked,
        process_kind: None,
        status: None,
        started_by: None,
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: None,
        cursor: Some(ApprovalInstanceListCursor { sort_time: 4, id: "inst-4".into() }),
        limit: 20,
    };
    let blocked_doc = instance_list_filter_doc(&blocked);
    assert_eq!(blocked_doc.get_str("status").unwrap(), "BLOCKED");
    assert_eq!(instance_list_sort(&blocked), doc! { "blocked_at": -1, "id": -1 });
    assert_eq!(
        blocked_doc.get_array("$or").unwrap(),
        &vec![
            Bson::Document(doc! { "blocked_at": { "$lt": 4_i64 } }),
            Bson::Document(doc! { "blocked_at": 4_i64, "id": { "$lt": "inst-4" } }),
        ]
    );

    let empty_scope = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Blocked,
        process_kind: None,
        status: None,
        started_by: None,
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: Some(Vec::new()),
        text_query: None,
        cursor: None,
        limit: 20,
    };
    assert!(instance_list_scope_empty(&empty_scope));
    assert!(!instance_list_scope_empty(&started));
    assert_eq!(instance_list_sort(&empty_scope), doc! { "blocked_at": -1, "id": -1 });
    assert_eq!(clamp_limit(0, MAX_INSTANCE_PAGE), 1);
    assert_eq!(clamp_limit(50, MAX_INSTANCE_PAGE), 50);
    assert_eq!(clamp_limit(101, MAX_INSTANCE_PAGE), 101);
    assert_eq!(clamp_limit(102, MAX_INSTANCE_PAGE), 101);
    assert_eq!(clamp_limit(u32::MAX, MAX_INSTANCE_PAGE), 101);
}

#[test]
fn started_view_fail_closes_without_started_by_and_allows_optional_filters() {
    let missing_starter = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Started,
        process_kind: Some(ProcessKind::StockAdjustment),
        status: Some(ApprovalProcessInstanceStatus::Running),
        started_by: None,
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: None,
        cursor: None,
        limit: 20,
    };
    assert!(instance_list_scope_empty(&missing_starter));

    let empty_starter =
        ApprovalInstanceListFilter { started_by: Some(String::new()), ..missing_starter.clone() };
    assert!(instance_list_scope_empty(&empty_starter));

    let kind_only = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Started,
        process_kind: Some(ProcessKind::StockAdjustment),
        status: None,
        started_by: Some("u1".into()),
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: None,
        cursor: None,
        limit: 20,
    };
    assert!(!instance_list_scope_empty(&kind_only));
    let kind_doc = instance_list_filter_doc(&kind_only);
    assert_eq!(kind_doc.get_str("started_by").unwrap(), "u1");
    assert_eq!(kind_doc.get_str("process_kind").unwrap(), "stock_adjustment");
    assert!(!kind_doc.contains_key("status"));
    assert_eq!(instance_list_sort(&kind_only), doc! { "started_at": -1, "id": -1 });

    let status_only = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Started,
        process_kind: None,
        status: Some(ApprovalProcessInstanceStatus::Running),
        started_by: Some("u1".into()),
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: None,
        cursor: None,
        limit: 20,
    };
    assert!(!instance_list_scope_empty(&status_only));
    let status_doc = instance_list_filter_doc(&status_only);
    assert_eq!(status_doc.get_str("started_by").unwrap(), "u1");
    assert_eq!(status_doc.get_str("status").unwrap(), "RUNNING");
    assert!(!status_doc.contains_key("process_kind"));
    assert_eq!(instance_list_sort(&status_only), doc! { "started_at": -1, "id": -1 });
}

/// 字面量检索转义正则，并与游标 `$or` 用 `$and` 组合。
#[test]
fn instance_list_text_query_is_literal_and_composes_with_cursor() {
    let text_query = ApprovalInstanceTextQuery { query: "SO.[1]".to_string() };
    let alternatives = instance_text_query_or(&text_query);
    assert_eq!(alternatives.len(), 3);
    let regex = alternatives[0].get_document("subject.subject_id").unwrap().get_str("$regex").unwrap();
    assert_eq!(regex, r"SO\.\[1\]");
    let with_cursor = ApprovalInstanceListFilter {
        view: ApprovalInstanceListView::Started,
        process_kind: None,
        status: None,
        started_by: Some("u1".into()),
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: Some(text_query.clone()),
        cursor: Some(ApprovalInstanceListCursor { sort_time: 20, id: "inst-2".into() }),
        limit: 20,
    };
    let document = instance_list_filter_doc(&with_cursor);
    let and = document.get_array("$and").unwrap();
    assert_eq!(and.len(), 2);
    assert!(and[0].as_document().unwrap().contains_key("$or"));
    assert!(and[1].as_document().unwrap().contains_key("$or"));
    assert_eq!(document.get_str("started_by").unwrap(), "u1");
    assert!(!document.contains_key("$or"));

    let query_only = ApprovalInstanceListFilter { cursor: None, ..with_cursor };
    let query_doc = instance_list_filter_doc(&query_only);
    assert!(query_doc.contains_key("$or"));
    assert!(!query_doc.contains_key("$and"));
    assert_eq!(query_doc.get_array("$or").unwrap().len(), 3);
}

#[test]
fn execution_history_filter_and_limit_are_bounded() {
    let instance_id = ApprovalProcessInstanceId::new("inst-1");
    let first_page = execution_history_filter(&instance_id, None);
    assert_eq!(first_page.get_str("process_instance_id").unwrap(), "inst-1");
    assert_eq!(first_page.get_i64("deleted_at").unwrap(), 0);
    assert!(!first_page.contains_key("execution_no"));

    let next_page = execution_history_filter(&instance_id, Some(7));
    assert_eq!(next_page.get_document("execution_no").unwrap(), &doc! { "$gt": 7_i64 });
    assert_eq!(next_page.get_str("process_instance_id").unwrap(), "inst-1");
    assert_eq!(execution_history_limit(0), 1);
    assert_eq!(execution_history_limit(50), MAX_EXECUTION_HISTORY);
    assert_eq!(execution_history_limit(51), MAX_EXECUTION_HISTORY);
    assert_eq!(execution_history_limit(u32::MAX), MAX_EXECUTION_HISTORY);
    assert_eq!(MAX_EXECUTION_HISTORY, 50);
}

#[test]
fn definition_child_filter_batches_by_definition_id_with_graph_limits() {
    let filter = definition_child_filter(&ApprovalProcessDefinitionId::new("def-1"));
    assert_eq!(filter.len(), 2);
    assert_eq!(filter.get_str("process_definition_id").unwrap(), "def-1");
    assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
    assert!(!filter.contains_key("node_key"));
    assert!(!filter.contains_key("id"));
    assert_eq!(MAX_DEFINITION_GRAPH_DOCS, 20);
    assert_eq!(definition_graph_transition_limit(), 40);
    assert_eq!(definition_graph_transition_limit(), MAX_DEFINITION_GRAPH_DOCS.saturating_mul(2));
}

#[test]
fn instance_summary_projection_is_bounded_and_excludes_history() {
    let projection = instance_summary_projection();
    let keys: std::collections::BTreeSet<&str> = projection.keys().map(String::as_str).collect();
    assert_eq!(
        keys,
        [
            "id",
            "process_kind",
            "process_definition_id",
            "definition_version",
            "subject",
            "subject_version",
            "status",
            "current_round_no",
            "current_node_execution_id",
            "current_node_key",
            "current_node_name",
            "current_assignee_participant_id",
            "current_assignee_name",
            "latest_rejected_execution_id",
            "latest_rejection_summary",
            "last_status_changed_at",
            "started_by",
            "started_at",
            "blocked_at",
            "version",
            "updated_at",
        ]
        .into_iter()
        .collect()
    );
    for field in [
        "id",
        "current_node_key",
        "current_node_name",
        "current_assignee_participant_id",
        "current_assignee_name",
        "latest_rejected_execution_id",
        "latest_rejection_summary",
        "last_status_changed_at",
    ] {
        assert_eq!(projection.get_i32(field).unwrap(), 1);
    }
    assert!(!projection.contains_key("history"));
    assert!(!projection.contains_key("executions"));
    assert!(!projection.contains_key("execution_history"));
    assert!(!projection.contains_key("node_executions"));
}

#[test]
fn receipt_filter_uses_command_scope_and_key() {
    let key = IdempotencyKey::parse("  key-1  ").unwrap();
    assert_eq!(
        receipt_key_filter(ApprovalCommandKind::SubmitDecision, "inst-1", &key),
        doc! {
            "command_kind": "SUBMIT_DECISION",
            "scope_id": "inst-1",
            "idempotency_key": "key-1",
        }
    );
}
