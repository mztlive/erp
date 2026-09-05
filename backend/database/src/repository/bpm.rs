//! BPM 模型仓储：目标集合映射、有界查询与带状态条件的 CAS。
//!
//! 本模块只读写 `bpm` 模型，不接收 ERP 实体，也不调用 BPM 决策函数。

pub use bpm::engine::DefinitionGraph;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId};
use bpm::model::types::{ApprovalDefinitionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessDefinition, ApprovalProcessInstance,
};
use bpm::{ProcessKind, SubjectRef};
use mongodb::bson::Document;
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::BpmExt;
use super::Repository;
use crate::executor::Executor;
use crate::{mongo_ops, Error, Result};

const DEFINITIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_PROCESS_DEFINITIONS;
const NODE_DEFINITIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_NODE_DEFINITIONS;
const TRANSITION_DEFINITIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_TRANSITION_DEFINITIONS;
const INSTANCES: &str = <mongodb::Database as BpmExt>::APPROVAL_PROCESS_INSTANCES;
const EXECUTIONS: &str = <mongodb::Database as BpmExt>::APPROVAL_NODE_EXECUTIONS;
const ASSIGNEES: &str = <mongodb::Database as BpmExt>::APPROVAL_INSTANCE_ASSIGNEES;
const RECEIPTS: &str = <mongodb::Database as BpmExt>::APPROVAL_COMMAND_RECEIPTS;

/// 命令收据唯一身份索引名；仅此索引冲突允许 Service 新会话回读胜者。
pub const APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX: &str = "uk_approval_command_receipts_idempotency";

const MAX_DEFINITION_GRAPH_DOCS: i64 = 20;
const MAX_DEFINITION_VERSIONS: i64 = 100;
const MAX_EXECUTION_HISTORY: i64 = 50;
const MAX_INSTANCE_PAGE: i64 = 101;
const MAX_CATALOG_STATUS_ROWS: i64 = 80;

/// CAS 写入结果。未命中必须区分为不存在、版本冲突或状态变化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CasWriteOutcome<T> {
    /// 条件更新成功。
    Applied(T),
    /// 目标文档不存在或已删除。
    NotFound,
    /// 文档存在但乐观锁版本不匹配。
    VersionConflict(T),
    /// 文档存在且版本匹配，但当前状态不允许该写入。
    StatusChanged(T),
}

/// 一次性编号赋值结果。同载荷回读，不同编号竞争只允许一个成功。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssignDocumentNoOutcome<T> {
    /// 本次成功写入空编号。
    Assigned(T),
    /// 目标已持有相同编号，按同载荷回读。
    SamePayload(T),
    /// 目标已持有不同编号。
    NumberConflict(T),
    /// 文档存在但版本不匹配。
    VersionConflict(T),
    /// 目标不存在或已删除。
    NotFound,
}

/// 实例列表视图。排序字段必须与匹配索引一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalInstanceListView {
    /// 我发起的审批：`started_by + started_at desc + id desc`。
    Started,
    /// 管理范围实例：未指定状态用 `updated_at`，指定状态用 `status + updated_at`。
    Managed,
    /// 公司级阻塞列表：`status + blocked_at desc + id desc`。
    Blocked,
}

/// 实例列表稳定游标。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalInstanceListCursor {
    /// 当前视图排序时间字段。
    pub sort_time: i64,
    /// 并列时的稳定主键。
    pub id: String,
}

/// Service 已计算的实例列表过滤条件。仓储必须在 MongoDB 内施加。
#[derive(Debug, Clone)]
pub struct ApprovalInstanceListFilter {
    /// 列表视图，决定排序与默认状态。
    pub view: ApprovalInstanceListView,
    /// 流程种类；为空时不筛选。
    pub process_kind: Option<ProcessKind>,
    /// 实例状态；`Managed` 未指定时走 `updated_at` 索引。
    pub status: Option<ApprovalProcessInstanceStatus>,
    /// 启动人；`Started` 视图由调用方提供。
    pub started_by: Option<String>,
    /// 业务对象种类。
    pub subject_kind: Option<String>,
    /// 授权范围内的对象主键；`Some(空)` 表示无可见对象。
    pub subject_ids: Option<Vec<String>>,
    /// 字面量检索；空表示不按关键词过滤。
    pub text_query: Option<ApprovalInstanceTextQuery>,
    /// 稳定游标；首页为空。
    pub cursor: Option<ApprovalInstanceListCursor>,
    /// 请求页大小，仓储会施加上限。
    pub limit: u32,
}

/// 实例列表字面量检索。仓储在 MongoDB 内施加，不得在内存过滤当前页。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalInstanceTextQuery {
    /// 调用方已规范化的检索串，仓储再转义为字面量正则。
    pub query: String,
}

/// 运行事务写入的有界列表投影。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalInstanceListProjection {
    /// 当前节点键。
    pub current_node_key: Option<String>,
    /// 当前节点名称。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee_participant_id: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 最近驳回执行。
    pub latest_rejected_execution_id: Option<String>,
    /// 最近驳回原因摘要。
    pub latest_rejection_summary: Option<String>,
    /// 最近状态变更时间。
    pub last_status_changed_at: Option<i64>,
}

/// 实例列表有界投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalInstanceSummary {
    /// 实例主键。
    pub id: String,
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 绑定定义。
    pub process_definition_id: ApprovalProcessDefinitionId,
    /// 绑定定义业务版本。
    pub definition_version: u32,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 实例状态。
    pub status: ApprovalProcessInstanceStatus,
    /// 当前轮次。
    pub current_round_no: u32,
    /// 当前执行。
    pub current_node_execution_id: Option<ApprovalNodeExecutionId>,
    /// 当前节点键投影。
    #[serde(default)]
    pub current_node_key: Option<String>,
    /// 当前节点名称投影。
    #[serde(default)]
    pub current_node_name: Option<String>,
    /// 当前审批人投影。
    #[serde(default)]
    pub current_assignee_participant_id: Option<String>,
    /// 当前审批人显示名投影。
    #[serde(default)]
    pub current_assignee_name: Option<String>,
    /// 最近驳回执行投影。
    #[serde(default)]
    pub latest_rejected_execution_id: Option<String>,
    /// 最近驳回摘要投影。
    #[serde(default)]
    pub latest_rejection_summary: Option<String>,
    /// 最近状态变更时间投影。
    #[serde(default)]
    pub last_status_changed_at: Option<i64>,
    /// 启动人。
    pub started_by: String,
    /// 启动时间。
    pub started_at: i64,
    /// 阻塞时间。
    pub blocked_at: Option<i64>,
    /// 乐观锁版本。
    pub version: u64,
    /// 最近更新时间。
    pub updated_at: u64,
}

/// 最高定义业务版本查询使用的最小持久化投影。
#[derive(Debug, Deserialize)]
struct LatestDefinitionVersionProjection {
    /// 已持久化的定义业务版本。
    definition_version: u32,
}

/// 目录查询返回的流程种类发布/草稿版本事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionCatalogStatusFact {
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 当前已发布业务版本；无发布时为空。
    pub published_version: Option<u32>,
    /// 当前活动草稿业务版本；无草稿时为空。
    pub draft_version: Option<u32>,
}

/// 目录批量查询使用的最小持久化投影。
#[derive(Debug, Deserialize)]
struct DefinitionCatalogRow {
    process_kind: ProcessKind,
    status: ApprovalDefinitionStatus,
    definition_version: u32,
}

/// CAS 替换写入所需的集合、过滤条件与待写入实体。
struct CasReplaceSpec<'a, T> {
    collection: &'a str,
    filter: Document,
    entity: &'a T,
    expected_version: u64,
    extra_set: Option<Document>,
}

/// 跨 BPM 目标集合的聚合仓储。
pub struct BpmWorkflowRepository<'a> {
    db: &'a Database,
}

impl<'a> BpmWorkflowRepository<'a> {
    /// 创建 BPM 聚合仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回不自行开事务的聚合仓储。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    fn definitions(&self) -> Repository<'a, ApprovalProcessDefinition> {
        Repository::new(self.db, DEFINITIONS)
    }

    fn instances(&self) -> Repository<'a, ApprovalProcessInstance> {
        Repository::new(self.db, INSTANCES)
    }

    fn executions(&self) -> Repository<'a, ApprovalNodeExecution> {
        Repository::new(self.db, EXECUTIONS)
    }

    fn receipts(&self) -> Repository<'a, ApprovalCommandReceipt> {
        Repository::new(self.db, RECEIPTS)
    }
}

mod cas;
mod definition_command;
mod definition_query;
mod execution_query;
mod instance_query;
mod runtime_write;

pub use cas::{
    approval_task_cas_filter, assign_document_no_filter, classify_assign_document_no_miss, classify_cas_miss,
};
pub(crate) use instance_query::{
    instance_cursor_or, instance_list_filter_doc, instance_list_limit, instance_list_scope_empty,
    instance_list_sort, instance_summary_projection,
};

async fn find_limited<T>(
    collection: &mongodb::Collection<T>,
    filter: Document,
    sort: Document,
    limit: i64,
    executor: &mut dyn Executor,
) -> Result<Vec<T>>
where
    T: for<'de> Deserialize<'de> + Send + Sync,
{
    let options = FindOptions::builder().sort(sort).limit(limit).build();
    mongo_ops::find_many(collection, filter, options, executor).await
}

fn merge_documents(target: &mut Document, extra: Document) {
    for (key, value) in extra {
        target.insert(key, value);
    }
}

fn clamp_limit(limit: u32, max: i64) -> i64 {
    if limit == 0 {
        return 1;
    }
    i64::from(limit).min(max)
}

fn i64_version(version: u64) -> Result<i64> {
    i64::try_from(version).map_err(|_| Error::EntityMetadataOutOfRange("version"))
}

#[cfg(test)]
mod tests {
    use super::cas::execution_end_filter;
    use super::definition_query::{
        definition_catalog_filter, definition_catalog_options, definition_child_filter,
        definition_graph_transition_limit, definition_versions_filter, group_definition_catalog_rows,
        latest_definition_version_from_rows, latest_definition_version_options, published_kind_docs_filter,
        unique_process_kinds, unique_published_definition,
    };
    use super::execution_query::{
        current_execution_filter, execution_history_filter, execution_history_limit,
    };
    use super::instance_query::{
        cancellation_subject_filter, instance_text_query_or, latest_subject_filter,
        non_terminal_subject_filter,
    };
    use super::runtime_write::{
        cancellable_execution_end_filter, cancelled_instance_projection, instance_advance_filter,
        instance_insert_document, previous_version, receipt_key_filter, require_cas_applied,
    };
    use super::{
        approval_task_cas_filter, assign_document_no_filter, clamp_limit, classify_assign_document_no_miss,
        classify_cas_miss, instance_list_filter_doc, instance_list_scope_empty, instance_list_sort,
        instance_summary_projection, merge_documents, ApprovalInstanceListCursor, ApprovalInstanceListFilter,
        ApprovalInstanceListProjection, ApprovalInstanceListView, ApprovalInstanceTextQuery,
        AssignDocumentNoOutcome, CasWriteOutcome, DefinitionCatalogRow, DefinitionCatalogStatusFact,
        LatestDefinitionVersionProjection, MAX_CATALOG_STATUS_ROWS, MAX_DEFINITION_GRAPH_DOCS,
        MAX_EXECUTION_HISTORY, MAX_INSTANCE_PAGE,
    };
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::{
        ApprovalCommandKind, ApprovalDefinitionStatus, ApprovalNodeExecutionStatus,
        ApprovalProcessInstanceStatus,
    };
    use bpm::model::{ApprovalProcessInstance, IdempotencyKey, NewProcessInstance, ParticipantId, Timestamp};
    use bpm::{ProcessKind, SubjectRef};
    use entity_core::{BaseModel, HasBaseModel};
    use mongodb::bson::{doc, serialize_to_document, Bson};

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
        assert_eq!(
            options.projection,
            Some(doc! { "definition_version": 1, "_id": 0 })
        );
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
            filter
                .get_document("process_kind")
                .unwrap()
                .get_array("$in")
                .unwrap(),
            &vec![
                Bson::String("sales_order".into()),
                Bson::String("stock_adjustment".into())
            ]
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
        assert!(group_definition_catalog_rows(
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
        .is_err());
        assert!(group_definition_catalog_rows(
            &[ProcessKind::SalesOrder],
            vec![DefinitionCatalogRow {
                process_kind: ProcessKind::SalesOrder,
                status: ApprovalDefinitionStatus::Retired,
                definition_version: 1,
            }],
        )
        .is_err());
        assert!(unique_published_definition(Vec::new()).unwrap().is_none());
        let mut published_a = dummy_definition("a");
        published_a
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        let mut published_b = dummy_definition("b");
        published_b
            .publish(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        assert_eq!(
            unique_published_definition(vec![published_a.clone()])
                .unwrap()
                .unwrap()
                .base
                .id,
            "a"
        );
        assert!(unique_published_definition(vec![published_a, published_b]).is_err());
        let catalog_src = include_str!("bpm/definition_query.rs")
            .split("pub async fn definition_catalog_facts")
            .nth(1)
            .and_then(|body| body.split("pub async fn load_published_definition_graph").next())
            .expect("目录查询");
        assert_eq!(catalog_src.matches("find_many").count(), 1);
        assert!(catalog_src.contains("unique_kinds.is_empty()"));
        let published_src = include_str!("bpm/definition_query.rs")
            .split("pub async fn load_published_definition_graph")
            .nth(1)
            .and_then(|body| body.split("async fn load_definition_nodes").next())
            .expect("发布图加载");
        assert!(published_src.contains("unique_published_definition"));
        assert!(published_src.contains("load_definition_nodes"));
        assert!(published_src.contains("load_definition_transitions"));
        assert!(!published_src.contains("find_by_id"));
        assert!(!published_src.contains("load_definition_graph("));
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
        assert_eq!(
            filter.get_str("subject.subject_kind").unwrap(),
            "stock_adjustment"
        );
        assert_eq!(
            filter.get_document("status").unwrap(),
            &doc! { "$in": ["RUNNING", "BLOCKED"] }
        );
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
        assert_eq!(
            advance.get_document("status").unwrap(),
            &doc! { "$in": ["RUNNING", "BLOCKED"] }
        );
        let ended = execution_end_filter("exec-1", 2, ApprovalNodeExecutionStatus::Active).unwrap();
        assert_eq!(ended.get_str("status").unwrap(), "ACTIVE");
        let blocked = execution_end_filter("exec-1", 2, ApprovalNodeExecutionStatus::Blocked).unwrap();
        assert_eq!(blocked.get_str("status").unwrap(), "BLOCKED");
        assert_eq!(blocked.get_i64("version").unwrap(), 2);
        let current = current_execution_filter(&ApprovalProcessInstanceId::new("inst-1"));
        assert_eq!(
            current.get_document("status").unwrap(),
            &doc! { "$in": ["ACTIVE", "BLOCKED"] }
        );
    }

    /// 取消执行 CAS 同时允许活动和受阻状态，并保持版本与软删除约束。
    ///
    /// 其他结束状态不得进入取消写入过滤条件。
    #[test]
    fn cancellable_execution_filter_accepts_current_states_only() {
        let filter = cancellable_execution_end_filter("exec-1", 2).unwrap();
        assert_eq!(filter.get_i64("version").unwrap(), 2);
        assert_eq!(
            filter.get_document("status").unwrap(),
            &doc! { "$in": ["ACTIVE", "BLOCKED"] }
        );
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
        assert_eq!(
            serialize_to_document(&projection)
                .unwrap()
                .get_str("current_node_name")
                .unwrap(),
            "仓储复核"
        );
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
            &vec![
                Bson::Document(doc! { "document_no": "" }),
                Bson::Document(doc! { "document_no": Bson::Null }),
            ]
        );

        assert!(matches!(
            classify_assign_document_no_miss(
                None::<(u64, String)>,
                1,
                "SO-1",
                |row| row.0,
                |row| row.1.as_str()
            ),
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
            subject_ids: Some(vec!["adj-1".into()]),
            text_query: None,
            cursor: Some(ApprovalInstanceListCursor {
                sort_time: 10,
                id: "inst-9".into(),
            }),
            limit: 20,
        };
        let document = instance_list_filter_doc(&managed);
        assert_eq!(document.get_str("process_kind").unwrap(), "stock_adjustment");
        assert_eq!(document.get_str("status").unwrap(), "RUNNING");
        assert_eq!(
            instance_list_sort(&managed),
            doc! { "status": 1, "updated_at": -1, "id": -1 }
        );
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
            subject_ids: None,
            text_query: None,
            cursor: Some(ApprovalInstanceListCursor {
                sort_time: 20,
                id: "inst-2".into(),
            }),
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
            subject_ids: None,
            text_query: None,
            cursor: Some(ApprovalInstanceListCursor {
                sort_time: 8,
                id: "inst-3".into(),
            }),
            limit: 20,
        };
        let managed_open_doc = instance_list_filter_doc(&managed_open);
        assert!(!managed_open_doc.contains_key("status"));
        assert_eq!(
            instance_list_sort(&managed_open),
            doc! { "updated_at": -1, "id": -1 }
        );
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
            subject_ids: None,
            text_query: None,
            cursor: Some(ApprovalInstanceListCursor {
                sort_time: 4,
                id: "inst-4".into(),
            }),
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
            subject_ids: Some(Vec::new()),
            text_query: None,
            cursor: None,
            limit: 20,
        };
        assert!(instance_list_scope_empty(&empty_scope));
        assert!(!instance_list_scope_empty(&started));
        assert_eq!(
            instance_list_sort(&empty_scope),
            doc! { "blocked_at": -1, "id": -1 }
        );
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
            subject_ids: None,
            text_query: None,
            cursor: None,
            limit: 20,
        };
        assert!(instance_list_scope_empty(&missing_starter));

        let empty_starter = ApprovalInstanceListFilter {
            started_by: Some(String::new()),
            ..missing_starter.clone()
        };
        assert!(instance_list_scope_empty(&empty_starter));

        let kind_only = ApprovalInstanceListFilter {
            view: ApprovalInstanceListView::Started,
            process_kind: Some(ProcessKind::StockAdjustment),
            status: None,
            started_by: Some("u1".into()),
            subject_kind: None,
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
        assert_eq!(
            instance_list_sort(&kind_only),
            doc! { "started_at": -1, "id": -1 }
        );

        let status_only = ApprovalInstanceListFilter {
            view: ApprovalInstanceListView::Started,
            process_kind: None,
            status: Some(ApprovalProcessInstanceStatus::Running),
            started_by: Some("u1".into()),
            subject_kind: None,
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
        assert_eq!(
            instance_list_sort(&status_only),
            doc! { "started_at": -1, "id": -1 }
        );
    }

    /// 字面量检索转义正则，并与游标 `$or` 用 `$and` 组合。
    #[test]
    fn instance_list_text_query_is_literal_and_composes_with_cursor() {
        let text_query = ApprovalInstanceTextQuery {
            query: "SO.[1]".to_string(),
        };
        let alternatives = instance_text_query_or(&text_query);
        assert_eq!(alternatives.len(), 3);
        let regex = alternatives[0]
            .get_document("subject.subject_id")
            .unwrap()
            .get_str("$regex")
            .unwrap();
        assert_eq!(regex, r"SO\.\[1\]");
        let with_cursor = ApprovalInstanceListFilter {
            view: ApprovalInstanceListView::Started,
            process_kind: None,
            status: None,
            started_by: Some("u1".into()),
            subject_kind: None,
            subject_ids: None,
            text_query: Some(text_query.clone()),
            cursor: Some(ApprovalInstanceListCursor {
                sort_time: 20,
                id: "inst-2".into(),
            }),
            limit: 20,
        };
        let document = instance_list_filter_doc(&with_cursor);
        let and = document.get_array("$and").unwrap();
        assert_eq!(and.len(), 2);
        assert!(and[0].as_document().unwrap().contains_key("$or"));
        assert!(and[1].as_document().unwrap().contains_key("$or"));
        assert_eq!(document.get_str("started_by").unwrap(), "u1");
        assert!(!document.contains_key("$or"));

        let query_only = ApprovalInstanceListFilter {
            cursor: None,
            ..with_cursor
        };
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
        assert_eq!(
            next_page.get_document("execution_no").unwrap(),
            &doc! { "$gt": 7_i64 }
        );
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
        assert_eq!(
            definition_graph_transition_limit(),
            MAX_DEFINITION_GRAPH_DOCS.saturating_mul(2)
        );
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
}

#[cfg(test)]
mod mongo_catalog_and_published {
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
}
