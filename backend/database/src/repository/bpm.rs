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
use persistence_core::Executor;
use persistence_core::{mongo_ops, Error, Result};

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
mod mongo_catalog_and_published;
#[cfg(test)]
mod tests;
