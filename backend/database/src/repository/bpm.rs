//! BPM 模型仓储：目标集合映射、有界查询与带状态条件的 CAS。
//!
//! 本模块只读写 `bpm` 模型，不接收 ERP 实体，也不调用 BPM 决策函数。

pub use bpm::engine::DefinitionGraph;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
use bpm::model::types::{
    ApprovalCommandKind, ApprovalDefinitionStatus, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeDefinition, ApprovalNodeExecution,
    ApprovalProcessDefinition, ApprovalProcessInstance, ApprovalTransitionDefinition, IdempotencyKey,
};
use bpm::{ProcessKind, SubjectRef};
use entity_core::{HasBaseModel, NOT_DELETED_TIMESTAMP_BSON};
use mongodb::bson::{doc, serialize_to_document, Document};
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

    /// 按主键读取审批流程实例。
    ///
    /// # 参数
    /// * `instance_id` - 审批流程实例 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的实例；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本方法只读取实例身份，不放宽状态或主体约束。
    pub async fn find_instance_by_id(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        self.instances().find_by_id(instance_id.as_ref(), executor).await
    }

    /// 按主键读取审批节点执行。
    ///
    /// # 参数
    /// * `execution_id` - 审批节点执行 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的节点执行；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本方法不推断当前执行，调用方需要当前令牌时应使用 `find_current_execution`。
    pub async fn find_execution_by_id(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions()
            .find_by_id(execution_id.as_ref(), executor)
            .await
    }

    /// 按主键批量读取审批节点执行。
    ///
    /// # 参数
    /// * `execution_ids` - 已授权工作项引用的审批节点执行 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的节点执行；不存在的 ID 不产生占位记录。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 本方法只批量解析工作项已经持有的运行身份，不按审批人扩大查询范围。
    pub async fn list_executions_by_ids(
        &self,
        execution_ids: &[ApprovalNodeExecutionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        if execution_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = execution_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.executions()
            .find_many(doc! { "id": { "$in": ids } }, executor)
            .await
    }

    /// 查询实例指定节点的当前审批人绑定。
    ///
    /// # 参数
    /// * `instance_id` - 审批流程实例 ID
    /// * `node_key` - 定义内节点键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配且未软删除的实例审批人绑定；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// `(process_instance_id, node_key)` 由唯一索引保证单值语义。
    pub async fn find_assignee_for_node(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        node_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalInstanceAssignee>> {
        mongo_ops::find_one(
            &self.db.collection(ASSIGNEES),
            doc! {
                "process_instance_id": instance_id.as_ref(),
                "node_key": node_key,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 判断业务主体是否已经启动过审批实例。
    ///
    /// # 参数
    /// * `subject` - 业务对象稳定引用
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 任意未删除审批实例命中主体时返回 `true`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 终态实例仍证明单据已经启动过审批，因此本查询不附加状态条件。
    pub async fn has_started_instance_for_subject(
        &self,
        subject: &SubjectRef,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        mongo_ops::exists(
            &self.db.collection::<Document>(INSTANCES),
            doc! {
                "subject.subject_kind": subject.subject_kind(),
                "subject.subject_id": subject.subject_id(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    /// 查询同一流程种类当前唯一已发布定义。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_published_by_process_kind(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessDefinition>> {
        self.definitions()
            .find_one(published_kind_filter(process_kind), executor)
            .await
    }

    /// 读取同一流程种类未删除定义的最高持久化业务版本。
    ///
    /// # 参数
    /// * `process_kind` - 需要读取版本事实的流程种类
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按业务版本倒序命中的首个版本；没有历史定义时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或投影反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 查询只投影版本字段并限制一条，不分配或递增版本，且必须保留软删除过滤。
    pub async fn latest_definition_version(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        let options = latest_definition_version_options();
        let rows = mongo_ops::find_many(
            &self
                .db
                .collection::<LatestDefinitionVersionProjection>(DEFINITIONS),
            definition_versions_filter(process_kind),
            options,
            executor,
        )
        .await?;
        Ok(latest_definition_version_from_rows(rows))
    }

    /// 列出同一流程种类的历史定义版本，按业务版本倒序且有上限。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_definition_versions(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalProcessDefinition>> {
        find_limited(
            &self.db.collection(DEFINITIONS),
            definition_versions_filter(process_kind),
            definition_versions_sort(),
            definition_versions_limit(MAX_DEFINITION_VERSIONS as u32),
            executor,
        )
        .await
    }

    /// 查询同一流程种类当前活动草稿。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_active_draft(
        &self,
        process_kind: ProcessKind,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessDefinition>> {
        self.definitions()
            .find_one(active_draft_filter(process_kind), executor)
            .await
    }

    /// 批量读取定义及其节点、连线，禁止按节点 N+1。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn load_definition_graph(
        &self,
        definition_id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<DefinitionGraph>> {
        let Some(definition) = self
            .definitions()
            .find_by_id(definition_id.as_ref(), executor)
            .await?
        else {
            return Ok(None);
        };
        let nodes = self.load_definition_nodes(definition_id, executor).await?;
        let transitions = self.load_definition_transitions(definition_id, executor).await?;
        Ok(Some(DefinitionGraph {
            definition,
            nodes,
            transitions,
        }))
    }

    /// 以 `id + DRAFT + expected_definition_lock_version` 更新草稿定义字段。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn update_draft_definition(
        &self,
        definition: &ApprovalProcessDefinition,
        expected_definition_lock_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        self.cas_write_definition(
            definition,
            expected_definition_lock_version,
            &[ApprovalDefinitionStatus::Draft],
            executor,
        )
        .await
    }

    /// 以 `id + DRAFT + expected_definition_lock_version` 整组替换草稿图。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn replace_draft_graph(
        &self,
        definition: &ApprovalProcessDefinition,
        nodes: &[ApprovalNodeDefinition],
        transitions: &[ApprovalTransitionDefinition],
        expected_definition_lock_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        let outcome = self
            .cas_write_definition(
                definition,
                expected_definition_lock_version,
                &[ApprovalDefinitionStatus::Draft],
                executor,
            )
            .await?;
        if !matches!(outcome, CasWriteOutcome::Applied(_)) {
            return Ok(outcome);
        }
        self.replace_graph_docs(&definition.base.id, nodes, transitions, executor)
            .await?;
        Ok(outcome)
    }

    /// 先退役旧发布版本，再把草稿发布为当前唯一 `PUBLISHED`。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn publish_and_retire_previous(
        &self,
        definition: &ApprovalProcessDefinition,
        previous: Option<&ApprovalProcessDefinition>,
        expected_definition_lock_version: u64,
        expected_previous_lock_version: Option<u64>,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        if let Some(previous) = previous {
            let expected = expected_previous_lock_version.unwrap_or(previous.definition_lock_version());
            let retired = self
                .cas_write_definition(
                    previous,
                    expected,
                    &[ApprovalDefinitionStatus::Published],
                    executor,
                )
                .await?;
            if !matches!(retired, CasWriteOutcome::Applied(_)) {
                return Ok(retired);
            }
        }
        self.cas_write_definition(
            definition,
            expected_definition_lock_version,
            &[ApprovalDefinitionStatus::Draft],
            executor,
        )
        .await
    }

    /// 查询同一主体与提交版本的非终态实例。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_non_terminal_by_subject(
        &self,
        subject: &SubjectRef,
        subject_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        self.instances()
            .find_one(non_terminal_subject_filter(subject, subject_version), executor)
            .await
    }

    /// 读取同一主体与提交版本的可取消实例候选。
    ///
    /// 本查询按同一主体与提交版本读取最近实例，不预先过滤状态；实例是否可取消以及
    /// 开放任务数量是否一致，统一由 BPM 模型在全部事实加载后判定。
    ///
    /// # 参数
    /// * `subject` - 业务对象引用
    /// * `subject_version` - 冻结提交版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回可供取消规则校验的实例候选；没有时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn cancellation_instance_by_subject(
        &self,
        subject: &SubjectRef,
        subject_version: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        let mut rows = find_limited(
            &self.db.collection(INSTANCES),
            cancellation_subject_filter(subject, subject_version),
            doc! { "started_at": -1, "id": -1 },
            1,
            executor,
        )
        .await?;
        Ok(rows.pop())
    }

    /// 按主体读取最近一条审批实例，含已终态，供单据详情投影。
    ///
    /// 命中 `idx_approval_process_instances_subject_history`。撤回命令应使用
    /// `cancellation_instance_by_subject`，不得用本方法替代提交版本与状态校验。
    ///
    /// # 参数
    /// * `subject` - 业务对象引用
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 按 `started_at desc, id desc` 的最新实例；没有时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_latest_by_subject(
        &self,
        subject: &SubjectRef,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalProcessInstance>> {
        let mut rows = find_limited(
            &self.db.collection(INSTANCES),
            latest_subject_filter(subject),
            doc! { "started_at": -1, "id": -1 },
            1,
            executor,
        )
        .await?;
        Ok(rows.pop())
    }

    /// 查询实例当前 `ACTIVE|BLOCKED` 执行。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_current_execution(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions()
            .find_one(current_execution_filter(instance_id), executor)
            .await
    }

    /// 读取取消用例所需的当前活动或受阻执行。
    ///
    /// # 参数
    /// * `instance_id` - 可取消实例主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回该实例当前 `ACTIVE|BLOCKED` 执行；缺失时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn current_execution_for_cancellation(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalNodeExecution>> {
        self.executions()
            .find_one(current_execution_filter(instance_id), executor)
            .await
    }

    /// 按执行序号稳定游标读取实例历史，单次不超过上限。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_execution_history(
        &self,
        instance_id: &ApprovalProcessInstanceId,
        after_execution_no: Option<u32>,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeExecution>> {
        find_limited(
            &self.db.collection(EXECUTIONS),
            execution_history_filter(instance_id, after_execution_no),
            doc! { "execution_no": 1 },
            execution_history_limit(limit),
            executor,
        )
        .await
    }

    /// 只写入 BPM 运行事实：实例、审批人、首个执行和命令收据。
    ///
    /// 实例插入必须同时写入有界列表投影；列表不得再扫执行历史补全当前节点、
    /// 当前审批人、最近驳回与最近状态变更时间。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create_bpm_runtime(
        &self,
        instance: &ApprovalProcessInstance,
        assignees: &[ApprovalInstanceAssignee],
        first_execution: &ApprovalNodeExecution,
        receipt: &ApprovalCommandReceipt,
        list_projection: &ApprovalInstanceListProjection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.create_bpm_runtime_after_receipt(
            instance,
            assignees,
            first_execution,
            list_projection,
            executor,
        )
        .await?;
        self.insert_command_receipt(receipt, executor).await
    }

    /// 在命令收据已先行仲裁后写入启动实例、审批人绑定和首个执行。
    ///
    /// 本方法不写收据、不创建事务。要求并发启动以收据作为第一写的调用方，
    /// 必须先在同一事务调用 [`Self::insert_command_receipt`]，再调用本方法。
    ///
    /// # 错误
    /// 任一运行集合插入失败时返回错误，调用方事务必须整体回滚。
    pub async fn create_bpm_runtime_after_receipt(
        &self,
        instance: &ApprovalProcessInstance,
        assignees: &[ApprovalInstanceAssignee],
        first_execution: &ApprovalNodeExecution,
        list_projection: &ApprovalInstanceListProjection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection(INSTANCES),
            &instance_insert_document(instance, list_projection)?,
            executor,
        )
        .await?;
        mongo_ops::insert_many(&self.db.collection(ASSIGNEES), assignees.to_vec(), executor).await?;
        mongo_ops::insert_one(&self.db.collection(EXECUTIONS), first_execution, executor).await
    }

    /// 按视图、状态和 DataScope 过滤分页读取实例摘要。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_instance_summaries(
        &self,
        filter: &ApprovalInstanceListFilter,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalInstanceSummary>> {
        if instance_list_scope_empty(filter) {
            return Ok(Vec::new());
        }
        let options = FindOptions::builder()
            .sort(instance_list_sort(filter))
            .limit(instance_list_limit(filter.limit))
            .projection(instance_summary_projection())
            .build();
        mongo_ops::find_many(
            &self.db.collection::<ApprovalInstanceSummary>(INSTANCES),
            instance_list_filter_doc(filter),
            options,
            executor,
        )
        .await
    }

    /// 统计当前授权过滤条件下的审批实例总数。
    ///
    /// 游标只限定当前页起点，不得缩小总数口径，因此统计时固定移除游标。
    ///
    /// # 参数
    /// * `filter` - 与列表相同的视图、状态、启动人和对象范围
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前过滤条件下的完整实例数量。
    ///
    /// # 错误
    /// MongoDB 统计失败时返回错误。
    pub async fn count_instance_summaries(
        &self,
        filter: &ApprovalInstanceListFilter,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        if instance_list_scope_empty(filter) {
            return Ok(0);
        }
        let mut total_filter = filter.clone();
        total_filter.cursor = None;
        mongo_ops::count_documents(
            &self.db.collection::<ApprovalInstanceSummary>(INSTANCES),
            instance_list_filter_doc(&total_filter),
            executor,
        )
        .await
    }

    /// 按实例主键批量读取审批运行的有界列表投影。
    ///
    /// # 参数
    /// * `instance_ids` - 已由工作项节点执行解析出的审批实例 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前节点、当前审批人、最近驳回摘要和实例状态；不扫描执行历史。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 关键业务约束
    /// 只允许使用有界实例投影补充工作台判断事实，不得为列表逐实例查询历史。
    pub async fn list_instance_summaries_by_ids(
        &self,
        instance_ids: &[ApprovalProcessInstanceId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalInstanceSummary>> {
        if instance_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids = instance_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let options = FindOptions::builder()
            .projection(instance_summary_projection())
            .build();
        mongo_ops::find_many(
            &self.db.collection::<ApprovalInstanceSummary>(INSTANCES),
            doc! {
                "id": { "$in": ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }

    /// 按幂等键读取命令收据。
    ///
    /// `scope_id` 只允许调用方传入当前 v3 scope 或其命令协议显式登记的历史
    /// scope；幂等键必须先形成 [`IdempotencyKey`]，仓储不接受或二次规范 raw
    /// 输入。未命中返回 `Ok(None)`，唯一索引竞争后的恢复读取必须由 Service 在
    /// 可用的新会话中编排。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_command_receipt(
        &self,
        command_kind: ApprovalCommandKind,
        scope_id: &str,
        idempotency_key: &IdempotencyKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<ApprovalCommandReceipt>> {
        self.receipts()
            .find_one(
                receipt_key_filter(command_kind, scope_id, idempotency_key),
                executor,
            )
            .await
    }

    /// 持久化已由引擎形成的取消实例、结束执行和命令收据。
    ///
    /// 本方法不创建事务；调用方必须传入与业务单据、任务关闭相同的事务执行器。
    /// 实例和执行均按引擎自增后的版本反推加载版本，并以当前令牌和
    /// `ACTIVE|BLOCKED` 状态执行 CAS。
    ///
    /// # 参数
    /// * `instance` - 已进入 `CANCELLED` 的实例快照
    /// * `updated_executions` - 已随实例取消结束的当前执行集合
    /// * `receipt` - 本次取消命令收据
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部 BPM 运行事实写入成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 取消计划缺少执行、版本元数据非法、CAS 未命中或 MongoDB 写入失败时返回错误。
    pub async fn persist_cancelled_runtime(
        &self,
        instance: &ApprovalProcessInstance,
        updated_executions: &[ApprovalNodeExecution],
        receipt: &ApprovalCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.persist_cancelled_runtime_after_receipt(instance, updated_executions, executor)
            .await?;
        self.insert_command_receipt(receipt, executor).await
    }

    /// 在命令收据已先行仲裁后持久化取消实例与结束执行。
    ///
    /// 本方法不写命令收据、不创建事务。需要以唯一收据作为并发仲裁点的调用方
    /// 必须先在同一事务内调用 [`Self::insert_command_receipt`]，再调用本方法，
    /// 最后写业务单据、任务、通知和审计。旧调用方继续使用
    /// [`Self::persist_cancelled_runtime`]，其行为保持不变。
    ///
    /// # 错误
    /// 取消计划缺少执行、版本元数据非法、CAS 未命中或 MongoDB 写入失败时返回错误。
    pub async fn persist_cancelled_runtime_after_receipt(
        &self,
        instance: &ApprovalProcessInstance,
        updated_executions: &[ApprovalNodeExecution],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = updated_executions
            .first()
            .ok_or(Error::EntityMetadataOutOfRange("cancelled_execution"))?;
        let current_id = ApprovalNodeExecutionId::new(current.base.id.clone());
        let expected_instance_version = previous_version(instance.base.version)?;
        require_cas_applied(
            self.advance_instance(
                instance,
                expected_instance_version,
                &current_id,
                &cancelled_instance_projection(instance),
                executor,
            )
            .await?,
        )?;
        for execution in updated_executions {
            let expected_execution_version = previous_version(execution.base.version)?;
            require_cas_applied(
                self.end_cancellable_execution(execution, expected_execution_version, executor)
                    .await?,
            )?;
        }
        Ok(())
    }

    /// 以 `id + expected_instance_version + current_execution_id + RUNNING|BLOCKED` 推进实例。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn advance_instance(
        &self,
        instance: &ApprovalProcessInstance,
        expected_instance_version: u64,
        expected_current_execution_id: &ApprovalNodeExecutionId,
        list_projection: &ApprovalInstanceListProjection,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessInstance>> {
        let filter = instance_advance_filter(
            &instance.base.id,
            expected_instance_version,
            expected_current_execution_id,
        )?;
        self.cas_replace(
            CasReplaceSpec {
                collection: INSTANCES,
                filter,
                entity: instance,
                expected_version: expected_instance_version,
                extra_set: Some(serialize_to_document(list_projection)?),
            },
            |current| {
                matches!(
                    current.status,
                    ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked
                ) && current.current_node_execution_id.as_ref() == Some(expected_current_execution_id)
            },
            executor,
        )
        .await
    }

    /// 以 `id + expected_execution_version + ACTIVE` 结束活动执行。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    pub async fn end_active_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        self.cas_end_execution(
            execution,
            expected_execution_version,
            ApprovalNodeExecutionStatus::Active,
            executor,
        )
        .await
    }

    /// 以 `id + expected_execution_version + BLOCKED` 结束受阻执行。
    ///
    /// # 参数
    /// * `execution` - 已由恢复引擎置为 `SUPERSEDED` 的旧执行
    /// * `expected_execution_version` - 调用方持有的受阻执行版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回应用、缺失、版本冲突或状态变化的 CAS 分类。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅人员恢复可使用本端口；不得接受活动执行或任意当前状态。
    pub async fn end_blocked_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        self.cas_end_execution(
            execution,
            expected_execution_version,
            ApprovalNodeExecutionStatus::Blocked,
            executor,
        )
        .await
    }

    /// 以 `id + expected_execution_version + ACTIVE|BLOCKED` 结束可取消执行。
    ///
    /// # 参数
    /// * `execution` - 已由引擎置为 `CANCELLED` 的当前执行
    /// * `expected_execution_version` - 引擎变更前的加载版本
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回应用、缺失、版本冲突或状态变化的 CAS 分类。
    ///
    /// # 错误
    /// 元数据越界或 MongoDB 写入失败时返回错误。
    async fn end_cancellable_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        let filter = cancellable_execution_end_filter(&execution.base.id, expected_execution_version)?;
        self.cas_replace(
            CasReplaceSpec {
                collection: EXECUTIONS,
                filter,
                entity: execution,
                expected_version: expected_execution_version,
                extra_set: None,
            },
            |current| current.status.is_current(),
            executor,
        )
        .await
    }

    /// 插入新的节点执行。原审批人恢复不得更新旧 `CLOSED` 任务对应的旧执行。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn insert_execution(
        &self,
        execution: &ApprovalNodeExecution,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection(EXECUTIONS), execution, executor).await
    }

    /// 启动实例时一次性冻结全部节点审批人绑定；运行时不得更新或追加。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn insert_assignees(
        &self,
        assignees: &[ApprovalInstanceAssignee],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_many(&self.db.collection(ASSIGNEES), assignees.to_vec(), executor).await
    }

    /// 插入命令收据。唯一键冲突由调用方按同/异载荷回读分类。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn insert_command_receipt(
        &self,
        receipt: &ApprovalCommandReceipt,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection(RECEIPTS), receipt, executor).await
    }

    async fn load_definition_nodes(
        &self,
        definition_id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalNodeDefinition>> {
        find_limited(
            &self.db.collection(NODE_DEFINITIONS),
            definition_child_filter(definition_id),
            doc! { "display_order": 1 },
            MAX_DEFINITION_GRAPH_DOCS,
            executor,
        )
        .await
    }

    async fn load_definition_transitions(
        &self,
        definition_id: &ApprovalProcessDefinitionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ApprovalTransitionDefinition>> {
        find_limited(
            &self.db.collection(TRANSITION_DEFINITIONS),
            definition_child_filter(definition_id),
            doc! { "from_node_key": 1, "event": 1 },
            definition_graph_transition_limit(),
            executor,
        )
        .await
    }

    async fn replace_graph_docs(
        &self,
        definition_id: &str,
        nodes: &[ApprovalNodeDefinition],
        transitions: &[ApprovalTransitionDefinition],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let filter = doc! { "process_definition_id": definition_id };
        mongo_ops::delete_many(
            &self.db.collection::<ApprovalNodeDefinition>(NODE_DEFINITIONS),
            filter.clone(),
            executor,
        )
        .await?;
        mongo_ops::delete_many(
            &self
                .db
                .collection::<ApprovalTransitionDefinition>(TRANSITION_DEFINITIONS),
            filter,
            executor,
        )
        .await?;
        mongo_ops::insert_many(&self.db.collection(NODE_DEFINITIONS), nodes.to_vec(), executor).await?;
        mongo_ops::insert_many(
            &self.db.collection(TRANSITION_DEFINITIONS),
            transitions.to_vec(),
            executor,
        )
        .await
    }

    async fn cas_write_definition(
        &self,
        definition: &ApprovalProcessDefinition,
        expected_lock_version: u64,
        required_status: &[ApprovalDefinitionStatus],
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalProcessDefinition>> {
        let filter = draft_or_status_filter(&definition.base.id, expected_lock_version, required_status)?;
        let required = required_status.to_vec();
        self.cas_replace(
            CasReplaceSpec {
                collection: DEFINITIONS,
                filter,
                entity: definition,
                expected_version: expected_lock_version,
                extra_set: None,
            },
            move |current| required.contains(&current.status),
            executor,
        )
        .await
    }

    async fn cas_end_execution(
        &self,
        execution: &ApprovalNodeExecution,
        expected_execution_version: u64,
        required_status: ApprovalNodeExecutionStatus,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<ApprovalNodeExecution>> {
        let filter = execution_end_filter(&execution.base.id, expected_execution_version, required_status)?;
        self.cas_replace(
            CasReplaceSpec {
                collection: EXECUTIONS,
                filter,
                entity: execution,
                expected_version: expected_execution_version,
                extra_set: None,
            },
            move |current| current.status == required_status,
            executor,
        )
        .await
    }

    async fn cas_replace<T, F>(
        &self,
        spec: CasReplaceSpec<'_, T>,
        status_matches: F,
        executor: &mut dyn Executor,
    ) -> Result<CasWriteOutcome<T>>
    where
        T: Serialize + for<'de> Deserialize<'de> + HasBaseModel + Clone + Send + Sync,
        F: Fn(&T) -> bool,
    {
        let next_version = next_version_i64(spec.expected_version)?;
        let mut set_doc = serialize_to_document(spec.entity)?;
        set_doc.insert("version", next_version);
        if let Some(extra_set) = spec.extra_set {
            merge_documents(&mut set_doc, extra_set);
        }
        let matched = mongo_ops::update_one(
            &self.db.collection::<T>(spec.collection),
            spec.filter,
            doc! { "$set": set_doc },
            false,
            executor,
        )
        .await?
        .matched_count;
        if matched > 0 {
            let mut applied = spec.entity.clone();
            applied.base_mut().version = spec.expected_version.saturating_add(1);
            return Ok(CasWriteOutcome::Applied(applied));
        }
        let current = mongo_ops::find_one(
            &self.db.collection::<T>(spec.collection),
            doc! { "id": spec.entity.base().id.as_str(), "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await?;
        Ok(classify_cas_miss(current, spec.expected_version, status_matches))
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

fn published_kind_filter(process_kind: ProcessKind) -> Document {
    doc! {
        "process_kind": process_kind.as_str(),
        "status": ApprovalDefinitionStatus::Published.as_str(),
    }
}

fn active_draft_filter(process_kind: ProcessKind) -> Document {
    doc! {
        "process_kind": process_kind.as_str(),
        "status": ApprovalDefinitionStatus::Draft.as_str(),
    }
}

fn definition_child_filter(definition_id: &ApprovalProcessDefinitionId) -> Document {
    doc! {
        "process_definition_id": definition_id.as_ref(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构造定义历史查询条件，前缀对齐 `idx_approval_process_definitions_history`。
///
/// # 参数
/// * `process_kind` - 流程种类
///
/// # 返回
/// 返回含 `process_kind` 与软删除约束的查询文档。
fn definition_versions_filter(process_kind: ProcessKind) -> Document {
    doc! {
        "process_kind": process_kind.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 返回定义历史固定排序文档。
///
/// # 返回
/// 返回 `{ definition_version: -1 }`。
fn definition_versions_sort() -> Document {
    doc! { "definition_version": -1 }
}

/// 构建最高定义业务版本查询的限制与最小投影。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回按业务版本降序、限制一条且只读取版本字段的 MongoDB 查询选项。
///
/// # 错误
/// 不返回错误。
///
/// # 关键业务约束
/// 过滤条件由调用点单独提供；本选项不得读取完整定义或分配下一版本。
fn latest_definition_version_options() -> FindOptions {
    FindOptions::builder()
        .sort(definition_versions_sort())
        .limit(1)
        .projection(doc! { "definition_version": 1, "_id": 0 })
        .build()
}

/// 从已按最高版本查询返回的投影行中读取首个业务版本。
///
/// # 参数
/// * `rows` - MongoDB 按查询选项返回的零或一条版本投影
///
/// # 返回
/// 有结果时返回首条业务版本；无历史定义时返回 `None`。
///
/// # 错误
/// 不返回错误。
///
/// # 关键业务约束
/// 不在内存重新排序或选择最大值，保持 Repository 查询契约单一。
fn latest_definition_version_from_rows(rows: Vec<LatestDefinitionVersionProjection>) -> Option<u32> {
    rows.into_iter().next().map(|row| row.definition_version)
}

/// 将定义历史请求页大小夹紧到 `[1, MAX_DEFINITION_VERSIONS]`。
///
/// # 参数
/// * `limit` - 调用方请求条数
///
/// # 返回
/// 返回可交给 MongoDB `limit` 的有界整数。
fn definition_versions_limit(limit: u32) -> i64 {
    clamp_limit(limit, MAX_DEFINITION_VERSIONS)
}

/// 返回定义连线一次批量读取上限（节点上限的两倍）。
///
/// # 返回
/// 返回 `MAX_DEFINITION_GRAPH_DOCS.saturating_mul(2)`。
fn definition_graph_transition_limit() -> i64 {
    MAX_DEFINITION_GRAPH_DOCS.saturating_mul(2)
}

/// 构造实例执行历史的稳定游标过滤条件。
///
/// # 参数
/// * `instance_id` - 所属流程实例
/// * `after_execution_no` - 上一页最后一条执行序号；首页为空
///
/// # 返回
/// 返回含软删除约束、实例主键与可选 `execution_no $gt` 的查询文档。
fn execution_history_filter(
    instance_id: &ApprovalProcessInstanceId,
    after_execution_no: Option<u32>,
) -> Document {
    let mut filter = doc! {
        "process_instance_id": instance_id.as_ref(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    };
    if let Some(after_execution_no) = after_execution_no {
        filter.insert("execution_no", doc! { "$gt": i64::from(after_execution_no) });
    }
    filter
}

/// 将执行历史请求页大小夹紧到 `[1, MAX_EXECUTION_HISTORY]`。
///
/// # 参数
/// * `limit` - 调用方请求条数
///
/// # 返回
/// 返回可交给 MongoDB `limit` 的有界整数。
fn execution_history_limit(limit: u32) -> i64 {
    clamp_limit(limit, MAX_EXECUTION_HISTORY)
}

/// 构造取消候选的主体与提交版本过滤条件，不预先判断实例状态。
///
/// # 参数
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
///
/// # 返回
/// 返回主体、提交版本和软删除约束，供 BPM 模型统一判定是否可取消。
///
/// # 错误
/// 无。
fn cancellation_subject_filter(subject: &SubjectRef, subject_version: u32) -> Document {
    doc! {
        "subject.subject_kind": subject.subject_kind(),
        "subject.subject_id": subject.subject_id(),
        "subject_version": i64::from(subject_version),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

fn non_terminal_subject_filter(subject: &SubjectRef, subject_version: u32) -> Document {
    doc! {
        "subject.subject_kind": subject.subject_kind(),
        "subject.subject_id": subject.subject_id(),
        "subject_version": i64::from(subject_version),
        "status": {
            "$in": [
                ApprovalProcessInstanceStatus::Running.as_str(),
                ApprovalProcessInstanceStatus::Blocked.as_str(),
            ]
        },
    }
}

/// 构造按主体读取历史实例的过滤条件，不含状态限制。
///
/// # 参数
/// * `subject` - 业务对象引用
///
/// # 返回
/// 返回含主体键与软删除约束的查询文档。
///
/// # 错误
/// 无。
fn latest_subject_filter(subject: &SubjectRef) -> Document {
    doc! {
        "subject.subject_kind": subject.subject_kind(),
        "subject.subject_id": subject.subject_id(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

fn current_execution_filter(instance_id: &ApprovalProcessInstanceId) -> Document {
    doc! {
        "process_instance_id": instance_id.as_ref(),
        "status": {
            "$in": [
                ApprovalNodeExecutionStatus::Active.as_str(),
                ApprovalNodeExecutionStatus::Blocked.as_str(),
            ]
        },
    }
}

fn receipt_key_filter(
    command_kind: ApprovalCommandKind,
    scope_id: &str,
    idempotency_key: &IdempotencyKey,
) -> Document {
    doc! {
        "command_kind": command_kind.as_str(),
        "scope_id": scope_id,
        "idempotency_key": idempotency_key.as_str(),
    }
}

fn draft_or_status_filter(
    id: &str,
    expected_version: u64,
    required_status: &[ApprovalDefinitionStatus],
) -> Result<Document> {
    let expected = i64_version(expected_version)?;
    let mut filter = doc! {
        "id": id,
        "version": expected,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    };
    match required_status {
        [status] => {
            filter.insert("status", status.as_str());
        }
        statuses => {
            filter.insert(
                "status",
                doc! { "$in": statuses.iter().map(|status| status.as_str()).collect::<Vec<_>>() },
            );
        }
    }
    Ok(filter)
}

fn instance_advance_filter(
    id: &str,
    expected_version: u64,
    expected_current_execution_id: &ApprovalNodeExecutionId,
) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "current_node_execution_id": expected_current_execution_id.as_ref(),
        "status": {
            "$in": [
                ApprovalProcessInstanceStatus::Running.as_str(),
                ApprovalProcessInstanceStatus::Blocked.as_str(),
            ]
        },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

fn execution_end_filter(
    id: &str,
    expected_version: u64,
    required_status: ApprovalNodeExecutionStatus,
) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "status": required_status.as_str(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 构造取消当前执行的 `ACTIVE|BLOCKED` CAS 过滤条件。
///
/// # 参数
/// * `id` - 节点执行主键
/// * `expected_version` - 引擎变更前的加载版本
///
/// # 返回
/// 返回同时约束版本、当前状态和软删除标记的查询文档。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
fn cancellable_execution_end_filter(id: &str, expected_version: u64) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "status": {
            "$in": [
                ApprovalNodeExecutionStatus::Active.as_str(),
                ApprovalNodeExecutionStatus::Blocked.as_str(),
            ]
        },
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 构造取消后的实例列表投影。
///
/// # 参数
/// * `instance` - 已由引擎置为 `CANCELLED` 的实例
///
/// # 返回
/// 返回清空当前节点、审批人与驳回摘要并记录终态时间的有界投影。
///
/// # 错误
/// 无。
fn cancelled_instance_projection(instance: &ApprovalProcessInstance) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: None,
        current_node_name: None,
        current_assignee_participant_id: None,
        current_assignee_name: None,
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: instance.ended_at.map(|at| at.unix_secs()),
    }
}

fn instance_insert_document(
    instance: &ApprovalProcessInstance,
    list_projection: &ApprovalInstanceListProjection,
) -> Result<Document> {
    let mut document = serialize_to_document(instance)?;
    merge_documents(&mut document, serialize_to_document(list_projection)?);
    Ok(document)
}

pub(crate) fn instance_list_scope_empty(filter: &ApprovalInstanceListFilter) -> bool {
    if filter.subject_ids.as_ref().is_some_and(Vec::is_empty) {
        return true;
    }
    filter.view == ApprovalInstanceListView::Started && !started_by_present(filter)
}

/// 判断 `Started` 视图是否带有非空发起人前缀。
///
/// # 参数
/// * `filter` - 实例列表过滤条件
///
/// # 返回
/// `started_by` 为非空字符串时返回 `true`。
fn started_by_present(filter: &ApprovalInstanceListFilter) -> bool {
    filter
        .started_by
        .as_ref()
        .is_some_and(|started_by| !started_by.is_empty())
}

/// 把列表过滤条件编译为 MongoDB 文档。
///
/// # 参数
/// * `filter` - Service 已计算的视图、启动人、对象范围、游标与字面量检索
///
/// # 返回
/// 返回可直接交给 `find` / `count_documents` 的过滤文档。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 字面量检索与游标都使用 `$or` 时必须用 `$and` 组合，不得互相覆盖。
pub(crate) fn instance_list_filter_doc(filter: &ApprovalInstanceListFilter) -> Document {
    let mut document = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
    if let Some(process_kind) = filter.process_kind {
        document.insert("process_kind", process_kind.as_str());
    }
    insert_instance_status(&mut document, filter);
    if let Some(started_by) = &filter.started_by {
        document.insert("started_by", started_by);
    }
    if let Some(subject_kind) = &filter.subject_kind {
        document.insert("subject.subject_kind", subject_kind);
    }
    if let Some(subject_ids) = &filter.subject_ids {
        document.insert("subject.subject_id", doc! { "$in": subject_ids.clone() });
    }
    insert_list_disjunctions(&mut document, filter);
    document
}

/// 把游标与字面量检索的 `$or` 组合成互不覆盖的过滤条件。
///
/// # 参数
/// * `document` - 已写入等值条件的过滤文档
/// * `filter` - 列表过滤条件
///
/// # 返回
/// 无；就地写入 `$or` 或 `$and`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 顶层只能有一个 `$or`。游标与检索同时存在时必须用 `$and` 连接两组 `$or`，不得互相覆盖。
fn insert_list_disjunctions(document: &mut Document, filter: &ApprovalInstanceListFilter) {
    let mut groups = Vec::new();
    if let Some(cursor) = &filter.cursor {
        groups.push(doc! { "$or": instance_cursor_or(filter.view, cursor) });
    }
    if let Some(text_query) = &filter.text_query {
        groups.push(doc! { "$or": instance_text_query_or(text_query) });
    }
    match groups.len() {
        0 => {}
        1 => {
            if let Some(or) = groups[0].get("$or").cloned() {
                document.insert("$or", or);
            }
        }
        _ => {
            document.insert("$and", groups);
        }
    }
}

/// 构造字面量检索的 `$or` 分支。
///
/// # 参数
/// * `text_query` - 已规范化检索串
///
/// # 返回
/// 返回对象 ID、当前节点名和当前审批人的字面量匹配分支。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 检索串按字面量转义，调用方不得传入正则。快照编号只允许由联合快照聚合
/// 匹配，不得把未经三元组校验的实例 ID 注入本查询。
fn instance_text_query_or(text_query: &ApprovalInstanceTextQuery) -> Vec<Document> {
    let literal = regex::escape(text_query.query.trim());
    let regex = doc! { "$regex": &literal, "$options": "i" };
    vec![
        doc! { "subject.subject_id": regex.clone() },
        doc! { "current_assignee_name": regex.clone() },
        doc! { "current_node_name": regex },
    ]
}

fn insert_instance_status(document: &mut Document, filter: &ApprovalInstanceListFilter) {
    if filter.view == ApprovalInstanceListView::Blocked {
        document.insert("status", ApprovalProcessInstanceStatus::Blocked.as_str());
        return;
    }
    if let Some(status) = filter.status {
        document.insert("status", status.as_str());
    }
}

pub(crate) fn instance_list_sort(filter: &ApprovalInstanceListFilter) -> Document {
    match filter.view {
        ApprovalInstanceListView::Started => doc! { "started_at": -1, "id": -1 },
        ApprovalInstanceListView::Blocked => doc! { "blocked_at": -1, "id": -1 },
        ApprovalInstanceListView::Managed if filter.status.is_some() => {
            doc! { "status": 1, "updated_at": -1, "id": -1 }
        }
        ApprovalInstanceListView::Managed => doc! { "updated_at": -1, "id": -1 },
    }
}

pub(crate) fn instance_cursor_or(
    view: ApprovalInstanceListView,
    cursor: &ApprovalInstanceListCursor,
) -> Vec<Document> {
    let field = match view {
        ApprovalInstanceListView::Started => "started_at",
        ApprovalInstanceListView::Blocked => "blocked_at",
        ApprovalInstanceListView::Managed => "updated_at",
    };
    vec![
        doc! { field: { "$lt": cursor.sort_time } },
        doc! { field: cursor.sort_time, "id": { "$lt": cursor.id.as_str() } },
    ]
}

pub(crate) fn instance_summary_projection() -> Document {
    doc! {
        "id": 1,
        "process_kind": 1,
        "process_definition_id": 1,
        "definition_version": 1,
        "subject": 1,
        "subject_version": 1,
        "status": 1,
        "current_round_no": 1,
        "current_node_execution_id": 1,
        "current_node_key": 1,
        "current_node_name": 1,
        "current_assignee_participant_id": 1,
        "current_assignee_name": 1,
        "latest_rejected_execution_id": 1,
        "latest_rejection_summary": 1,
        "last_status_changed_at": 1,
        "started_by": 1,
        "started_at": 1,
        "blocked_at": 1,
        "version": 1,
        "updated_at": 1,
    }
}

/// 返回实例列表统一页大小上限。
pub(crate) fn instance_list_limit(limit: u32) -> i64 {
    clamp_limit(limit, MAX_INSTANCE_PAGE)
}

fn merge_documents(target: &mut Document, extra: Document) {
    for (key, value) in extra {
        target.insert(key, value);
    }
}

/// 按当前文档分类 CAS 未命中：不存在、版本冲突或状态变化。
pub fn classify_cas_miss<T: HasBaseModel>(
    current: Option<T>,
    expected_version: u64,
    status_matches: impl Fn(&T) -> bool,
) -> CasWriteOutcome<T> {
    let Some(current) = current else {
        return CasWriteOutcome::NotFound;
    };
    if current.base().version != expected_version {
        return CasWriteOutcome::VersionConflict(current);
    }
    if status_matches(&current) {
        return CasWriteOutcome::VersionConflict(current);
    }
    CasWriteOutcome::StatusChanged(current)
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

/// 从引擎变更后的实体版本反推加载时版本。
///
/// # 参数
/// * `current_version` - 引擎规则已经自增后的内存版本
///
/// # 返回
/// 返回执行 CAS 时应匹配的持久化版本。
///
/// # 错误
/// 当前版本为零时返回元数据越界错误。
fn previous_version(current_version: u64) -> Result<u64> {
    current_version
        .checked_sub(1)
        .ok_or(Error::EntityMetadataOutOfRange("version"))
}

/// 要求语义写入中的 CAS 已成功应用。
///
/// # 参数
/// * `outcome` - 单次实例或执行 CAS 分类
///
/// # 返回
/// 仅 [`CasWriteOutcome::Applied`] 返回 `Ok(())`。
///
/// # 错误
/// 目标缺失、版本冲突或状态变化时返回乐观锁错误。
fn require_cas_applied<T>(outcome: CasWriteOutcome<T>) -> Result<()> {
    if matches!(outcome, CasWriteOutcome::Applied(_)) {
        return Ok(());
    }
    Err(Error::OptimisticLockingError)
}

fn next_version_i64(expected_version: u64) -> Result<i64> {
    let next = expected_version
        .checked_add(1)
        .ok_or(Error::EntityMetadataOutOfRange("version"))?;
    i64_version(next)
}

/// 审批任务完成/关闭 CAS 过滤条件。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
pub fn approval_task_cas_filter(
    id: &str,
    expected_task_version: u64,
    approval_node_execution_id: &ApprovalNodeExecutionId,
) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_task_version)?,
        "status": "OPEN",
        "approval_node_execution_id": approval_node_execution_id.as_ref(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 一次性编号赋值 CAS 过滤条件。空字符串与 `null` 均视为未分配。
///
/// # 错误
/// 版本无法表示为 BSON 整数时返回错误。
pub fn assign_document_no_filter(id: &str, expected_version: u64) -> Result<Document> {
    Ok(doc! {
        "id": id,
        "version": i64_version(expected_version)?,
        "$or": [
            { "document_no": "" },
            { "document_no": mongodb::bson::Bson::Null },
        ],
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    })
}

/// 按当前事实分类一次性编号赋值未命中。
pub fn classify_assign_document_no_miss<T>(
    current: Option<T>,
    expected_version: u64,
    requested_document_no: &str,
    version_of: impl Fn(&T) -> u64,
    document_no_of: impl Fn(&T) -> &str,
) -> AssignDocumentNoOutcome<T> {
    let Some(current) = current else {
        return AssignDocumentNoOutcome::NotFound;
    };
    let existing_no = document_no_of(&current);
    if !existing_no.is_empty() && existing_no == requested_document_no {
        return AssignDocumentNoOutcome::SamePayload(current);
    }
    if !existing_no.is_empty() {
        return AssignDocumentNoOutcome::NumberConflict(current);
    }
    if version_of(&current) != expected_version {
        return AssignDocumentNoOutcome::VersionConflict(current);
    }
    AssignDocumentNoOutcome::VersionConflict(current)
}

#[cfg(test)]
mod tests {
    use super::{
        approval_task_cas_filter, assign_document_no_filter, cancellable_execution_end_filter,
        cancellation_subject_filter, cancelled_instance_projection, clamp_limit,
        classify_assign_document_no_miss, classify_cas_miss, current_execution_filter,
        definition_child_filter, definition_graph_transition_limit, definition_versions_filter,
        execution_end_filter, execution_history_filter, execution_history_limit, instance_advance_filter,
        instance_insert_document, instance_list_filter_doc, instance_list_scope_empty, instance_list_sort,
        instance_summary_projection, instance_text_query_or, latest_definition_version_from_rows,
        latest_definition_version_options, latest_subject_filter, merge_documents,
        non_terminal_subject_filter, previous_version, receipt_key_filter, require_cas_applied,
        ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListProjection,
        ApprovalInstanceListView, ApprovalInstanceTextQuery, AssignDocumentNoOutcome, CasWriteOutcome,
        LatestDefinitionVersionProjection, MAX_DEFINITION_GRAPH_DOCS, MAX_EXECUTION_HISTORY,
        MAX_INSTANCE_PAGE,
    };
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::{
        ApprovalCommandKind, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
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
