//! HTTP 面审批运行 Service：查询、决定、恢复、受阻取消与绑定升级。
//!
//! Handler 只转换协议；本文件编排仓储、prepare_* 与事务写入。

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use bpm::engine::{CommitRequired, Eligibility, TaskCloseReason, TaskIntent};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{
    ApprovalBlockerCode, ApprovalDecision, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
    ModelError,
};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey, ParticipantId,
    Timestamp,
};
use database::repository::approval_integration::{
    ApprovalRuntimeReadRepository, ApprovalRuntimeReadRow, ApprovalRuntimeReadScope,
    ApprovalRuntimeReadTypeScope,
};
use database::repository::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListProjection, ApprovalInstanceListView,
    ApprovalInstanceSummary, ApprovalInstanceTextQuery,
};
use database::{
    AccessControlExt, ApprovalIntegrationExt, BpmExt, Executor, NoTransaction, Transactional, WorkItemExt,
};
use entities::approval_integration::ApprovalSubjectSnapshot;
use entities::common::time::Instant;
use entities::document_registry::{DocumentType, WorkflowActionId};
use entities::ids::WorkItemId;
use entities::work_item::{
    ApprovalDecisionTaskError, ApprovalRuntimeTaskEnding, AssignmentSource, DocumentApprovalWorkItemData,
    WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use id_generator::next_id;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::apply_plan::PlannedWrites;
use super::authorization::{
    converge_eligibility, hidden_forbidden, requires_blocked_cancel, AuthorizationFailure,
};
use super::decision::prepare_decision;
use super::idempotency::{
    cancel_blocked_identity, command_may_have_committed, command_recovery_delay, decision_identity,
    map_receipt_first_write_error, normalize_idempotency_key, payload_conflict_error, resume_identity,
    upgrade_binding_identity, PreparedCommandIdentity, ReceiptBranch,
};
use super::resume::prepare_resume;
use super::runtime_history::{history_item_from_execution, history_page_from, RuntimeHistoryPage};
use super::runtime_query::{
    recovery_options_for, RuntimeInstanceListView, RuntimeInstanceStatusFilter, RuntimeRecoveryAction,
};
use super::view::{map_command_view, ApprovalCommandView, OpenTaskSummary};
use super::{
    prepare_cancel, CancelExecutionInput, DecisionExecutionInput, ExecutionCommandInput, PreparedExecution,
    ResumeExecutionInput,
};
use crate::approval::binding::{
    replay_unsubmitted_document_definition_upgrade, upgrade_unsubmitted_document_definition,
    UpgradeBindingResultView, UpgradeUnsubmittedDefinitionCommand,
};
use crate::approval::business_adapter::{
    adapter_object_read_decision, adapter_spec_of, document_type_from_subject_kind,
    ensure_separation_of_duties, BindingRevalidationContext,
};
use crate::approval::policy::{
    policy_of, DocumentApprovalPolicy, SeparationOfDutiesPolicy, ALL_DOCUMENT_TYPES,
    STATIC_APPROVE_PERMISSION,
};
use crate::approval::process_kind::process_kind_of;
use crate::approval::scope::definition_management_visibility;
use crate::approval::{
    approval_actor_is_active, approval_actor_is_active_with_executor,
    approval_cancel_blocked_scope_with_executor, approval_decide_scope_with_executor,
    approval_document_read_scope, approval_document_read_scope_with_executor, approval_recovery_scope,
    definition_management_visibility_with_executor, ApprovalActionContext, ApprovalCancelBlockedCommand,
    ApprovalDomainActionPort, ApprovalResumeCommand, FailClosedApprovalActionPort,
};
use crate::audit::AuditActor;
use crate::errors::{Error, ErrorCode, Result};
use crate::iam::subject;
use crate::iam::SharedRbacService;

/// HTTP 面审批运行服务。
pub struct ApprovalRuntimeService {
    db: Database,
    rbac: SharedRbacService,
    action_port: Arc<dyn ApprovalDomainActionPort>,
}

/// 实例列表默认页大小。
const DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT: u32 = 20;
/// 实例列表最大页大小。
const MAX_RUNTIME_INSTANCE_LIST_LIMIT: u32 = 100;
/// 实例列表检索串与游标 ID 最大字符数。
const RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN: usize = 128;

/// 实例列表查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeInstanceListQuery {
    /// 固定视图。
    pub view: RuntimeInstanceListView,
    /// 可选单据类型稳定码。
    pub document_type: Option<String>,
    /// 可选状态。
    pub status: Option<RuntimeInstanceStatusFilter>,
    /// 当前视图的稳定游标。
    pub cursor: Option<RuntimeInstanceListCursor>,
    /// 页大小。
    pub limit: u32,
    /// 可选字面量检索；空表示不按关键词过滤。
    pub query: Option<String>,
}

impl RuntimeInstanceListQuery {
    /// 由协议输入形成规范化查询。
    ///
    /// # 参数
    /// * `view` - 固定查询视图
    /// * `document_type` - 可选单据类型稳定码
    /// * `status` - 可选实例状态
    /// * `cursor` - HTTP 层已解码的稳定游标
    /// * `limit` - 可选页大小；省略时使用 20
    /// * `query` - 可选字面量检索串
    ///
    /// # 返回
    /// 返回已规范化并通过完整边界校验的查询。
    ///
    /// # 错误
    /// view/status、document_type、limit、cursor 或检索串不符合合同时返回校验错误。
    pub fn prepare(
        view: RuntimeInstanceListView,
        document_type: Option<String>,
        status: Option<RuntimeInstanceStatusFilter>,
        cursor: Option<RuntimeInstanceListCursor>,
        limit: Option<u32>,
        query: Option<String>,
    ) -> Result<Self> {
        let prepared = Self {
            view,
            document_type,
            status,
            cursor: cursor.map(Self::prepare_cursor),
            limit: limit.unwrap_or(DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT),
            query: query
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty()),
        };
        prepared.validate()?;
        Ok(prepared)
    }

    /// 校验规范化查询的全部纯输入合同。
    ///
    /// # 返回
    /// 所有合同成立时返回 `Ok(())`。
    ///
    /// # 错误
    /// view/status、document_type、limit、cursor 或检索串不符合合同时返回校验错误。
    pub fn validate(&self) -> Result<()> {
        self.validate_view_status()?;
        if let Some(document_type) = self.document_type.as_deref() {
            parse_document_type(document_type)?;
        }
        if !(1..=MAX_RUNTIME_INSTANCE_LIST_LIMIT).contains(&self.limit) {
            return Err(Error::ValidationError(format!(
                "limit 必须在 1 到 {MAX_RUNTIME_INSTANCE_LIST_LIMIT} 之间"
            )));
        }
        self.validate_cursor()?;
        self.validate_query()
    }

    /// trim 游标 ID；排序时间的完整 `i64` 定义域保持不变。
    fn prepare_cursor(mut cursor: RuntimeInstanceListCursor) -> RuntimeInstanceListCursor {
        cursor.id = cursor.id.trim().to_string();
        cursor
    }

    /// 校验固定视图允许的状态集合。
    fn validate_view_status(&self) -> Result<()> {
        match (self.view, self.status) {
            (RuntimeInstanceListView::Mine, None | Some(RuntimeInstanceStatusFilter::Running))
            | (RuntimeInstanceListView::Blocked, None | Some(RuntimeInstanceStatusFilter::Blocked))
            | (RuntimeInstanceListView::Started | RuntimeInstanceListView::Managed, _) => Ok(()),
            (RuntimeInstanceListView::Mine, _) => Err(Error::ValidationError(
                "mine 只接受省略 status 或 status=RUNNING".to_string(),
            )),
            (RuntimeInstanceListView::Blocked, _) => Err(Error::ValidationError(
                "blocked 只接受省略 status 或 status=BLOCKED".to_string(),
            )),
        }
    }

    /// 校验游标 ID 为已 trim 的有界稳定标识。
    fn validate_cursor(&self) -> Result<()> {
        let Some(cursor) = self.cursor.as_ref() else {
            return Ok(());
        };
        let id = cursor.id.as_str();
        if id.is_empty() || id != id.trim() {
            return Err(Error::ValidationError("cursor id 不能为空".to_string()));
        }
        if id.chars().count() > RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN {
            return Err(Error::ValidationError(format!(
                "cursor id 不能超过 {RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN} 个字符"
            )));
        }
        Ok(())
    }

    /// 校验字面量检索串已规范化且长度有界。
    fn validate_query(&self) -> Result<()> {
        let Some(query) = self.query.as_deref() else {
            return Ok(());
        };
        if query.is_empty() || query != query.trim() {
            return Err(Error::ValidationError("q 必须是非空规范化文本".to_string()));
        }
        if query.chars().count() > RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN {
            return Err(Error::ValidationError(format!(
                "q 不能超过 {RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN} 个字符"
            )));
        }
        Ok(())
    }
}

/// 实例列表稳定游标。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListCursor {
    /// 当前视图排序时间。
    pub sort_time: i64,
    /// 并列时的实例主键。
    pub id: String,
}

#[cfg(test)]
mod runtime_instance_list_query_tests {
    use entities::document_registry::DocumentType;

    use super::{
        RuntimeInstanceListCursor, RuntimeInstanceListQuery, RuntimeInstanceListView,
        RuntimeInstanceStatusFilter, DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT, MAX_RUNTIME_INSTANCE_LIST_LIMIT,
        RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN,
    };

    fn prepare(
        view: RuntimeInstanceListView,
        status: Option<RuntimeInstanceStatusFilter>,
    ) -> crate::errors::Result<RuntimeInstanceListQuery> {
        RuntimeInstanceListQuery::prepare(view, None, status, None, None, None)
    }

    /// 四种视图逐一覆盖省略状态及全部固定状态。
    #[test]
    fn view_status_matrix_is_complete() {
        let views = [
            RuntimeInstanceListView::Mine,
            RuntimeInstanceListView::Blocked,
            RuntimeInstanceListView::Started,
            RuntimeInstanceListView::Managed,
        ];
        let statuses = [
            None,
            Some(RuntimeInstanceStatusFilter::Running),
            Some(RuntimeInstanceStatusFilter::Approved),
            Some(RuntimeInstanceStatusFilter::Cancelled),
            Some(RuntimeInstanceStatusFilter::Blocked),
        ];
        for view in views {
            for status in statuses {
                let expected = match view {
                    RuntimeInstanceListView::Mine => {
                        matches!(status, None | Some(RuntimeInstanceStatusFilter::Running))
                    }
                    RuntimeInstanceListView::Blocked => {
                        matches!(status, None | Some(RuntimeInstanceStatusFilter::Blocked))
                    }
                    RuntimeInstanceListView::Started | RuntimeInstanceListView::Managed => true,
                };
                assert_eq!(prepare(view, status).is_ok(), expected, "{view:?} {status:?}");
            }
        }
    }

    /// limit 省略、两端点及越界值必须得到唯一结果。
    #[test]
    fn limit_defaults_and_rejects_outside_closed_range() {
        let default = prepare(RuntimeInstanceListView::Managed, None).expect("默认 limit");
        assert_eq!(default.limit, DEFAULT_RUNTIME_INSTANCE_LIST_LIMIT);
        for limit in [1, MAX_RUNTIME_INSTANCE_LIST_LIMIT] {
            let query = RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                None,
                Some(limit),
                None,
            )
            .expect("闭区间端点");
            assert_eq!(query.limit, limit);
        }
        for limit in [0, MAX_RUNTIME_INSTANCE_LIST_LIMIT + 1] {
            assert!(RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                None,
                Some(limit),
                None,
            )
            .is_err());
        }
    }

    /// document_type 只接受注册表中的精确稳定码，prepare 与直接 validate 必须同样失败关闭。
    #[test]
    fn document_type_requires_an_exact_registered_code() {
        let registered = DocumentType::SalesOrder.as_str();
        let prepared = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Managed,
            Some(registered.to_string()),
            None,
            None,
            None,
            None,
        )
        .expect("精确登记码");
        assert_eq!(prepared.document_type.as_deref(), Some(registered));

        for document_type in ["", "   ", "unknown", "SALES_ORDER", "sales_order "] {
            assert!(
                RuntimeInstanceListQuery::prepare(
                    RuntimeInstanceListView::Managed,
                    Some(document_type.to_string()),
                    None,
                    None,
                    None,
                    None,
                )
                .is_err(),
                "prepare 必须拒绝 {document_type:?}"
            );

            let mut direct = prepared.clone();
            direct.document_type = Some(document_type.to_string());
            assert!(direct.validate().is_err(), "validate 必须拒绝 {document_type:?}");
        }
    }

    /// cursor 保留完整 i64 时间域，ID 则 trim、非空且最多 128 字符。
    #[test]
    fn cursor_prepares_id_and_preserves_i64_time_domain() {
        for sort_time in [i64::MIN, i64::MAX] {
            let query = RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                Some(RuntimeInstanceListCursor {
                    sort_time,
                    id: "  inst-1  ".to_string(),
                }),
                None,
                None,
            )
            .expect("合法 i64 时间与可规范化 ID");
            let cursor = query.cursor.expect("游标");
            assert_eq!(cursor.sort_time, sort_time);
            assert_eq!(cursor.id, "inst-1");
        }
        let max_id = "a".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN);
        assert!(RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Managed,
            None,
            None,
            Some(RuntimeInstanceListCursor {
                sort_time: 0,
                id: max_id,
            }),
            None,
            None,
        )
        .is_ok());
        for id in [
            "   ".to_string(),
            "a".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN + 1),
        ] {
            assert!(RuntimeInstanceListQuery::prepare(
                RuntimeInstanceListView::Managed,
                None,
                None,
                Some(RuntimeInstanceListCursor { sort_time: 0, id }),
                None,
                None,
            )
            .is_err());
        }
    }

    /// q 空白归 None，文本 trim，字符上限不得按字节数误判。
    #[test]
    fn query_text_is_trimmed_and_character_bounded() {
        let blank = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("   ".to_string()),
        )
        .expect("空白 q");
        assert_eq!(blank.query, None);

        let trimmed = RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("  SO-1  ".to_string()),
        )
        .expect("trim q");
        assert_eq!(trimmed.query.as_deref(), Some("SO-1"));

        let max = "界".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN);
        assert!(RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some(max),
        )
        .is_ok());
        assert!(RuntimeInstanceListQuery::prepare(
            RuntimeInstanceListView::Started,
            None,
            None,
            None,
            None,
            Some("界".repeat(RUNTIME_INSTANCE_LIST_TEXT_MAX_LEN + 1)),
        )
        .is_err());
    }
}

/// 实例列表行。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListItem {
    /// 实例 ID。
    pub instance_id: String,
    /// 实例状态。
    pub status: String,
    /// 轮次。
    pub current_round_no: u32,
    /// 当前节点键。
    pub current_node_key: Option<String>,
    /// 当前节点名。
    pub current_node_name: Option<String>,
    /// 当前审批人。
    pub current_assignee_participant_id: Option<String>,
    /// 当前审批人显示名。
    pub current_assignee_name: Option<String>,
    /// 被审批单据类型稳定码。
    pub document_type: Option<String>,
    /// 被审批业务对象 ID。
    pub document_id: Option<String>,
    /// 被审批单据业务编号。
    pub document_label: Option<String>,
    /// 审批定义业务版本。
    pub process_version: Option<u32>,
    /// 发起时间。
    pub started_at: Option<i64>,
    /// 最近驳回原因摘要。
    pub latest_rejection_summary: Option<String>,
}

/// 实例列表页。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceListPage {
    /// 当前页。
    pub items: Vec<RuntimeInstanceListItem>,
    /// 当前过滤条件下的完整数量。
    pub total: u64,
    /// 下一页稳定游标。
    pub next_cursor: Option<RuntimeInstanceListCursor>,
}

/// 恢复选项。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeRecoveryOptionsView {
    /// 实例 ID。
    pub instance_id: String,
    /// 允许的恢复动作。
    pub actions: Vec<RuntimeRecoveryAction>,
}

/// 单实例读取授权所需的持久化事实。
struct RuntimeReadSubject {
    instance: ApprovalProcessInstance,
    current_execution: Option<ApprovalNodeExecution>,
    snapshot: ApprovalSubjectSnapshot,
    document_type: DocumentType,
}

/// 已规范化的审批决定命令；协议字段保持不变，摘要在进入事务前固定。
#[derive(Debug, Clone)]
struct RuntimeDecisionCommand {
    work_item_id: String,
    decision: ApprovalDecision,
    reason: Option<String>,
    expected_task_version: u64,
    idempotency_key: IdempotencyKey,
}

/// Fresh 决定前置只允许开放任务继续；已终结任务才可能进入收据回放。
#[derive(Debug)]
enum DecisionReceiptLookup {
    Fresh,
    Terminal(ApprovalNodeExecutionId),
}

/// 已提交受阻取消的不可变终态事实；先证明原操作人，再允许比较请求摘要。
struct CancelBlockedTerminalFacts {
    blocker: ApprovalBlockerCode,
    actor_id: String,
    reason: String,
    execution_version: u64,
    task_versions: Vec<u64>,
}

/// 决定事务的提交结果；受阻事实提交后由外层转换为稳定 409。
struct RuntimeDecisionOutcome {
    view: ApprovalCommandView,
    blocked: bool,
}

/// 决定通知只消费冻结快照、实际执行与当前权限事实。
struct DecisionNotificationFacts<'a> {
    document_type_label: &'a str,
    document_no: &'a str,
    submitted_by: &'a str,
    ended_execution: &'a ApprovalNodeExecution,
    reject_reason: Option<&'a str>,
    runtime_admin_ids: &'a [String],
}

/// 纯授权矩阵输入；I/O 与政策解析由 Service 先完成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RuntimeReadAuthorizationFacts {
    actor_active: bool,
    initiator: bool,
    current_responsibility: bool,
    object_readable: bool,
    scope_covers: bool,
    runtime_admin: bool,
}

/// 候选人。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAssigneeCandidate {
    /// 账号 ID。
    pub user_id: String,
    /// 显示名。
    pub name: String,
}

/// 绑定升级命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeBindingCommand {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 单据 ID。
    pub document_id: String,
    /// 升级原因。
    pub reason: String,
    /// 期望单据版本。
    pub expected_document_version: u64,
    /// 期望绑定版本。
    pub expected_approval_binding_version: u64,
    /// 幂等键。
    pub idempotency_key: String,
}

impl ApprovalRuntimeService {
    /// 创建运行服务。
    ///
    /// # 参数
    /// * `db` - MongoDB
    /// * `rbac` - 共享 RBAC
    ///
    /// # 返回
    /// 返回尚未由 P0-B 注入 AppState 的应用端口。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self {
            db,
            rbac,
            action_port: Arc::new(FailClosedApprovalActionPort),
        }
    }

    /// 创建已由组合根注入领域动作端口的运行服务。
    pub fn with_action_port(
        db: Database,
        rbac: SharedRbacService,
        action_port: Arc<dyn ApprovalDomainActionPort>,
    ) -> Self {
        Self {
            db,
            rbac,
            action_port,
        }
    }

    /// 按固定 view 查询实例摘要。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `query` - 已规范化查询
    ///
    /// # 错误
    /// view/status 非法或仓储失败时返回错误。
    pub async fn instance_list(
        &self,
        actor: &AuditActor,
        query: RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        query.validate()?;
        self.ensure_active_instance_reader(actor).await?;
        match query.view {
            RuntimeInstanceListView::Mine => self.list_mine(actor, &query).await,
            _ => self.list_managed_or_started(actor, &query).await,
        }
    }

    /// 读取实例详情与最近执行。
    ///
    /// # 参数
    /// * `actor` - 操作人
    /// * `instance_id` - 实例 ID
    ///
    /// # 返回
    /// 返回实例状态与当前执行投影。
    ///
    /// # 错误
    /// 不存在或无权时不泄露存在性。
    pub async fn instance_detail(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeInstanceListItem> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_ordinary_runtime_read(actor, &subject).await?;
        let instance = subject.instance;
        let execution = subject.current_execution;
        Ok(item_from_instance_id(
            instance_id,
            instance.status.as_str(),
            instance.current_round_no,
            execution.as_ref().map(|item| item.node_key.clone()),
            execution.as_ref().map(|item| item.node_name.clone()),
            execution
                .as_ref()
                .map(|item| item.assignee_participant_id.as_str().to_string()),
        ))
    }

    /// 读取实例执行历史。
    ///
    /// # 参数
    /// * `actor` - 操作人
    /// * `instance_id` - 实例 ID
    /// * `after_execution_no` - 上一页最后执行序号；首页为空
    /// * `limit` - 页大小
    ///
    /// # 返回
    /// 返回按 `execution_no` 升序的历史页，字段对齐审批 Tab。
    ///
    /// # 错误
    /// 仓储失败时返回错误。
    pub async fn instance_history(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        after_execution_no: Option<u32>,
        limit: u32,
    ) -> Result<RuntimeHistoryPage> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_ordinary_runtime_read(actor, &subject).await?;
        let fetch_limit = limit.saturating_add(1);
        let rows = self
            .db
            .bpm_workflow()
            .list_execution_history(
                &bpm::ids::ApprovalProcessInstanceId::new(instance_id),
                after_execution_no,
                fetch_limit,
                &mut NoTransaction,
            )
            .await?;
        let items = rows.iter().map(history_item_from_execution).collect();
        Ok(history_page_from(items, limit))
    }

    /// 返回当前 blocker 的唯一合法恢复动作。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `instance_id` - 审批实例 ID
    ///
    /// # 返回
    /// 返回实例 ID 与当前允许的恢复动作集合。
    ///
    /// # 错误
    /// 实例不存在时不泄露存在性。
    pub async fn recovery_options(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeRecoveryOptionsView> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_management_runtime_read(actor, &subject).await?;
        let instance = subject.instance;
        let blocked = instance.status == bpm::model::types::ApprovalProcessInstanceStatus::Blocked;
        Ok(RuntimeRecoveryOptionsView {
            instance_id: instance_id.to_string(),
            actions: recovery_options_for(blocked, instance.blocker_code),
        })
    }

    /// 提交当前开放任务的通过或驳回。
    ///
    /// 加载任务、执行、实例与定义图，重验三方责任与写时资格，调用
    /// `prepare_decision` 规划，并在一个 MongoDB 事务中应用：最终通过先登记
    /// 领域动作（单据生效），再写实例推进（CAS）、执行结束/插入、审批人绑定、
    /// 命令收据、任务完成/关闭/新建、通知 outbox 与审计。
    ///
    /// # 参数
    /// * `actor` - 当前决定人
    /// * `work_item_id` - 当前开放单据审批任务 ID
    /// * `decision` - `APPROVE` 或 `REJECT`
    /// * `reason` - 可选决定原因；空白按未提供处理
    /// * `expected_task_version` - 调用方持有的任务版本
    /// * `idempotency_key` - 本次决定命令幂等键
    ///
    /// # 返回
    /// 返回回放或实际应用后的审批命令视图。
    ///
    /// # 错误
    /// 任务不存在、责任不一致、版本冲突或仓储失败时返回错误。
    pub async fn submit_decision(
        &self,
        actor: &AuditActor,
        work_item_id: &str,
        decision: &str,
        reason: Option<&str>,
        expected_task_version: u64,
        idempotency_key: &str,
    ) -> Result<ApprovalCommandView> {
        let decision = match decision {
            "APPROVE" => ApprovalDecision::Approve,
            "REJECT" => ApprovalDecision::Reject,
            other => {
                return Err(Error::ValidationError(format!(
                    "决定必须是 APPROVE 或 REJECT，收到 {other}"
                )))
            }
        };
        let reason = reason
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned);
        let key = normalize_idempotency_key(idempotency_key)?;
        let command = RuntimeDecisionCommand {
            work_item_id: work_item_id.to_string(),
            decision,
            reason: reason.clone(),
            expected_task_version,
            idempotency_key: key,
        };
        let outcome = self.commit_decision(actor, command.clone()).await;
        let outcome = match outcome {
            Ok(outcome) => outcome,
            Err(error) if command_may_have_committed(&error) => {
                self.recover_decision_after_competing_commit(actor, command, error)
                    .await?
            }
            Err(error) => return Err(error),
        };
        if outcome.blocked {
            return Err(Error::from_approval_code(ErrorCode::ApprovalInstanceBlocked));
        }
        Ok(outcome.view)
    }

    /// 在唯一 MongoDB 事务中先查收据，再执行决定前置、授权与全部写入。
    async fn commit_decision(
        &self,
        actor: &AuditActor,
        command: RuntimeDecisionCommand,
    ) -> Result<RuntimeDecisionOutcome> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let action_port = Arc::clone(&self.action_port);
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    submit_decision_in_transaction(
                        &db,
                        &rbac,
                        action_port.as_ref(),
                        &actor,
                        &command,
                        session,
                    )
                    .await
                })
            })
            .await
    }

    /// 并发唯一键或提交结果未知后，必须使用新的事务会话只读胜者收据。
    async fn recover_decision_after_competing_commit(
        &self,
        actor: &AuditActor,
        command: RuntimeDecisionCommand,
        original_error: Error,
    ) -> Result<RuntimeDecisionOutcome> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let actor = actor.clone();
            let command = command.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        replay_decision_in_transaction(&db, &rbac, &actor, &command, session).await
                    })
                })
                .await;
            match recovered {
                Ok(Some(outcome)) => return Ok(outcome),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 写时重验审批人资格：账号启用、具备 `approval_instance:decide`、能读取
    /// 被审单据。任一失败收敛为对应人员 blocker，不得回滚为空。
    ///
    /// # 参数
    /// * `assignee_id` - 当前或下一节点审批人账号 ID
    /// * `assignee_name` - 定义或执行中的显示名快照
    /// * `snapshot` - 被审单据冻结主体与责任组织
    /// * `spec` - 单据类型审批适配器规格
    /// * `executor` - 调用方持有的数据库快照执行器
    ///
    /// # 返回
    /// 返回 BPM 可消费的有效或结构化受阻资格。
    ///
    /// # 错误
    /// Repository、权限解析、RBAC 或对象读取适配器失败时返回错误。
    ///
    /// # 关键业务约束
    /// 账号后台有效性由实体判断，Service 只编排权限与对象读取 I/O。
    async fn revalidate_approver(
        &self,
        assignee_id: &str,
        assignee_name: &str,
        snapshot: &ApprovalSubjectSnapshot,
        spec: &crate::approval::business_adapter::ApprovalAdapterSpec,
        executor: &mut dyn Executor,
    ) -> Result<Eligibility> {
        if spec.document_type == DocumentType::StockAdjustment {
            return revalidate_decision_approver(
                &self.db,
                &self.rbac,
                assignee_id,
                assignee_name,
                None,
                snapshot,
                spec,
                process_required_separation_policy(snapshot.document_type)?,
                executor,
            )
            .await;
        }
        let failure = match self
            .db
            .accounts()
            .find_approval_assignee_by_id(assignee_id, executor)
            .await?
        {
            Some(account) if account.is_active_backoffice() => {
                let permission = entities::Permission::parse(STATIC_APPROVE_PERMISSION)
                    .map_err(|error| Error::Internal(format!("静态审批权限不变量损坏: {error}")))?;
                if self
                    .rbac
                    .enforce(&subject(account.kind, &account.base.id), &permission)
                    .await?
                {
                    let context = BindingRevalidationContext {
                        organization_id: snapshot.payload.responsible_org_id.clone(),
                        creator_id: String::new(),
                    };
                    match adapter_object_read_decision(spec, &context, assignee_id)? {
                        Some(true) => None,
                        _ => Some(AuthorizationFailure::CannotReadSubject),
                    }
                } else {
                    Some(AuthorizationFailure::NotEligible)
                }
            }
            _ => Some(AuthorizationFailure::AccountInactive),
        };
        converge_eligibility(assignee_id, assignee_name, failure)
    }

    /// 在原审批人重新合格后恢复当前受阻执行。
    ///
    /// # 参数
    /// * `actor` - 当前认证且具备恢复权限的审计主体
    /// * `command` - 实例、执行、审批人和已关闭任务的期望版本及幂等键
    ///
    /// # 返回
    /// 返回恢复后的审批命令视图；幂等回放返回已持久化视图。
    ///
    /// # 错误
    /// 主体不一致、实例缺失、权限不足、版本冲突、快照不一致或事务写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 冻结快照必须精确匹配单据类型、主体 ID 和提交版本，事务边界仍由 Service 持有。
    pub async fn resume_current_approver(
        &self,
        actor: &AuditActor,
        command: ApprovalResumeCommand,
    ) -> Result<ApprovalCommandView> {
        ensure_command_actor(actor, &command.actor_id)?;
        let instance_id = command.approval_process_instance_id.clone();
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        let identity = resume_identity(
            idempotency_key.clone(),
            &instance_id,
            command.expected_instance_version,
            command.expected_execution_version,
            command.expected_assignment_version,
            command.expected_closed_task_version,
            actor.id(),
        )?;
        if let Some(view) = self.replay_resume(actor, &instance_id, &identity).await? {
            return Ok(view);
        }

        self.require_recovery_action(&instance_id, RuntimeRecoveryAction::ResumeCurrentApprover)
            .await?;
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        ensure_expected_version(
            "审批实例",
            command.expected_instance_version,
            instance.base.version,
        )?;
        let current = self
            .db
            .bpm_workflow()
            .find_current_execution(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例缺少当前受阻执行".to_string()))?;
        ensure_expected_version(
            "审批执行",
            command.expected_execution_version,
            current.base.version,
        )?;
        let assignee = self
            .db
            .bpm_workflow()
            .find_assignee_for_node(
                &ApprovalProcessInstanceId::new(&instance_id),
                &current.node_key,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::ConflictError("实例缺少当前节点审批人绑定".to_string()))?;
        ensure_expected_version(
            "审批人绑定",
            command.expected_assignment_version,
            assignee.base.version,
        )?;
        let closed_task_guard = self
            .load_resume_task_guard(
                &ApprovalNodeExecutionId::new(current.base.id.clone()),
                command.expected_closed_task_version,
            )
            .await?;
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(&instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例缺少冻结业务快照".to_string()))?;
        let document_type = document_type_from_subject_kind(instance.subject.subject_kind())?;
        snapshot
            .ensure_matches_runtime_subject(
                document_type,
                instance.subject.subject_id(),
                instance.subject_version,
            )
            .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
        let recovery_scope = approval_recovery_scope(&self.db, &self.rbac, actor).await?;
        if !recovery_scope.covers(&snapshot.payload.responsible_org_id) {
            return Err(Error::Forbidden("无权恢复该责任组织的审批实例".to_string()));
        }
        let spec = adapter_spec_of(document_type)?;
        let eligibility = self
            .revalidate_approver(
                assignee.current_assignee_participant_id.as_str(),
                &current.assignee_name_snapshot,
                &snapshot,
                &spec,
                &mut NoTransaction,
            )
            .await?;
        ensure_resume_approver_recovered(&eligibility)?;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&instance.process_definition_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
        let now = Instant::now();
        let prepared = prepare_resume(ResumeExecutionInput {
            command: ExecutionCommandInput {
                graph,
                current_eligibility: eligibility.clone(),
                next_eligibility: eligibility,
                receipt: None,
                idempotency_key: idempotency_key.clone(),
                now: Timestamp::from_utc(now.as_utc()),
            },
            instance,
            current: current.clone(),
            assignee: assignee.clone(),
            expected_instance_version: command.expected_instance_version,
            expected_execution_version: command.expected_execution_version,
            expected_assignment_version: command.expected_assignment_version,
            expected_closed_task_version: command.expected_closed_task_version,
            next_execution_id: ApprovalNodeExecutionId::new(next_id()),
            next_execution_no: current.execution_no.saturating_add(1),
            receipt_id: ApprovalCommandReceiptId::new(next_id()),
            actor_id: actor.id().to_string(),
        })?;
        let PreparedExecution::Apply(writes) = prepared else {
            return self
                .persisted_command_view(&instance_id, CommitRequired::Proceed, true)
                .await;
        };
        let writes = *writes;
        let new_task_ids = writes.create_tasks.iter().map(|_| next_id()).collect::<Vec<_>>();
        let list_projection = list_projection_from_writes(&writes, &current.base.id, None, now);
        let audit = actor.clone().resource_log_with_message(
            "approval.resume_current_approver",
            "approval_process_instance",
            instance_id.clone(),
            Some(format!("execution={}", current.base.id)),
        )?;
        let db = self.db.clone();
        let client = self.db.client().clone();
        let owner_role = spec.owner_role.as_str().to_string();
        let owner_organization_id = snapshot.payload.responsible_org_id.clone();
        let subject_version = snapshot.subject_version.to_string();
        let business_object_id = snapshot.business_object_id.clone();
        let document_type_label = document_type.label().to_string();
        let document_no = snapshot.payload.document_no.clone();
        let submitted_by = snapshot.payload.submitted_by.clone();
        let ended_execution_id = current.base.id.clone();
        let rbac = self.rbac.clone();
        let stock_resume_revalidation = (document_type == DocumentType::StockAdjustment).then(|| {
            (
                assignee.current_assignee_participant_id.as_str().to_string(),
                current.assignee_name_snapshot.clone(),
                snapshot.clone(),
                spec.clone(),
            )
        });
        let recovery_identity = identity.clone();
        let recovery_instance_id = instance_id.clone();
        let view = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some((assignee_id, assignee_name, snapshot, spec)) =
                        stock_resume_revalidation.as_ref()
                    {
                        let eligibility = revalidate_decision_approver(
                            &db,
                            &rbac,
                            assignee_id,
                            assignee_name,
                            None,
                            snapshot,
                            spec,
                            process_required_separation_policy(snapshot.document_type)?,
                            session,
                        )
                        .await?;
                        ensure_resume_approver_recovered(&eligibility)?;
                    }
                    persist_resume_writes(
                        &db,
                        ResumePersistInput {
                            writes: &writes,
                            ended_execution_id: &ended_execution_id,
                            expected_instance_version: command.expected_instance_version,
                            expected_execution_version: command.expected_execution_version,
                            closed_task_guard: closed_task_guard.as_ref(),
                            new_task_ids: &new_task_ids,
                            list_projection: &list_projection,
                            audit: &audit,
                            now,
                            owner_role: &owner_role,
                            owner_organization_id: &owner_organization_id,
                            subject_version: &subject_version,
                            business_object_id: &business_object_id,
                            document_type_label: &document_type_label,
                            document_no: &document_no,
                            submitted_by: &submitted_by,
                        },
                        session,
                    )
                    .await?;
                    Ok::<ApprovalCommandView, crate::errors::Error>(map_command_view(
                        &writes.instance,
                        writes.created_executions.last(),
                        None,
                        None,
                        first_open_task(&writes, &new_task_ids),
                        writes.commit,
                        false,
                    ))
                })
            })
            .await;
        match view {
            Ok(view) => Ok(view),
            Err(error) if command_may_have_committed(&error) => {
                self.recover_resume_after_competing_commit(
                    actor,
                    recovery_instance_id,
                    recovery_identity,
                    error,
                )
                .await
            }
            Err(error) => Err(error),
        }
    }

    /// 在独立事务快照内按当前权限回放原审批人恢复结果。
    async fn replay_resume(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        identity: &PreparedCommandIdentity,
    ) -> Result<Option<ApprovalCommandView>> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_string();
        let identity = identity.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    replay_resume_in_transaction(&db, &rbac, &actor, &instance_id, &identity, session).await
                })
            })
            .await
    }

    /// 唯一键竞争、瞬态事务错误或提交结果未知后，以新会话有限回读胜者。
    async fn recover_resume_after_competing_commit(
        &self,
        actor: &AuditActor,
        instance_id: String,
        identity: PreparedCommandIdentity,
        original_error: Error,
    ) -> Result<ApprovalCommandView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            match self.replay_resume(actor, &instance_id, &identity).await {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 取消不允许原审批人恢复、但允许人工终止的受阻实例。
    ///
    /// # 参数
    /// * `actor` - 当前认证且具备恢复权限的审计主体
    /// * `command` - 受阻实例、执行和任务期望版本、取消原因及幂等键
    ///
    /// # 返回
    /// 返回取消后的审批命令视图；幂等回放返回已持久化视图。
    ///
    /// # 错误
    /// 原审批人恢复前置不满足、权限不足、版本冲突、快照不一致或事务写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅允许无开放任务的受阻端口取消，冻结快照三项主体引用必须全部精确匹配。
    pub async fn cancel_blocked(
        &self,
        actor: &AuditActor,
        mut command: ApprovalCancelBlockedCommand,
    ) -> Result<ApprovalCommandView> {
        ensure_command_actor(actor, &command.actor_id)?;
        command.reason = command.reason.trim().to_string();
        if command.reason.is_empty() {
            return Err(Error::ValidationError("受阻取消原因不能为空".to_string()));
        }
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        command.idempotency_key = idempotency_key.as_str().to_string();
        let outcome = self
            .commit_cancel_blocked(actor, command.clone(), idempotency_key.clone())
            .await;
        match outcome {
            Ok(view) => Ok(view),
            Err(error) if command_may_have_committed(&error) => {
                self.recover_cancel_blocked_after_competing_commit(actor, command, idempotency_key, error)
                    .await
            }
            Err(error) => Err(error),
        }
    }

    /// 在唯一事务内先查/写收据，再执行受阻取消的强类型动作与全部副作用。
    async fn commit_cancel_blocked(
        &self,
        actor: &AuditActor,
        command: ApprovalCancelBlockedCommand,
        idempotency_key: IdempotencyKey,
    ) -> Result<ApprovalCommandView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let action_port = Arc::clone(&self.action_port);
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    cancel_blocked_in_transaction(
                        &db,
                        &rbac,
                        action_port.as_ref(),
                        &actor,
                        &command,
                        &idempotency_key,
                        session,
                    )
                    .await
                })
            })
            .await
    }

    /// 并发唯一键或提交结果未知后使用新会话只读胜者收据。
    async fn recover_cancel_blocked_after_competing_commit(
        &self,
        actor: &AuditActor,
        command: ApprovalCancelBlockedCommand,
        idempotency_key: IdempotencyKey,
        original_error: Error,
    ) -> Result<ApprovalCommandView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let actor = actor.clone();
            let command = command.clone();
            let idempotency_key = idempotency_key.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        replay_cancel_blocked_in_transaction(
                            &db,
                            &rbac,
                            &actor,
                            &command,
                            &idempotency_key,
                            session,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 升级未提交单据绑定到当前发布定义。
    ///
    /// # 错误
    /// 已提交或版本冲突时返回错误。
    pub async fn upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeBindingCommand,
    ) -> Result<UpgradeBindingResultView> {
        let reason = command.reason.trim().to_string();
        if reason.is_empty() {
            return Err(Error::ValidationError("升级原因不能为空".to_string()));
        }
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        let identity = upgrade_binding_identity(
            command.document_type.as_str(),
            &command.document_id,
            command.expected_document_version,
            command.expected_approval_binding_version,
            &reason,
            actor.id(),
            idempotency_key,
        )?;
        let prepared = UpgradeUnsubmittedDefinitionCommand {
            document_type: command.document_type,
            document_id: command.document_id,
            expected_business_object_version: command.expected_document_version,
            expected_binding_version: command.expected_approval_binding_version,
            reason,
            identity,
            action_id: WorkflowActionId::new(next_id()),
            receipt_id: ApprovalCommandReceiptId::new(next_id()),
        };
        match self.commit_upgrade_binding(actor, prepared.clone()).await {
            Ok(view) => Ok(view),
            Err(error) if command_may_have_committed(&error) => {
                self.recover_upgrade_binding(actor, prepared, error).await
            }
            Err(error) => Err(error),
        }
    }

    /// 在唯一调用方事务内执行升级；绑定端口不得自行开启嵌套事务。
    async fn commit_upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeUnsubmittedDefinitionCommand,
    ) -> Result<UpgradeBindingResultView> {
        let db = self.db.clone();
        let rbac = self.rbac.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |session| {
                Box::pin(async move {
                    upgrade_unsubmitted_document_definition(&db, &rbac, &command, &actor, session).await
                })
            })
            .await
    }

    /// 唯一键竞争或提交结果未知后，以新事务重验授权并只读胜者收据。
    async fn recover_upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeUnsubmittedDefinitionCommand,
        original_error: Error,
    ) -> Result<UpgradeBindingResultView> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let rbac = self.rbac.clone();
            let actor = actor.clone();
            let command = command.clone();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        replay_unsubmitted_document_definition_upgrade(&db, &rbac, &command, &actor, session)
                            .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(view)) => return Ok(view),
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
    }

    /// 重验当前读取主体仍为有效账号；失效时隐藏目标实例存在性。
    ///
    /// # 参数
    /// * `actor` - Handler 已认证但状态可能已经变化的账号快照
    ///
    /// # 返回
    /// 账号仍存在、类型一致且启用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 账号失效时返回隐藏存在性的 NotFound；仓储失败时传播基础设施错误。
    async fn ensure_active_instance_reader(&self, actor: &AuditActor) -> Result<()> {
        if approval_actor_is_active(&self.db, actor).await? {
            return Ok(());
        }
        Err(hidden_not_found())
    }

    /// 加载实例、当前执行与唯一冻结快照，并校验运行主体三元组。
    async fn load_runtime_read_subject(&self, instance_id: &str) -> Result<RuntimeReadSubject> {
        let instance_id = ApprovalProcessInstanceId::new(instance_id);
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(instance_id.as_ref(), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let document_type = document_type_from_subject_kind(instance.subject.subject_kind())
            .map_err(|_| hidden_not_found())?;
        adapter_spec_of(document_type)?;
        if instance.process_kind != process_kind_of(document_type)
            || snapshot
                .ensure_matches_runtime_subject(
                    document_type,
                    instance.subject.subject_id(),
                    instance.subject_version,
                )
                .is_err()
        {
            return Err(hidden_not_found());
        }
        let current_execution = self
            .db
            .bpm_workflow()
            .find_current_execution(&instance_id, &mut NoTransaction)
            .await?;
        if !current_execution_matches_instance(&instance, current_execution.as_ref()) {
            return Err(hidden_not_found());
        }
        Ok(RuntimeReadSubject {
            instance,
            current_execution,
            snapshot,
            document_type,
        })
    }

    /// 校验普通详情/历史读取的三条互斥授权来源。
    async fn ensure_ordinary_runtime_read(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<()> {
        let initiator = subject.instance.started_by.as_str() == actor.id();
        if ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator,
            current_responsibility: false,
            object_readable: false,
            scope_covers: false,
            runtime_admin: false,
        }) {
            return Ok(());
        }
        if self.current_runtime_responsibility(actor, subject).await? {
            return Ok(());
        }
        adapter_spec_of(subject.document_type)?;
        let scope = approval_document_read_scope(&self.db, &self.rbac, actor, subject.document_type).await?;
        let facts = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: false,
            current_responsibility: false,
            object_readable: !scope.is_empty(),
            scope_covers: scope.covers(&subject.snapshot.payload.responsible_org_id),
            runtime_admin: false,
        };
        if ordinary_runtime_read_allowed(facts) {
            return Ok(());
        }
        Err(hidden_not_found())
    }

    /// 校验恢复选项等管理读取的类型、对象与组织三门。
    async fn ensure_management_runtime_read(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<()> {
        let visibility = definition_management_visibility(&self.db, &self.rbac, actor).await?;
        adapter_spec_of(subject.document_type)?;
        let scope = approval_document_read_scope(&self.db, &self.rbac, actor, subject.document_type).await?;
        let facts = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: subject.instance.started_by.as_str() == actor.id(),
            current_responsibility: false,
            object_readable: !scope.is_empty(),
            scope_covers: scope.covers(&subject.snapshot.payload.responsible_org_id),
            runtime_admin: visibility.runtime_admin_types().contains(&subject.document_type),
        };
        if management_runtime_read_allowed(facts) {
            return Ok(());
        }
        Err(hidden_not_found())
    }

    /// 判断当前执行是否仍有由本人承担的开放审批任务。
    async fn current_runtime_responsibility(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<bool> {
        if subject.instance.status != ApprovalProcessInstanceStatus::Running {
            return Ok(false);
        }
        let Some(execution) = &subject.current_execution else {
            return Ok(false);
        };
        if execution.status != ApprovalNodeExecutionStatus::Active
            || execution.round_no != subject.instance.current_round_no
            || execution.assignee_participant_id.as_str() != actor.id()
        {
            return Ok(false);
        }
        let execution_id = ApprovalNodeExecutionId::new(execution.base.id.clone());
        let tasks = self
            .db
            .work_items()
            .open_approval_tasks_for_execution(&execution_id, &mut NoTransaction)
            .await?;
        if tasks.len() != 1 {
            return Ok(false);
        }
        let owner_role = adapter_spec_of(subject.document_type)?.owner_role;
        Ok(task_proves_current_responsibility(
            &tasks[0],
            execution,
            subject,
            actor.id(),
            owner_role.as_str(),
        ))
    }

    /// 返回由开放审批任务映射的运行实例列表页。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 可选单据类型与页大小
    ///
    /// # 返回
    /// 返回当前账号拥有的开放单据审批任务页。
    ///
    /// # 错误
    /// WorkItem Repository 查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 任务类型、开放状态、责任人与可选单据类型全部由 Repository 固定查询封装。
    async fn list_mine(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let document_type = query
            .document_type
            .as_deref()
            .map(parse_document_type)
            .transpose()?;
        let cursor = query
            .cursor
            .as_ref()
            .map(|cursor| {
                if cursor.sort_time < 0 {
                    return Err(Error::ValidationError(
                        "mine cursor sort_time 不能为负数".to_string(),
                    ));
                }
                Ok((cursor.sort_time, cursor.id.as_str()))
            })
            .transpose()?;
        let page = self
            .db
            .work_items()
            .page_open_document_approval_owned_by(
                actor.id(),
                document_type.map(|document_type| document_type.as_str()),
                query.query.as_deref(),
                cursor,
                query.limit,
                &mut NoTransaction,
            )
            .await?;
        ensure_mine_page_integrity(page.integrity_conflicts.len())?;
        let items = self.hydrate_mine_items(actor, page.items).await?;
        let next_cursor = if page.has_more {
            page.next_cursor
                .map(|(sort_time, id)| RuntimeInstanceListCursor { sort_time, id })
        } else {
            None
        };
        Ok(RuntimeInstanceListPage {
            items,
            total: page.total,
            next_cursor,
        })
    }

    /// 批量装载 Mine 页的 execution、instance、summary 与 snapshot，并按原任务
    /// 顺序重建实例行。任一身份链漂移时整页失败关闭，禁止静默丢行后伪造 total。
    async fn hydrate_mine_items(
        &self,
        actor: &AuditActor,
        tasks: Vec<WorkItem>,
    ) -> Result<Vec<RuntimeInstanceListItem>> {
        let execution_ids = mine_execution_ids(&tasks)?;
        let executions = self
            .db
            .bpm_workflow()
            .list_executions_by_ids(&execution_ids, &mut NoTransaction)
            .await?;
        let execution_by_id = unique_by_id(executions, |execution| execution.base.id.clone())?;

        let instance_ids = mine_instance_ids(&execution_ids, &execution_by_id)?;
        let summaries = self
            .db
            .bpm_workflow()
            .list_instance_summaries_by_ids(&instance_ids, &mut NoTransaction)
            .await?;
        let summary_by_id = unique_by_id(summaries, |summary| summary.id.clone())?;
        let instance_id_strings = instance_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        let snapshots = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_ids(&instance_id_strings, &mut NoTransaction)
            .await?;
        let snapshot_by_instance = unique_by_id(snapshots, |snapshot| {
            snapshot.approval_process_instance_id.to_string()
        })?;

        tasks
            .into_iter()
            .map(|task| {
                let execution_id = task
                    .approval_node_execution_id
                    .as_ref()
                    .ok_or_else(hidden_not_found)?;
                let execution = execution_by_id
                    .get(execution_id.as_ref())
                    .ok_or_else(hidden_not_found)?;
                let summary = summary_by_id
                    .get(execution.process_instance_id.as_ref())
                    .ok_or_else(hidden_not_found)?;
                let snapshot = snapshot_by_instance.get(summary.id.as_str());
                if !mine_runtime_chain_matches(&task, execution, summary, snapshot, actor.id())? {
                    return Err(hidden_not_found());
                }
                item_from_summary(summary.clone(), snapshot)
            })
            .collect()
    }

    /// 查询本人发起或管理范围内的审批实例。
    ///
    /// # 参数
    /// * `actor` - 当前已认证账号
    /// * `query` - 已规范化查询，可含字面量检索
    ///
    /// # 返回
    /// 返回 MongoDB 联合不可变快照完成授权过滤、检索、计数与分页后的实例页。
    ///
    /// # 错误
    /// 单据类型未登记或仓储失败时返回错误。
    ///
    /// # 关键业务约束
    /// 检索必须在 MongoDB 内施加。不得先取当前页再内存过滤。
    async fn list_managed_or_started(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        let type_scopes = self.runtime_read_type_scopes(actor, query).await?;
        let mut filter = instance_list_filter(actor, query)?;
        filter.limit = query.limit.saturating_add(1);
        let scope = if query.view == RuntimeInstanceListView::Started {
            ApprovalRuntimeReadScope::Started {
                process_kinds: type_scopes.iter().map(|scope| scope.process_kind).collect(),
                submitted_by: actor.id().to_string(),
            }
        } else {
            ApprovalRuntimeReadScope::Managed {
                type_scopes: type_scopes.clone(),
            }
        };
        let mut page = ApprovalRuntimeReadRepository::new(&self.db)
            .search(&filter, &scope, &mut NoTransaction)
            .await?;
        let has_more = page.items.len() > query.limit as usize;
        if has_more {
            page.items.truncate(query.limit as usize);
        }
        let next_cursor = has_more
            .then(|| {
                page.items
                    .last()
                    .map(|row| cursor_from_summary(filter.view, &row.instance))
            })
            .flatten();
        let items = page
            .items
            .into_iter()
            .map(|row| item_from_runtime_read_row(row, actor, query.view, &type_scopes))
            .collect::<Result<Vec<_>>>()?;
        Ok(RuntimeInstanceListPage {
            items,
            total: page.total,
            next_cursor,
        })
    }

    /// 计算 Started 或管理视图可进入 Repository 的固定流程种类。
    ///
    /// # 参数
    /// * `actor` - 当前有效账号
    /// * `query` - 已规范化视图与可选固定单据类型
    ///
    /// # 返回
    /// Started 返回全部必须审批类型或请求类型；Managed/Blocked 返回当前账号具备
    /// `runtime_admin_permission` 的类型交集。
    ///
    /// # 错误
    /// 单据类型、政策或 RBAC 读取失败时返回错误。
    ///
    /// # 关键业务约束
    /// Service 只把已证明的类型集合交给 Repository；空集合固定形成空页。
    async fn runtime_read_type_scopes(
        &self,
        actor: &AuditActor,
        query: &RuntimeInstanceListQuery,
    ) -> Result<Vec<ApprovalRuntimeReadTypeScope>> {
        let requested = query
            .document_type
            .as_deref()
            .map(parse_document_type)
            .transpose()?;
        let mut allowed = if query.view == RuntimeInstanceListView::Started {
            process_required_document_types()?
        } else {
            definition_management_visibility(&self.db, &self.rbac, actor)
                .await?
                .runtime_admin_types()
                .to_vec()
        };
        if let Some(requested) = requested {
            allowed.retain(|document_type| *document_type == requested);
        }
        let mut scopes = Vec::new();
        for document_type in allowed {
            adapter_spec_of(document_type)?;
            let organization_ids = if query.view == RuntimeInstanceListView::Started {
                None
            } else {
                let scope = approval_document_read_scope(&self.db, &self.rbac, actor, document_type).await?;
                if scope.is_empty() {
                    continue;
                }
                scope.organization_ids().map(ToOwned::to_owned)
            };
            scopes.push(ApprovalRuntimeReadTypeScope {
                process_kind: process_kind_of(document_type),
                organization_ids,
            });
        }
        Ok(scopes)
    }

    async fn require_recovery_action(&self, instance_id: &str, wanted: RuntimeRecoveryAction) -> Result<()> {
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let blocked = instance.status == bpm::model::types::ApprovalProcessInstanceStatus::Blocked;
        if recovery_options_for(blocked, instance.blocker_code).contains(&wanted) {
            return Ok(());
        }
        Err(Error::ConflictError("当前 blocker 不允许该恢复动作".to_string()))
    }

    async fn load_resume_task_guard(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        expected_closed_task_version: Option<u64>,
    ) -> Result<Option<ClosedTaskGuard>> {
        let tasks = self
            .db
            .work_items()
            .approval_tasks_for_execution(execution_id, &mut NoTransaction)
            .await?;
        if tasks.len() > 1 {
            return Err(Error::ConflictError("受阻执行关联多个历史审批任务".to_string()));
        }
        let Some(task) = tasks.into_iter().next() else {
            if expected_closed_task_version.is_some() {
                return Err(Error::ConflictError(
                    "调用方声明了关闭任务版本，但历史任务不存在".to_string(),
                ));
            }
            return Ok(None);
        };
        if task.status != WorkItemStatus::Closed {
            return Err(Error::ConflictError("人员恢复要求原审批任务已经关闭".to_string()));
        }
        if let Some(expected) = expected_closed_task_version {
            ensure_expected_version("已关闭审批任务", expected, task.base.version)?;
        }
        Ok(Some(ClosedTaskGuard {
            task_id: task.base.id,
            execution_id: execution_id.clone(),
            version: task.base.version,
        }))
    }

    async fn persisted_command_view(
        &self,
        instance_id: &str,
        commit: CommitRequired,
        replay: bool,
    ) -> Result<ApprovalCommandView> {
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let current = self
            .db
            .bpm_workflow()
            .find_current_execution(&ApprovalProcessInstanceId::new(instance_id), &mut NoTransaction)
            .await?;
        let next_open_task = match current.as_ref() {
            Some(execution) => {
                let tasks = self
                    .db
                    .work_items()
                    .open_approval_tasks_for_execution(
                        &ApprovalNodeExecutionId::new(execution.base.id.clone()),
                        &mut NoTransaction,
                    )
                    .await?;
                if tasks.len() > 1 {
                    return Err(Error::ConflictError("当前执行关联多个开放审批任务".to_string()));
                }
                tasks.into_iter().next().map(|task| OpenTaskSummary {
                    work_item_id: task.base.id,
                    task_version: task.base.version.to_string(),
                    owner_user_id: task.owner_user_id.unwrap_or_default(),
                })
            }
            None => None,
        };
        Ok(map_command_view(
            &instance,
            current.as_ref(),
            None,
            None,
            next_open_task,
            commit,
            replay,
        ))
    }
}

/// 原审批人恢复回放先按当前账号与责任组织授权，再允许读取和比较收据。
async fn replay_resume_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    instance_id: &str,
    identity: &PreparedCommandIdentity,
    session: &mut mongodb::ClientSession,
) -> Result<Option<ApprovalCommandView>> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(instance_id), session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let (_, snapshot) = load_exact_runtime_snapshot(db, &instance, session, true).await?;
    let recovery_scope = approval_recovery_scope(db, rbac, actor).await?;
    if !recovery_scope.covers(&snapshot.payload.responsible_org_id) {
        return Err(Error::Forbidden("无权恢复该责任组织的审批实例".to_string()));
    }
    let Some(receipt) = find_receipt_for_identity(db, identity, session).await? else {
        return Ok(None);
    };
    if receipt.result_ref != instance_id {
        return Err(Error::ConflictError("恢复收据结果引用与实例不一致".to_string()));
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {}
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    persisted_command_view_with_executor(db, instance_id, CommitRequired::Proceed, true, session)
        .await
        .map(Some)
}

/// 受阻取消先按实例终态证明原操作人并重验当前授权，再允许查询和比较收据。
async fn replay_cancel_blocked_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    session: &mut mongodb::ClientSession,
) -> Result<Option<ApprovalCommandView>> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(
            &ApprovalProcessInstanceId::new(&command.approval_process_instance_id),
            session,
        )
        .await?
        .ok_or_else(hidden_not_found)?;
    if instance.base.id != command.approval_process_instance_id {
        return Err(hidden_not_found());
    }
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, &instance, session, true).await?;

    let terminal_facts = if instance.status == ApprovalProcessInstanceStatus::Cancelled {
        // 与 Fresh 路径保持相同顺序：失权调用方在任何收据读取或摘要比较前即失败关闭。
        ensure_cancel_blocked_authorized(db, rbac, actor, document_type, &snapshot, session).await?;
        let facts = load_cancel_blocked_terminal_facts(db, &instance, session)
            .await?
            .ok_or_else(hidden_not_found)?;
        if facts.actor_id != actor.id() {
            return match ensure_cancel_blocked_instance_preconditions(&instance, command) {
                Err(error) => Err(error),
                Ok(()) => Err(hidden_forbidden()),
            };
        }
        Some(facts)
    } else {
        None
    };

    let Some(terminal_facts) = terminal_facts else {
        return Ok(None);
    };
    let identity = cancel_blocked_identity(
        idempotency_key.clone(),
        &command.approval_process_instance_id,
        terminal_facts.blocker.as_str(),
        command.expected_instance_version,
        command.expected_execution_version,
        command.expected_task_version,
        &command.reason,
        actor.id(),
    )?;
    let Some(receipt) = find_receipt_for_identity(db, &identity, session).await? else {
        return Ok(None);
    };
    if receipt.result_ref != command.approval_process_instance_id {
        return Err(hidden_not_found());
    }
    // V2 与 legacy 都必须由同一组终态事实证明 receipt result；legacy 不能只依赖旧摘要。
    if !cancel_blocked_terminal_facts_match(&instance, &terminal_facts, command, actor.id()) {
        return Err(payload_conflict_error());
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {}
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    persisted_command_view_with_executor(db, &receipt.result_ref, CommitRequired::Cancelled, true, session)
        .await
        .map(Some)
}

/// 加载受阻取消的结构化终态、唯一成功审计与历史任务事实，不使用当前请求字段筛选。
async fn load_cancel_blocked_terminal_facts(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
) -> Result<Option<CancelBlockedTerminalFacts>> {
    if instance.status != ApprovalProcessInstanceStatus::Cancelled
        || instance.current_node_execution_id.is_some()
        || instance.blocker_code.is_some()
        || instance.ended_at.is_none()
    {
        return Ok(None);
    }

    let audits = db
        .audit_logs()
        .list_successful_by_resource("approval_process_instance", &instance.base.id, executor)
        .await?;
    let matching_audits = audits
        .iter()
        .filter(|audit| audit.action == "approval.cancel_blocked")
        .filter_map(|audit| {
            let message = audit.message.as_deref()?;
            let (execution_id, reason) = message.strip_prefix("execution=")?.split_once(" reason=")?;
            (!execution_id.is_empty()).then(|| {
                (
                    audit.actor_id.clone(),
                    execution_id.to_string(),
                    reason.to_string(),
                )
            })
        })
        .collect::<Vec<_>>();
    let [(actor_id, execution_id, reason)] = matching_audits.as_slice() else {
        return Ok(None);
    };
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&ApprovalNodeExecutionId::new(execution_id), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != instance.base.id
        || execution.status != ApprovalNodeExecutionStatus::Cancelled
        || execution.ended_at != instance.ended_at
    {
        return Ok(None);
    }
    let Some(blocker) = execution.blocker_code else {
        return Ok(None);
    };
    if !requires_blocked_cancel(blocker) {
        return Ok(None);
    }

    let tasks = db
        .work_items()
        .approval_tasks_for_execution(&ApprovalNodeExecutionId::new(execution_id), executor)
        .await?;
    Ok(Some(CancelBlockedTerminalFacts {
        blocker,
        actor_id: actor_id.clone(),
        reason: reason.clone(),
        execution_version: execution.base.version,
        task_versions: tasks.into_iter().map(|task| task.base.version).collect(),
    }))
}

/// 只有原操作人、原因、版本与历史任务事实全部相同时，终态才证明当前取消载荷。
fn cancel_blocked_terminal_facts_match(
    instance: &ApprovalProcessInstance,
    facts: &CancelBlockedTerminalFacts,
    command: &ApprovalCancelBlockedCommand,
    actor_id: &str,
) -> bool {
    let Some(expected_instance_version) = command.expected_instance_version.checked_add(1) else {
        return false;
    };
    let Some(expected_execution_version) = command.expected_execution_version.checked_add(1) else {
        return false;
    };
    let task_identity_matches = match command.expected_task_version {
        Some(expected) => facts.task_versions.as_slice() == [expected],
        None => facts.task_versions.is_empty(),
    };
    instance.base.version == expected_instance_version
        && facts.execution_version == expected_execution_version
        && facts.actor_id == actor_id
        && facts.reason == command.reason
        && task_identity_matches
}

/// 复用 Fresh 受阻取消的实例版本与可恢复状态判定，隐藏收据是否存在。
fn ensure_cancel_blocked_instance_preconditions(
    instance: &ApprovalProcessInstance,
    command: &ApprovalCancelBlockedCommand,
) -> Result<()> {
    ensure_expected_version(
        "审批实例",
        command.expected_instance_version,
        instance.base.version,
    )?;
    let blocked = instance.status == ApprovalProcessInstanceStatus::Blocked;
    if !recovery_options_for(blocked, instance.blocker_code).contains(&RuntimeRecoveryAction::CancelBlocked) {
        return Err(Error::ConflictError("当前 blocker 不允许该恢复动作".to_string()));
    }
    Ok(())
}

/// 在同一事务内完成受阻取消的收据仲裁、授权、动作、运行时、通知与审计。
async fn cancel_blocked_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    action_port: &dyn ApprovalDomainActionPort,
    actor: &AuditActor,
    command: &ApprovalCancelBlockedCommand,
    idempotency_key: &IdempotencyKey,
    session: &mut mongodb::ClientSession,
) -> Result<ApprovalCommandView> {
    if let Some(replay) =
        replay_cancel_blocked_in_transaction(db, rbac, actor, command, idempotency_key, session).await?
    {
        return Ok(replay);
    }
    let instance_id = ApprovalProcessInstanceId::new(&command.approval_process_instance_id);
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let (document_type, snapshot) = load_exact_runtime_snapshot(db, &instance, session, false).await?;
    ensure_cancel_blocked_authorized(db, rbac, actor, document_type, &snapshot, session).await?;
    ensure_cancel_blocked_instance_preconditions(&instance, command)?;
    let task_policy = instance
        .cancellation_task_policy()
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    if task_policy.closes_open_task() {
        return Err(Error::ConflictError("受阻取消不得处理运行中审批实例".to_string()));
    }
    let current = db
        .bpm_workflow()
        .find_current_execution(&instance_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前受阻执行".to_string()))?;
    if current.process_instance_id != instance_id
        || instance.current_node_execution_id.as_ref()
            != Some(&ApprovalNodeExecutionId::new(&current.base.id))
        || current.status != ApprovalNodeExecutionStatus::Blocked
    {
        return Err(Error::ConflictError("受阻审批当前执行引用不一致".to_string()));
    }
    ensure_expected_version(
        "审批执行",
        command.expected_execution_version,
        current.base.version,
    )?;
    let execution_id = ApprovalNodeExecutionId::new(&current.base.id);
    let open_tasks = db
        .work_items()
        .open_approval_tasks_for_execution(&execution_id, session)
        .await?;
    task_policy
        .ensure_open_task_count(open_tasks.len())
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    validate_cancel_task_version_with_executor(db, &execution_id, command.expected_task_version, session)
        .await?;
    let spec = adapter_spec_of(document_type)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&instance.process_definition_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
    match (instance.blocker_code, current.blocker_code) {
        (Some(instance_blocker), Some(execution_blocker)) if instance_blocker == execution_blocker => {}
        _ => {
            return Err(Error::ConflictError(
                "受阻实例与当前执行 blocker 不一致".to_string(),
            ))
        }
    };
    let eligibility = converge_eligibility(
        current.assignee_participant_id.as_str(),
        &current.assignee_name_snapshot,
        None,
    )?;
    let now = Instant::now();
    let prepared = prepare_cancel(CancelExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: eligibility.clone(),
            next_eligibility: eligibility,
            receipt: None,
            idempotency_key: idempotency_key.clone(),
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance,
        current: current.clone(),
        subject_version: snapshot.subject_version,
        expected_instance_version: command.expected_instance_version,
        expected_execution_version: command.expected_execution_version,
        expected_task_version: command.expected_task_version,
        reason: command.reason.clone(),
        actor: ParticipantId::new(actor.id())
            .map_err(|_| Error::ValidationError("取消人引用无效".to_string()))?,
        close_open_task: false,
        blocked_port: true,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })?;
    let PreparedExecution::Apply(writes) = prepared else {
        return Err(Error::Internal("新受阻取消命令不得进入幂等回放分支".to_string()));
    };
    let writes = *writes;
    let action_context = ApprovalActionContext {
        approval_process_instance_id: command.approval_process_instance_id.clone(),
        approval_node_execution_id: Some(current.base.id.clone()),
        work_item_id: None,
        business_object_type: document_type.as_str().to_string(),
        business_object_id: snapshot.business_object_id.clone(),
        subject_version: snapshot.subject_version.to_string(),
        actor_id: actor.id().to_string(),
        reason: Some(command.reason.clone()),
        idempotency_key: command.idempotency_key.clone(),
    };
    let audit = actor.clone().resource_log_with_message(
        "approval.cancel_blocked",
        "approval_process_instance",
        command.approval_process_instance_id.clone(),
        Some(format!("execution={} reason={}", current.base.id, command.reason)),
    )?;

    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, session)
        .await
        .map_err(map_receipt_first_write_error)?;
    action_port
        .execute(spec.cancel_action, &action_context, actor, session)
        .await?;
    db.bpm_workflow()
        .persist_cancelled_runtime_after_receipt(&writes.instance, &writes.updated_executions, session)
        .await?;
    persist_cancel_notifications(
        db,
        &writes,
        &snapshot.payload.submitted_by,
        actor.id(),
        document_type.label(),
        &snapshot.payload.document_no,
        &current.node_name,
        &current.assignee_name_snapshot,
        now,
        session,
    )
    .await?;
    db.audit_logs().create(&audit, session).await?;
    Ok(map_command_view(
        &writes.instance,
        None,
        None,
        Some("DRAFT".to_string()),
        None,
        writes.commit,
        false,
    ))
}

/// 加载并校验实例、process_kind 与冻结快照的不可变主体三元组。
async fn load_exact_runtime_snapshot(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
    hide_mismatch: bool,
) -> Result<(DocumentType, ApprovalSubjectSnapshot)> {
    let mismatch = || {
        if hide_mismatch {
            hidden_not_found()
        } else {
            Error::ConflictError("审批实例与冻结业务快照不一致".to_string())
        }
    };
    let document_type =
        document_type_from_subject_kind(instance.subject.subject_kind()).map_err(|_| mismatch())?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(mismatch());
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(mismatch)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| mismatch())?;
    Ok((document_type, snapshot))
}

/// 事务内重验受阻取消账号、动作权限、类型级运行管理、对象读取与 DataScope。
async fn ensure_cancel_blocked_authorized(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    document_type: DocumentType,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !approval_actor_is_active_with_executor(db, actor, executor).await? {
        return Err(hidden_forbidden());
    }
    let action_scope = approval_cancel_blocked_scope_with_executor(db, rbac, actor, executor).await?;
    let read_scope =
        approval_document_read_scope_with_executor(db, rbac, actor, document_type, executor).await?;
    let visibility = definition_management_visibility_with_executor(db, rbac, actor, executor).await?;
    let spec = adapter_spec_of(document_type)?;
    let context = BindingRevalidationContext {
        organization_id: snapshot.payload.responsible_org_id.clone(),
        creator_id: snapshot.payload.submitted_by.clone(),
    };
    let read_scope_covers = !read_scope.is_empty() && read_scope.covers(&snapshot.payload.responsible_org_id);
    let object_readable = runtime_object_readable(&spec, &context, actor.id(), read_scope_covers)?;
    if action_scope.is_empty()
        || !action_scope.covers(&snapshot.payload.responsible_org_id)
        || !read_scope_covers
        || !visibility.runtime_admin_types().contains(&document_type)
        || !object_readable
    {
        return Err(hidden_forbidden());
    }
    Ok(())
}

/// 在调用方事务内校验受阻取消携带的可空历史任务版本。
async fn validate_cancel_task_version_with_executor(
    db: &Database,
    execution_id: &ApprovalNodeExecutionId,
    expected_task_version: Option<u64>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(expected) = expected_task_version else {
        return Ok(());
    };
    let tasks = db
        .work_items()
        .approval_tasks_for_execution(execution_id, executor)
        .await?;
    if tasks.len() != 1 {
        return Err(Error::ConflictError(
            "调用方声明了审批任务版本，但受阻执行未关联唯一历史任务".to_string(),
        ));
    }
    ensure_expected_version("审批任务", expected, tasks[0].base.version)
}

/// 执行与 Fresh 相同的任务前置；只有 `NotOpen` 才可继续证明终态回放。
fn decision_receipt_lookup_gate(
    item: &WorkItem,
    actor_id: &str,
    expected_task_version: u64,
) -> Result<DecisionReceiptLookup> {
    match item.approval_execution_for_decision(actor_id, expected_task_version) {
        Ok(_) => Ok(DecisionReceiptLookup::Fresh),
        Err(ApprovalDecisionTaskError::NotOpen) => item
            .approval_node_execution_id
            .clone()
            .map(DecisionReceiptLookup::Terminal)
            .ok_or_else(|| Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen)),
        Err(error) => Err(map_approval_task_error(error)),
    }
}

/// 终态任务的 Fresh 语义固定为稳定 `APPROVAL_TASK_NOT_OPEN`，不得暴露收据或授权差异。
fn decision_terminal_fresh_error() -> Error {
    Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen)
}

/// 决定回放先执行与 Fresh 相同的任务前置和当前授权，再允许查询和比较收据。
async fn replay_decision_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    session: &mut mongodb::ClientSession,
) -> Result<Option<RuntimeDecisionOutcome>> {
    let item = db
        .work_items()
        .find_document_approval_by_id(&command.work_item_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let execution_id = match decision_receipt_lookup_gate(&item, actor.id(), command.expected_task_version)? {
        DecisionReceiptLookup::Fresh => return Ok(None),
        DecisionReceiptLookup::Terminal(execution_id) => execution_id,
    };
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, session)
        .await?
        .ok_or_else(decision_terminal_fresh_error)?;
    let Some(original_actor_id) = decision_terminal_actor(&item, &execution) else {
        return Err(decision_terminal_fresh_error());
    };
    if original_actor_id != actor.id() {
        return Err(decision_terminal_fresh_error());
    }
    match authorize_decision_terminal_replay(db, rbac, actor, &execution, session).await {
        Ok(()) => {}
        Err(Error::Forbidden(_)) => {
            return Err(decision_terminal_fresh_error());
        }
        Err(error) => return Err(error),
    }
    let identity = decision_identity(
        command.idempotency_key.clone(),
        execution_id.as_ref(),
        &command.work_item_id,
        command.decision.as_str(),
        command.reason.as_deref(),
        command.expected_task_version,
        actor.id(),
    )?;
    let Some(receipt) = find_receipt_for_identity(db, &identity, session).await? else {
        return Err(decision_terminal_fresh_error());
    };
    // 历史无版本摘要可能存在分隔符碰撞，必须先证明收据仍指向该任务的冻结运行身份。
    let ended_execution =
        verify_decision_receipt_runtime_identity(db, &receipt, &execution_id, session).await?;
    let is_current_v3 = receipt.scope_id == identity.current().scope().as_str();
    if !is_current_v3 && !legacy_decision_terminal_facts_match(&item, &ended_execution, command, actor.id()) {
        return Err(payload_conflict_error());
    }
    match identity.classify(Some(&receipt)) {
        ReceiptBranch::SamePayload(_) => {}
        ReceiptBranch::Fresh => unreachable!("receipt was loaded"),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
    }
    let view =
        persisted_command_view_with_executor(db, &receipt.result_ref, CommitRequired::Proceed, true, session)
            .await?;
    Ok(Some(RuntimeDecisionOutcome {
        blocked: view.instance_status == ApprovalProcessInstanceStatus::Blocked.as_str(),
        view,
    }))
}

/// 在同一事务内执行决定的完整前置、授权、领域动作与运行时写入。
async fn submit_decision_in_transaction(
    db: &Database,
    rbac: &SharedRbacService,
    action_port: &dyn ApprovalDomainActionPort,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    session: &mut mongodb::ClientSession,
) -> Result<RuntimeDecisionOutcome> {
    if let Some(replay) = replay_decision_in_transaction(db, rbac, actor, command, session).await? {
        return Ok(replay);
    }

    let item = db
        .work_items()
        .find_document_approval_by_id(&command.work_item_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let execution_id = item
        .approval_execution_for_decision(actor.id(), command.expected_task_version)
        .map_err(map_approval_task_error)?
        .clone();
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let expected_execution_version = execution.base.version;
    let instance_id = execution.process_instance_id.clone();
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let expected_instance_version = instance.base.version;
    let document_type = document_type_from_subject_kind(instance.subject.subject_kind())?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(Error::ConflictError(
            "审批实例流程种类与单据类型不一致".to_string(),
        ));
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(instance_id.as_ref(), session)
        .await?
        .ok_or_else(hidden_not_found)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
    let spec = adapter_spec_of(document_type)?;
    let separation_policy = process_required_separation_policy(document_type)?;
    let subject = RuntimeReadSubject {
        instance: instance.clone(),
        current_execution: Some(execution.clone()),
        snapshot: snapshot.clone(),
        document_type,
    };
    if !task_proves_current_responsibility(&item, &execution, &subject, actor.id(), spec.owner_role.as_str())
    {
        return Err(Error::ConflictError(
            "APPROVAL_RESPONSIBILITY_CONFLICT".to_string(),
        ));
    }
    let open_tasks = db
        .work_items()
        .open_approval_tasks_for_execution(&execution_id, session)
        .await?;
    if open_tasks.is_empty() || !open_tasks.iter().any(|task| task.base.id == command.work_item_id) {
        return Err(Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen));
    }
    let instance_assignee = db
        .bpm_workflow()
        .find_assignee_for_node(&instance_id, &execution.node_key, session)
        .await?
        .ok_or_else(|| Error::ConflictError("实例缺少节点审批人绑定".to_string()))?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&instance.process_definition_id, session)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
    let current_eligibility = revalidate_decision_approver(
        db,
        rbac,
        actor.id(),
        &execution.assignee_name_snapshot,
        Some(actor),
        &snapshot,
        &spec,
        separation_policy,
        session,
    )
    .await?;
    let decision_target = graph
        .decision_target_node_key(&execution.node_key, command.decision)
        .map_err(map_runtime_graph_error)?;
    let next_eligibility = match decision_target {
        Some(node_key) => match graph.node(&node_key) {
            Some(node) => {
                revalidate_decision_approver(
                    db,
                    rbac,
                    node.assignee_participant_id.as_str(),
                    &node.assignee_label_snapshot,
                    None,
                    &snapshot,
                    &spec,
                    separation_policy,
                    session,
                )
                .await?
            }
            None => return Err(Error::ConflictError("审批定义缺少目标节点".to_string())),
        },
        None => current_eligibility.clone(),
    };
    let now = Instant::now();
    let prepared = prepare_decision(DecisionExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility,
            next_eligibility,
            receipt: None,
            idempotency_key: command.idempotency_key.clone(),
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance,
        current: execution.clone(),
        work_item_id: command.work_item_id.clone(),
        task_owner_id: item.owner_user_id.clone().unwrap_or_default(),
        instance_assignee_id: instance_assignee
            .current_assignee_participant_id
            .as_str()
            .to_string(),
        decision: command.decision,
        reason: command.reason.clone(),
        expected_task_version: command.expected_task_version,
        actor: ParticipantId::new(actor.id())
            .map_err(|_| Error::ValidationError("决定人引用无效".to_string()))?,
        next_execution_id: ApprovalNodeExecutionId::new(next_id()),
        next_execution_no: execution
            .execution_no
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("审批执行序号溢出".to_string()))?,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        open_task_count: open_tasks.len(),
    })?;
    let PreparedExecution::Apply(writes) = prepared else {
        return Err(Error::Internal("新决定命令不得进入幂等回放分支".to_string()));
    };
    let writes = *writes;
    let actor_id = actor.id().to_string();
    let owner_role = spec.owner_role.as_str().to_string();
    let owner_organization_id = snapshot.payload.responsible_org_id.clone();
    let subject_version = writes.instance.subject_version.to_string();
    let business_object_id = writes.instance.subject.subject_id().to_string();
    let document_type_label = document_type.label().to_string();
    let runtime_admin_ids = if writes.notifications.iter().any(|intent| {
        intent.event_kind == entities::approval_integration::ApprovalNotificationEventKind::Blocked
    }) {
        runtime_admin_notification_recipients(db, rbac, document_type, &snapshot, session).await?
    } else {
        Vec::new()
    };
    let should_finalize = writes.commit == CommitRequired::TerminalApproved;
    let action_context = ApprovalActionContext {
        approval_process_instance_id: instance_id.to_string(),
        approval_node_execution_id: Some(execution_id.to_string()),
        work_item_id: Some(command.work_item_id.clone()),
        business_object_type: document_type.as_str().to_string(),
        business_object_id: business_object_id.clone(),
        subject_version: subject_version.clone(),
        actor_id: actor_id.clone(),
        reason: command.reason.clone(),
        idempotency_key: command.idempotency_key.as_str().to_string(),
    };
    let new_task_ids: Vec<String> = writes.create_tasks.iter().map(|_| next_id()).collect();
    let list_projection =
        list_projection_from_writes(&writes, execution_id.as_ref(), command.reason.clone(), now);
    let audit = actor.clone().resource_log_with_message(
        "approval.decide",
        "approval_process_instance",
        instance_id.to_string(),
        Some(format!(
            "decision={} reason={:?} work_item={}",
            command.decision.as_str(),
            command.reason,
            command.work_item_id
        )),
    )?;

    // 收据唯一键先于任何领域写入仲裁同键并发；后续失败会随事务整体回滚。
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, session)
        .await
        .map_err(map_receipt_first_write_error)?;
    if should_finalize {
        action_port
            .execute(spec.on_final_approve, &action_context, actor, session)
            .await?;
    }
    persist_decision_writes(
        db,
        &writes,
        execution_id.as_ref(),
        expected_instance_version,
        expected_execution_version,
        command.expected_task_version,
        &command.work_item_id,
        &new_task_ids,
        &list_projection,
        &audit,
        now,
        &actor_id,
        &owner_role,
        &owner_organization_id,
        &subject_version,
        &business_object_id,
        &document_type_label,
        &snapshot.payload.document_no,
        &snapshot.payload.submitted_by,
        &execution,
        command.reason.as_deref(),
        &runtime_admin_ids,
        session,
    )
    .await?;
    let blocked = writes.commit == CommitRequired::Blocked;
    let view = map_command_view(
        &writes.instance,
        writes.created_executions.last(),
        command.reason.clone(),
        None,
        first_open_task(&writes, &new_task_ids),
        writes.commit,
        false,
    );
    Ok(RuntimeDecisionOutcome { view, blocked })
}

/// 在 legacy 摘要双读前验证收据、任务执行、实例、流程类型与冻结主体属于同一运行身份。
async fn verify_decision_receipt_runtime_identity(
    db: &Database,
    receipt: &ApprovalCommandReceipt,
    expected_execution_id: &ApprovalNodeExecutionId,
    session: &mut mongodb::ClientSession,
) -> Result<ApprovalNodeExecution> {
    if receipt.scope_id != expected_execution_id.as_ref() {
        return Err(hidden_not_found());
    }
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(expected_execution_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), session)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != receipt.result_ref || instance.base.id != receipt.result_ref
    {
        return Err(hidden_not_found());
    }
    let document_type =
        document_type_from_subject_kind(instance.subject.subject_kind()).map_err(|_| hidden_not_found())?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| hidden_not_found())?;
    Ok(execution)
}

/// 从任务不可逆终态与执行事实提取原决定人；不得把当前 owner 投影作为回放授权。
fn decision_terminal_actor<'a>(item: &'a WorkItem, execution: &'a ApprovalNodeExecution) -> Option<&'a str> {
    if item.approval_node_execution_id.as_ref()
        != Some(&ApprovalNodeExecutionId::new(execution.base.id.clone()))
    {
        return None;
    }
    match item.status {
        WorkItemStatus::Completed => {
            let completed_by = item.completed_by.as_deref()?;
            let decided_by = execution.decided_by.as_ref()?.as_str();
            let terminal_decision_matches = matches!(
                (execution.status, execution.decision),
                (
                    ApprovalNodeExecutionStatus::Approved,
                    Some(ApprovalDecision::Approve)
                ) | (
                    ApprovalNodeExecutionStatus::Rejected,
                    Some(ApprovalDecision::Reject)
                )
            );
            (terminal_decision_matches
                && completed_by == decided_by
                && execution.decided_at.is_some()
                && execution.ended_at.is_some())
            .then_some(completed_by)
        }
        WorkItemStatus::Closed => {
            let closed_by = item.closed_by.as_deref()?;
            (item.close_reason.as_deref() == Some(TaskCloseReason::ApprovalRuntimeBlocked.as_str())
                && execution.status == ApprovalNodeExecutionStatus::Blocked
                && execution.blocker_code.is_some()
                && execution.blocked_at.is_some()
                && execution.ended_at.is_none()
                && execution.assignee_participant_id.as_str() == closed_by)
                .then_some(closed_by)
        }
        WorkItemStatus::Open => None,
    }
}

/// legacy 决定摘要只有在不可变终态执行与任务完整证明原命令时才允许回放。
fn legacy_decision_terminal_facts_match(
    item: &WorkItem,
    execution: &ApprovalNodeExecution,
    command: &RuntimeDecisionCommand,
    actor_id: &str,
) -> bool {
    let Some(persisted_task_version) = command.expected_task_version.checked_add(1) else {
        return false;
    };
    let expected_execution_status = match command.decision {
        ApprovalDecision::Approve => ApprovalNodeExecutionStatus::Approved,
        ApprovalDecision::Reject => ApprovalNodeExecutionStatus::Rejected,
    };
    item.base.id == command.work_item_id
        && item.approval_node_execution_id.as_ref()
            == Some(&ApprovalNodeExecutionId::new(execution.base.id.clone()))
        && item.status == WorkItemStatus::Completed
        && item.completed_by.as_deref() == Some(actor_id)
        && item.base.version == persisted_task_version
        && execution.status == expected_execution_status
        && execution.decision == Some(command.decision)
        && execution.decision_reason.as_deref() == command.reason.as_deref()
        && execution.decided_by.as_ref().map(ParticipantId::as_str) == Some(actor_id)
        && execution.decided_at.is_some()
        && execution.ended_at.is_some()
}

/// 回放只使用冻结运行事实证明原决定人当前仍具备动作权限，不信任收据或 WorkItem owner 投影。
async fn authorize_decision_terminal_replay(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    execution: &ApprovalNodeExecution,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&execution.process_instance_id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.process_instance_id.as_ref() != instance.base.id {
        return Err(hidden_not_found());
    }
    let document_type =
        document_type_from_subject_kind(instance.subject.subject_kind()).map_err(|_| hidden_not_found())?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, session)
        .await?
        .ok_or_else(hidden_not_found)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| hidden_not_found())?;
    let assignee = db
        .bpm_workflow()
        .find_assignee_for_node(
            &ApprovalProcessInstanceId::new(&instance.base.id),
            &execution.node_key,
            session,
        )
        .await?
        .ok_or_else(hidden_not_found)?;
    if execution.assignee_participant_id.as_str() != actor.id()
        || assignee.current_assignee_participant_id.as_str() != actor.id()
    {
        return Err(hidden_forbidden());
    }
    let spec = adapter_spec_of(document_type)?;
    let eligibility = revalidate_decision_approver(
        db,
        rbac,
        actor.id(),
        &execution.assignee_name_snapshot,
        Some(actor),
        &snapshot,
        &spec,
        process_required_separation_policy(document_type)?,
        session,
    )
    .await?;
    if eligibility.blocked_code().is_some() {
        return Err(hidden_forbidden());
    }
    Ok(())
}

/// 按冻结快照重验审批人账号、动作权限、对象读取、DataScope 与岗位分离。
#[allow(clippy::too_many_arguments)]
async fn revalidate_decision_approver(
    db: &Database,
    rbac: &SharedRbacService,
    assignee_id: &str,
    assignee_name: &str,
    authenticated_actor: Option<&AuditActor>,
    snapshot: &ApprovalSubjectSnapshot,
    spec: &crate::approval::business_adapter::ApprovalAdapterSpec,
    separation_policy: SeparationOfDutiesPolicy,
    executor: &mut dyn Executor,
) -> Result<Eligibility> {
    let account = db
        .accounts()
        .find_approval_assignee_by_id(assignee_id, executor)
        .await?;
    let failure = match account {
        None => Some(AuthorizationFailure::AccountInactive),
        Some(account)
            if !account.is_active_backoffice()
                || authenticated_actor
                    .is_some_and(|actor| actor.id() != account.base.id || actor.kind() != account.kind) =>
        {
            Some(AuthorizationFailure::AccountInactive)
        }
        Some(account) => {
            let scope_actor = authenticated_actor.cloned().unwrap_or_else(|| {
                AuditActor::new(account.base.id.clone(), account.base.id.clone(), account.kind)
            });
            let decide_scope = approval_decide_scope_with_executor(db, rbac, &scope_actor, executor).await?;
            if decide_scope.is_empty() {
                Some(AuthorizationFailure::NotEligible)
            } else if !decide_scope.covers(&snapshot.payload.responsible_org_id) {
                Some(AuthorizationFailure::OutOfDataScope)
            } else {
                let read_scope = approval_document_read_scope_with_executor(
                    db,
                    rbac,
                    &scope_actor,
                    snapshot.document_type,
                    executor,
                )
                .await?;
                if read_scope.is_empty() {
                    Some(AuthorizationFailure::CannotReadSubject)
                } else if !read_scope.covers(&snapshot.payload.responsible_org_id) {
                    Some(AuthorizationFailure::OutOfDataScope)
                } else {
                    let context = BindingRevalidationContext {
                        organization_id: snapshot.payload.responsible_org_id.clone(),
                        creator_id: snapshot.payload.submitted_by.clone(),
                    };
                    match runtime_object_readable(spec, &context, assignee_id, true)? {
                        true => {
                            if ensure_separation_of_duties(
                                separation_policy,
                                &snapshot.payload.submitted_by,
                                &[assignee_id.to_string()],
                            )
                            .is_err()
                            {
                                Some(AuthorizationFailure::SeparationOfDuties)
                            } else {
                                None
                            }
                        }
                        false => Some(AuthorizationFailure::CannotReadSubject),
                    }
                }
            }
        }
    };
    converge_eligibility(assignee_id, assignee_name, failure)
}

/// 按当前已签署对象读取端口判定审批运行可读性。
///
/// `StockAdjustment` 的真实读取端口是 Entity 登记的
/// `stock_adjustment:detail` 与当前组织 DataScope 交集；不得回退到已删除的
/// 常量 helper。其它类型仍要求业务 Adapter 显式接线。
fn runtime_object_readable(
    spec: &crate::approval::business_adapter::ApprovalAdapterSpec,
    context: &BindingRevalidationContext,
    actor_id: &str,
    read_scope_covers: bool,
) -> Result<bool> {
    if spec.document_type == DocumentType::StockAdjustment {
        return Ok(read_scope_covers);
    }
    Ok(adapter_object_read_decision(spec, context, actor_id)?.unwrap_or(false))
}

/// 读取必须审批政策唯一签署的岗位分离规则。
fn process_required_separation_policy(document_type: DocumentType) -> Result<SeparationOfDutiesPolicy> {
    match policy_of(document_type)? {
        DocumentApprovalPolicy::ProcessRequired(policy) => Ok(policy.separation_of_duties_policy),
        DocumentApprovalPolicy::NoApproval(_) => {
            Err(Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered))
        }
    }
}

/// 恢复端口只接受已重新满足全部资格的原审批人。
fn ensure_resume_approver_recovered(eligibility: &Eligibility) -> Result<()> {
    if eligibility.blocked_code().is_some() {
        return Err(Error::from_approval_code(
            ErrorCode::ApprovalCurrentApproverNotRecovered,
        ));
    }
    Ok(())
}

/// 以调用方事务执行器读取最新运行视图；回放不依赖原任务仍为 OPEN。
async fn persisted_command_view_with_executor(
    db: &Database,
    instance_id: &str,
    commit: CommitRequired,
    replay: bool,
    executor: &mut dyn Executor,
) -> Result<ApprovalCommandView> {
    let instance_id = ApprovalProcessInstanceId::new(instance_id);
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let current = db
        .bpm_workflow()
        .find_current_execution(&instance_id, executor)
        .await?;
    let next_open_task = match current.as_ref() {
        Some(execution) => {
            let tasks = db
                .work_items()
                .open_approval_tasks_for_execution(
                    &ApprovalNodeExecutionId::new(execution.base.id.clone()),
                    executor,
                )
                .await?;
            if tasks.len() > 1 {
                return Err(Error::ConflictError("当前执行关联多个开放审批任务".to_string()));
            }
            tasks.into_iter().next().map(|task| OpenTaskSummary {
                work_item_id: task.base.id,
                task_version: task.base.version.to_string(),
                owner_user_id: task.owner_user_id.unwrap_or_default(),
            })
        }
        None => None,
    };
    Ok(map_command_view(
        &instance,
        current.as_ref(),
        None,
        None,
        next_open_task,
        commit,
        replay,
    ))
}

/// 按当前 V3 scope 优先、已知历史 scope 次之读取唯一命令收据。
///
/// scope 与 digest 的完整成对判定由 [`PreparedCommandIdentity::classify`] 完成；
/// 当前 scope 一旦存在收据，调用方不得继续向历史 scope 降级。
async fn find_receipt_for_identity(
    db: &Database,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    for scope in identity.scope_candidates() {
        if let Some(receipt) = db
            .bpm_workflow()
            .find_command_receipt(
                identity.current().command_kind(),
                scope,
                identity.idempotency_key(),
                executor,
            )
            .await?
        {
            return Ok(Some(receipt));
        }
    }
    Ok(None)
}

/// 将单据审批任务前置校验映射为稳定的 Service 错误。
///
/// # 参数
/// * `error` - WorkItem 返回的决定前置失败原因
///
/// # 返回
/// 返回权限拒绝或稳定冲突错误。
///
/// # 错误
/// 无；本方法只执行错误分类转换。
///
/// # 关键业务约束
/// 非当前责任人必须保持禁止语义，任务状态、版本和执行引用失败必须保持冲突语义。
fn map_approval_task_error(error: ApprovalDecisionTaskError) -> Error {
    match error {
        ApprovalDecisionTaskError::NotCurrentOwner => Error::Forbidden("无权执行该审批动作".to_string()),
        ApprovalDecisionTaskError::VersionConflict => {
            Error::ConflictError("任务版本已变化，请刷新后重试".to_string())
        }
        ApprovalDecisionTaskError::NotDocumentApproval
        | ApprovalDecisionTaskError::NotOpen
        | ApprovalDecisionTaskError::MissingExecution => {
            Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen)
        }
    }
}

/// 将 BPM 决定连线错误映射为运行时冲突。
///
/// # 参数
/// * `error` - BPM 图模型返回的决定连线错误
///
/// # 返回
/// 返回失败关闭的运行时冲突错误。
///
/// # 错误
/// 无；本方法只执行错误分类转换。
///
/// # 关键业务约束
/// 持久化图缺失或重复决定连线时不得继续推导下一审批人。
fn map_runtime_graph_error(error: ModelError) -> Error {
    Error::ConflictError(error.to_string())
}

/// 校验当前执行与实例持有的运行令牌完全一致。
fn current_execution_matches_instance(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
) -> bool {
    match (&instance.current_node_execution_id, current) {
        (None, None) => true,
        (Some(expected), Some(execution)) => {
            expected.as_ref() == execution.base.id
                && execution.process_instance_id.as_ref() == instance.base.id
        }
        _ => false,
    }
}

/// 普通详情/历史读取允许发起人、当前责任人，或对象读取与 DataScope 同时成立。
fn ordinary_runtime_read_allowed(facts: RuntimeReadAuthorizationFacts) -> bool {
    facts.actor_active
        && (facts.initiator || facts.current_responsibility || (facts.object_readable && facts.scope_covers))
}

/// 管理读取必须同时具备类型级运行管理、对象读取与 DataScope。
fn management_runtime_read_allowed(facts: RuntimeReadAuthorizationFacts) -> bool {
    facts.actor_active && facts.runtime_admin && facts.object_readable && facts.scope_covers
}

/// Started 视图由 BPM 启动人事实独立证明普通读取权。
fn started_runtime_read_allowed(facts: RuntimeReadAuthorizationFacts) -> bool {
    facts.actor_active && facts.initiator
}

/// 当前开放审批任务是否精确证明 actor 对运行实例的当前责任。
fn task_proves_current_responsibility(
    task: &WorkItem,
    execution: &ApprovalNodeExecution,
    subject: &RuntimeReadSubject,
    actor_id: &str,
    expected_owner_role: &str,
) -> bool {
    subject.instance.status == ApprovalProcessInstanceStatus::Running
        && execution.status == ApprovalNodeExecutionStatus::Active
        && execution.round_no == subject.instance.current_round_no
        && execution.assignee_participant_id.as_str() == actor_id
        && task.work_item_type == WorkItemType::DocumentApproval
        && task.status == WorkItemStatus::Open
        && task.assignment_source == AssignmentSource::ApprovalRuntime
        && task.owner_user_id.as_deref() == Some(actor_id)
        && task.owner_role == expected_owner_role
        && task.owner_organization_id == subject.snapshot.payload.responsible_org_id
        && task.approval_node_execution_id.as_ref().is_some_and(|id| {
            id.as_ref() == execution.base.id
                && subject.instance.current_node_execution_id.as_ref() == Some(id)
        })
        && task.business_object_type == subject.document_type.as_str()
        && task.business_object_id == subject.instance.subject.subject_id()
        && task.subject_version == subject.instance.subject_version.to_string()
        && execution.process_instance_id.as_ref() == subject.instance.base.id
        && execution.node_key.trim() == execution.node_key
        && !execution.node_key.is_empty()
}

/// Mine 页的 WorkItem、当前 execution 与实例摘要是否构成同一当前责任链。
fn mine_runtime_chain_matches(
    task: &WorkItem,
    execution: &ApprovalNodeExecution,
    summary: &ApprovalInstanceSummary,
    snapshot: Option<&ApprovalSubjectSnapshot>,
    actor_id: &str,
) -> Result<bool> {
    let document_type =
        document_type_from_subject_kind(summary.subject.subject_kind()).map_err(|_| hidden_not_found())?;
    let spec = adapter_spec_of(document_type)?;
    let canonical_subject_version = summary.subject_version.to_string();
    let runtime_chain_matches = task.work_item_type == WorkItemType::DocumentApproval
        && task.status == WorkItemStatus::Open
        && task.assignment_source == AssignmentSource::ApprovalRuntime
        && task.owner_user_id.as_deref() == Some(actor_id)
        && task.owner_role == spec.owner_role.as_str()
        && task.approval_node_execution_id.as_ref().is_some_and(|id| {
            id.as_ref() == execution.base.id && summary.current_node_execution_id.as_ref() == Some(id)
        })
        && task.business_object_type == document_type.as_str()
        && task.business_object_id == summary.subject.subject_id()
        && task.subject_version == canonical_subject_version
        && summary.process_kind == process_kind_of(document_type)
        && summary.status == ApprovalProcessInstanceStatus::Running
        && execution.status == ApprovalNodeExecutionStatus::Active
        && execution.process_instance_id.as_ref() == summary.id
        && execution.round_no == summary.current_round_no
        && execution.assignee_participant_id.as_str() == actor_id
        && summary.current_node_key.as_deref() == Some(execution.node_key.as_str())
        && summary.current_node_name.as_deref() == Some(execution.node_name.as_str())
        && summary.current_assignee_participant_id.as_deref()
            == Some(execution.assignee_participant_id.as_str())
        && summary.current_assignee_name.as_deref() == Some(execution.assignee_name_snapshot.as_str());
    if !runtime_chain_matches {
        return Ok(false);
    }
    let snapshot_owner_matches = snapshot
        .filter(|snapshot| snapshot.approval_process_instance_id.as_ref() == summary.id)
        .filter(|snapshot| {
            snapshot
                .ensure_matches_runtime_subject(
                    document_type,
                    summary.subject.subject_id(),
                    summary.subject_version,
                )
                .is_ok()
        })
        .is_none_or(|snapshot| snapshot.payload.responsible_org_id == task.owner_organization_id);
    Ok(snapshot_owner_matches)
}

/// 提取 Mine 当前页的 execution ID，并在服务边界拒绝重复责任投影。
///
/// 不得依赖唯一索引或静默去重；重复 WorkItem 会同时污染当前页行与
/// Repository 返回的 total，因此必须按隐藏实例存在性的稳定语义整页失败。
fn mine_execution_ids(tasks: &[WorkItem]) -> Result<Vec<ApprovalNodeExecutionId>> {
    let mut seen = HashSet::with_capacity(tasks.len());
    let mut execution_ids = Vec::with_capacity(tasks.len());
    for task in tasks {
        let execution_id = task
            .approval_node_execution_id
            .clone()
            .ok_or_else(hidden_not_found)?;
        if !seen.insert(execution_id.to_string()) {
            return Err(hidden_not_found());
        }
        execution_ids.push(execution_id);
    }
    Ok(execution_ids)
}

/// 把 Repository 在完整过滤集合中发现的责任链冲突映射为隐藏式拒绝。
fn ensure_mine_page_integrity(conflict_count: usize) -> Result<()> {
    if conflict_count == 0 {
        return Ok(());
    }
    Err(hidden_not_found())
}

/// 由 Mine 当前页 execution 解析实例 ID，并拒绝两个执行指向同一实例。
///
/// 执行结果缺失或实例重复均表示当前责任链不能形成唯一列表行，整页失败关闭。
fn mine_instance_ids(
    execution_ids: &[ApprovalNodeExecutionId],
    execution_by_id: &HashMap<String, ApprovalNodeExecution>,
) -> Result<Vec<ApprovalProcessInstanceId>> {
    let mut seen = HashSet::with_capacity(execution_ids.len());
    let mut instance_ids = Vec::with_capacity(execution_ids.len());
    for execution_id in execution_ids {
        let instance_id = execution_by_id
            .get(execution_id.as_ref())
            .map(|execution| execution.process_instance_id.clone())
            .ok_or_else(hidden_not_found)?;
        if !seen.insert(instance_id.to_string()) {
            return Err(hidden_not_found());
        }
        instance_ids.push(instance_id);
    }
    Ok(instance_ids)
}

/// 将批量读取结果按稳定 ID 建表；重复 ID 按持久化身份损坏失败关闭。
fn unique_by_id<T, F>(items: Vec<T>, key_of: F) -> Result<HashMap<String, T>>
where
    F: Fn(&T) -> String,
{
    let mut by_id = HashMap::with_capacity(items.len());
    for item in items {
        let key = key_of(&item);
        if by_id.insert(key, item).is_some() {
            return Err(hidden_not_found());
        }
    }
    Ok(by_id)
}

/// 返回政策矩阵中必须接入审批运行时的固定单据类型。
fn process_required_document_types() -> Result<Vec<DocumentType>> {
    let mut document_types = Vec::new();
    for document_type in ALL_DOCUMENT_TYPES {
        if matches!(
            policy_of(document_type)?,
            DocumentApprovalPolicy::ProcessRequired(_)
        ) {
            document_types.push(document_type);
        }
    }
    Ok(document_types)
}

/// 隐藏实例存在性。
fn hidden_not_found() -> Error {
    Error::NotFound("审批实例不存在".to_string())
}

/// 校验协议命令只能由当前认证主体执行。
fn ensure_command_actor(actor: &AuditActor, command_actor_id: &str) -> Result<()> {
    if actor.id() == command_actor_id {
        return Ok(());
    }
    Err(Error::Forbidden("审批命令操作人与认证主体不一致".to_string()))
}

/// 校验调用方持有的乐观锁版本。
fn ensure_expected_version(label: &str, expected: u64, actual: u64) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{label}版本已变化，请刷新后重试")))
}

/// CAS 未应用时失败关闭。
///
/// # 错误
/// 未找到、版本冲突或状态已变时返回冲突。
fn require_cas_applied<T>(outcome: database::repository::bpm::CasWriteOutcome<T>, label: &str) -> Result<()> {
    match outcome {
        database::repository::bpm::CasWriteOutcome::Applied(_) => Ok(()),
        _ => Err(Error::ConflictError(format!(
            "{label}已被其他请求修改，请刷新后重试"
        ))),
    }
}

/// 解析实例列表筛选中的单据类型稳定码。
///
/// # 参数
/// * `code` - 调用方提供的单据类型稳定代码
///
/// # 返回
/// 精确命中登记代码时返回对应单据类型。
///
/// # 错误
/// 未登记代码返回原有校验错误文本。
///
/// # 关键业务约束
/// Service 不裁剪、不接受别名，也不维护第二份代码注册表。
fn parse_document_type(code: &str) -> Result<DocumentType> {
    DocumentType::try_from_code(code).map_err(|_| Error::ValidationError(format!("未登记单据类型: {code}")))
}

/// 构造实例列表的授权过滤与稳定游标。
///
/// # 参数
/// * `actor` - 当前账号
/// * `query` - 已规范化列表查询
///
/// # 返回
/// 返回仓储过滤条件；无检索串时 `text_query` 为空。
///
/// # 错误
/// 单据类型未登记时返回校验错误。
///
/// # 关键业务约束
/// `Started` 必须带 `started_by`。检索在仓储内施加，不得先分页再内存过滤。
fn instance_list_filter(
    actor: &AuditActor,
    query: &RuntimeInstanceListQuery,
) -> Result<ApprovalInstanceListFilter> {
    let view = match query.view {
        RuntimeInstanceListView::Started => ApprovalInstanceListView::Started,
        RuntimeInstanceListView::Blocked => ApprovalInstanceListView::Blocked,
        _ => ApprovalInstanceListView::Managed,
    };
    let process_kind = query
        .document_type
        .as_deref()
        .map(parse_document_type)
        .transpose()?
        .map(process_kind_of);
    Ok(ApprovalInstanceListFilter {
        view,
        process_kind,
        status: query.status.map(map_status_filter),
        started_by: (query.view == RuntimeInstanceListView::Started).then(|| actor.id().to_string()),
        subject_kind: None,
        subject_ids: None,
        text_query: query
            .query
            .as_ref()
            .map(|text| ApprovalInstanceTextQuery { query: text.clone() }),
        cursor: query
            .cursor
            .as_ref()
            .map(|cursor| database::repository::bpm::ApprovalInstanceListCursor {
                sort_time: cursor.sort_time,
                id: cursor.id.clone(),
            }),
        limit: query.limit,
    })
}

/// 从当前视图最后一行生成下一页游标。
fn cursor_from_summary(
    view: ApprovalInstanceListView,
    row: &ApprovalInstanceSummary,
) -> RuntimeInstanceListCursor {
    let updated_at = i64::try_from(row.updated_at).unwrap_or(i64::MAX);
    let sort_time = match view {
        ApprovalInstanceListView::Started => row.started_at,
        ApprovalInstanceListView::Blocked => row.blocked_at.unwrap_or(updated_at),
        ApprovalInstanceListView::Managed => updated_at,
    };
    RuntimeInstanceListCursor {
        sort_time,
        id: row.id.clone(),
    }
}

/// 映射列表状态过滤。
fn map_status_filter(
    status: RuntimeInstanceStatusFilter,
) -> bpm::model::types::ApprovalProcessInstanceStatus {
    match status {
        RuntimeInstanceStatusFilter::Running => bpm::model::types::ApprovalProcessInstanceStatus::Running,
        RuntimeInstanceStatusFilter::Approved => bpm::model::types::ApprovalProcessInstanceStatus::Approved,
        RuntimeInstanceStatusFilter::Cancelled => bpm::model::types::ApprovalProcessInstanceStatus::Cancelled,
        RuntimeInstanceStatusFilter::Blocked => bpm::model::types::ApprovalProcessInstanceStatus::Blocked,
    }
}

/// 由仓储摘要与启动快照映射列表行。
fn item_from_summary(
    row: ApprovalInstanceSummary,
    snapshot: Option<&ApprovalSubjectSnapshot>,
) -> Result<RuntimeInstanceListItem> {
    let document_type =
        document_type_from_subject_kind(row.subject.subject_kind()).map_err(|_| hidden_not_found())?;
    let document_id = row.subject.subject_id().to_string();
    if row.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    let document_label = snapshot
        .filter(|snapshot| snapshot.approval_process_instance_id.as_ref() == row.id.as_str())
        .filter(|snapshot| {
            snapshot
                .ensure_matches_runtime_subject(document_type, &document_id, row.subject_version)
                .is_ok()
        })
        .map(|snapshot| snapshot.payload.document_no.clone());
    Ok(RuntimeInstanceListItem {
        instance_id: row.id,
        status: row.status.as_str().to_string(),
        current_round_no: row.current_round_no,
        current_node_key: row.current_node_key,
        current_node_name: row.current_node_name,
        current_assignee_participant_id: row.current_assignee_participant_id,
        current_assignee_name: row.current_assignee_name,
        document_type: Some(document_type.as_str().to_string()),
        document_id: Some(document_id),
        document_label,
        process_version: Some(row.definition_version),
        started_at: Some(row.started_at),
        latest_rejection_summary: row.latest_rejection_summary,
    })
}

/// 对聚合页行重新执行对象读取与授权矩阵，然后映射公开列表行。
fn item_from_runtime_read_row(
    row: ApprovalRuntimeReadRow,
    actor: &AuditActor,
    view: RuntimeInstanceListView,
    type_scopes: &[ApprovalRuntimeReadTypeScope],
) -> Result<RuntimeInstanceListItem> {
    let document_type = document_type_from_subject_kind(row.instance.subject.subject_kind())
        .map_err(|_| hidden_not_found())?;
    let type_allowed = type_scopes
        .iter()
        .any(|scope| scope.process_kind == row.instance.process_kind);
    let facts = RuntimeReadAuthorizationFacts {
        actor_active: true,
        initiator: row.instance.started_by == actor.id(),
        current_responsibility: false,
        object_readable: type_allowed,
        scope_covers: type_allowed,
        runtime_admin: type_allowed,
    };
    let allowed = match view {
        RuntimeInstanceListView::Started => started_runtime_read_allowed(facts),
        RuntimeInstanceListView::Managed | RuntimeInstanceListView::Blocked => {
            management_runtime_read_allowed(facts)
        }
        RuntimeInstanceListView::Mine => false,
    };
    if !allowed {
        return Err(hidden_not_found());
    }
    if row.instance.process_kind != process_kind_of(document_type) {
        return Err(hidden_not_found());
    }
    item_from_summary(row.instance, row.snapshot.as_ref())
}

/// 由实例字段构造列表行。
fn item_from_instance_id(
    instance_id: &str,
    status: &str,
    current_round_no: u32,
    current_node_key: Option<String>,
    current_node_name: Option<String>,
    current_assignee_participant_id: Option<String>,
) -> RuntimeInstanceListItem {
    RuntimeInstanceListItem {
        instance_id: instance_id.to_string(),
        status: status.to_string(),
        current_round_no,
        current_node_key,
        current_node_name,
        current_assignee_participant_id,
        current_assignee_name: None,
        document_type: None,
        document_id: None,
        document_label: None,
        process_version: None,
        started_at: None,
        latest_rejection_summary: None,
    }
}

/// 由决定后的写入构造实例列表投影。
fn list_projection_from_writes(
    writes: &PlannedWrites,
    ended_execution_id: &str,
    reject_reason: Option<String>,
    now: Instant,
) -> ApprovalInstanceListProjection {
    let current = writes.created_executions.last();
    ApprovalInstanceListProjection {
        current_node_key: current.map(|item| item.node_key.clone()),
        current_node_name: current.map(|item| item.node_name.clone()),
        current_assignee_participant_id: current
            .map(|item| item.assignee_participant_id.as_str().to_string()),
        current_assignee_name: current.map(|item| item.assignee_name_snapshot.clone()),
        latest_rejected_execution_id: reject_reason.as_ref().map(|_| ended_execution_id.to_string()),
        latest_rejection_summary: reject_reason,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// 在受阻取消事务内追加通知 outbox。
#[allow(clippy::too_many_arguments)]
async fn persist_cancel_notifications(
    db: &Database,
    writes: &PlannedWrites,
    submitted_by: &str,
    actor_id: &str,
    document_type_label: &str,
    document_no: &str,
    current_node_name: &str,
    current_approver_display_name: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    if !writes.create_tasks.is_empty() || !writes.complete_tasks.is_empty() || !writes.close_tasks.is_empty()
    {
        return Err(Error::Internal(
            "受阻取消计划不得创建、完成或关闭审批任务".to_string(),
        ));
    }
    let [intent] = writes.notifications.as_slice() else {
        return Err(Error::Internal("受阻取消必须且只能产生一条通知意图".to_string()));
    };
    let expected_dedup = format!("blocked_cancelled:{}", writes.instance.base.id);
    if intent.event_kind != entities::approval_integration::ApprovalNotificationEventKind::BlockedCancelled
        || intent.dedup_key != expected_dedup
    {
        return Err(Error::Internal("受阻取消通知意图不匹配".to_string()));
    }
    let record = entities::approval_integration::ApprovalNotificationOutbox::enqueue(
        entities::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
        intent.dedup_key.clone(),
        intent.event_kind,
        blocked_cancel_notification_recipients(submitted_by, actor_id),
        entities::approval_integration::ApprovalNotificationTemplateParams {
            document_type_label: document_type_label.to_string(),
            document_no: document_no.to_string(),
            current_node_name: current_node_name.to_string(),
            current_approver_display_name: current_approver_display_name.to_string(),
            round_no: writes.instance.current_round_no,
            reject_reason_summary: None,
        },
        now,
    )
    .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.approval_notification_outbox().create(&record, session).await?;
    Ok(())
}

/// 受阻取消固定通知提交人和实际执行取消的运行管理员；同人时只保留一次。
fn blocked_cancel_notification_recipients(submitted_by: &str, actor_id: &str) -> Vec<String> {
    if submitted_by == actor_id {
        return vec![submitted_by.to_string()];
    }
    vec![submitted_by.to_string(), actor_id.to_string()]
}

/// 视图中的首个新建开放任务摘要。
fn first_open_task(writes: &PlannedWrites, new_task_ids: &[String]) -> Option<OpenTaskSummary> {
    let intent = writes.create_tasks.first()?;
    let task_id = new_task_ids.first()?;
    let TaskIntent::HumanTaskRequested { assignee, .. } = intent else {
        return None;
    };
    Some(OpenTaskSummary {
        work_item_id: task_id.clone(),
        task_version: "1".to_string(),
        owner_user_id: assignee.as_str().to_string(),
    })
}

/// 读取当前有效且真正具备该单据类型运行管理权限的通知收件人。
async fn runtime_admin_notification_recipients(
    db: &Database,
    rbac: &SharedRbacService,
    document_type: DocumentType,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let spec = adapter_spec_of(document_type)?;
    let accounts = db
        .accounts()
        .list_by_kind(entities::AccountKind::Admin, executor)
        .await?;
    let mut recipients = Vec::new();
    for account in accounts {
        if !account.is_active_backoffice() {
            continue;
        }
        let actor = AuditActor::new(account.base.id.clone(), account.base.id.clone(), account.kind);
        let visibility = definition_management_visibility_with_executor(db, rbac, &actor, executor).await?;
        let read_scope =
            approval_document_read_scope_with_executor(db, rbac, &actor, document_type, executor).await?;
        let context = BindingRevalidationContext {
            organization_id: snapshot.payload.responsible_org_id.clone(),
            creator_id: snapshot.payload.submitted_by.clone(),
        };
        let read_scope_covers =
            !read_scope.is_empty() && read_scope.covers(&snapshot.payload.responsible_org_id);
        let object_readable = runtime_object_readable(&spec, &context, &account.base.id, read_scope_covers)?;
        if visibility.runtime_admin_types().contains(&document_type) && read_scope_covers && object_readable {
            recipients.push(account.base.id);
        }
    }
    recipients.sort();
    recipients.dedup();
    Ok(recipients)
}

/// 按 §16.5 消费决定计划中的每个通知意图并写入同一事务 outbox。
async fn persist_decision_notifications(
    db: &Database,
    writes: &PlannedWrites,
    facts: DecisionNotificationFacts<'_>,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    use entities::approval_integration::ApprovalNotificationEventKind as EventKind;

    if writes.notifications.is_empty() {
        return Err(Error::Internal("审批决定计划缺少通知意图".to_string()));
    }
    let mut seen = HashSet::with_capacity(writes.notifications.len());
    for intent in &writes.notifications {
        if !seen.insert(intent.dedup_key.as_str()) {
            return Err(Error::Internal("审批决定计划包含重复通知意图".to_string()));
        }
        let event_execution = match intent.event_kind {
            EventKind::Entered | EventKind::NodeApproved | EventKind::NodeRejected | EventKind::Blocked => {
                let execution_id = intent
                    .dedup_key
                    .split_once(':')
                    .map(|(_, id)| id)
                    .filter(|id| !id.is_empty())
                    .ok_or_else(|| Error::Internal("审批决定通知缺少执行引用".to_string()))?;
                writes
                    .created_executions
                    .iter()
                    .chain(writes.updated_executions.iter())
                    .find(|execution| execution.base.id == execution_id)
                    .or_else(|| {
                        (facts.ended_execution.base.id == execution_id).then_some(facts.ended_execution)
                    })
                    .ok_or_else(|| Error::Internal("审批决定通知执行引用不存在".to_string()))?
            }
            EventKind::Completed => facts.ended_execution,
            _ => return Err(Error::Internal("审批决定计划包含非决定通知事件".to_string())),
        };
        let expected_dedup = match intent.event_kind {
            EventKind::Entered => format!("entered:{}", event_execution.base.id),
            EventKind::NodeApproved => format!("approved:{}", event_execution.base.id),
            EventKind::NodeRejected => format!("rejected:{}", event_execution.base.id),
            EventKind::Blocked => format!("blocked:{}", event_execution.base.id),
            EventKind::Completed => format!("completed:{}", writes.instance.base.id),
            _ => unreachable!("unsupported decision event was rejected above"),
        };
        if intent.dedup_key != expected_dedup {
            return Err(Error::Internal("审批决定通知去重键不匹配".to_string()));
        }
        let recipients = match intent.event_kind {
            EventKind::Entered => vec![event_execution.assignee_participant_id.as_str().to_string()],
            EventKind::NodeApproved | EventKind::NodeRejected | EventKind::Completed => {
                vec![facts.submitted_by.to_string()]
            }
            EventKind::Blocked => notification_recipients(
                facts.submitted_by,
                facts.runtime_admin_ids.iter().map(String::as_str),
            ),
            _ => unreachable!("unsupported decision event was rejected above"),
        };
        let record = entities::approval_integration::ApprovalNotificationOutbox::enqueue(
            entities::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            recipients,
            entities::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: facts.document_type_label.to_string(),
                document_no: facts.document_no.to_string(),
                current_node_name: event_execution.node_name.clone(),
                current_approver_display_name: event_execution.assignee_name_snapshot.clone(),
                round_no: event_execution.round_no,
                reject_reason_summary: (intent.event_kind == EventKind::NodeRejected)
                    .then(|| facts.reject_reason.map(ToOwned::to_owned))
                    .flatten(),
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
    Ok(())
}

/// 以主收件人开头追加其它收件人并稳定去重。
fn notification_recipients<'a>(primary: &str, additional: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut recipients = vec![primary.to_string()];
    for recipient in additional {
        if !recipients.iter().any(|existing| existing == recipient) {
            recipients.push(recipient.to_string());
        }
    }
    recipients
}

/// 单事务应用决定写入：实例 CAS 推进、执行结束/插入、审批人绑定、任务
/// 完成/关闭/新建、通知 outbox 与审计。命令收据必须由调用方先写入以仲裁并发。
#[allow(clippy::too_many_arguments)]
async fn persist_decision_writes(
    db: &Database,
    writes: &PlannedWrites,
    ended_execution_id: &str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    expected_task_version: u64,
    work_item_id: &str,
    new_task_ids: &[String],
    list_projection: &ApprovalInstanceListProjection,
    audit: &entities::audit_log::AuditLog,
    now: Instant,
    actor_id: &str,
    owner_role: &str,
    owner_organization_id: &str,
    subject_version: &str,
    business_object_id: &str,
    document_type_label: &str,
    document_no: &str,
    submitted_by: &str,
    ended_execution: &ApprovalNodeExecution,
    reject_reason: Option<&str>,
    runtime_admin_ids: &[String],
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    // 实例 CAS 推进（RUNNING|BLOCKED + 当前执行不变式）。期望版本为加载时的
    // 持久化版本（引擎计划内的版本已在快照上自增），未应用视为并发冲突。
    let expected_current_execution_id = ApprovalNodeExecutionId::new(ended_execution_id);
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &writes.instance,
                expected_instance_version,
                &expected_current_execution_id,
                list_projection,
                session,
            )
            .await?,
        "审批实例",
    )?;
    // 当前执行结束（APPROVED/REJECTED/BLOCKED）。
    for execution in &writes.updated_executions {
        let expected = if execution.base.id.as_str() == ended_execution_id {
            expected_execution_version
        } else {
            execution
                .base
                .version
                .checked_sub(1)
                .ok_or_else(|| Error::Internal("决定后执行版本非法".to_string()))?
        };
        require_cas_applied(
            db.bpm_workflow()
                .end_active_execution(execution, expected, session)
                .await?,
            "审批执行",
        )?;
    }
    // 新执行与审批人绑定。
    for execution in &writes.created_executions {
        db.bpm_workflow().insert_execution(execution, session).await?;
    }
    if !writes.created_assignees.is_empty() {
        db.bpm_workflow()
            .insert_assignees(&writes.created_assignees, session)
            .await?;
    }
    // 任务：完成当前、按原因关闭、为下一节点新建。
    complete_or_close_tasks(
        db,
        CompleteOrCloseTasksInput {
            complete_tasks: &writes.complete_tasks,
            close_tasks: &writes.close_tasks,
            work_item_id,
            expected_task_version,
            ended_execution_id,
            actor_id,
            now,
        },
        session,
    )
    .await?;
    create_open_tasks(
        db,
        CreateOpenTasksInput {
            writes,
            new_task_ids,
            owner_role,
            owner_organization_id,
            subject_version,
            business_object_id,
            now,
        },
        session,
    )
    .await?;
    persist_decision_notifications(
        db,
        writes,
        DecisionNotificationFacts {
            document_type_label,
            document_no,
            submitted_by,
            ended_execution,
            reject_reason,
            runtime_admin_ids,
        },
        now,
        session,
    )
    .await?;
    db.audit_logs().create(audit, session).await?;
    Ok(())
}

/// 人员恢复时对旧关闭任务执行的只读并发守卫。
struct ClosedTaskGuard {
    task_id: String,
    execution_id: ApprovalNodeExecutionId,
    version: u64,
}

/// 人员恢复事务需要的冻结输入。
struct ResumePersistInput<'a> {
    writes: &'a PlannedWrites,
    ended_execution_id: &'a str,
    expected_instance_version: u64,
    expected_execution_version: u64,
    closed_task_guard: Option<&'a ClosedTaskGuard>,
    new_task_ids: &'a [String],
    list_projection: &'a ApprovalInstanceListProjection,
    audit: &'a entities::audit_log::AuditLog,
    now: Instant,
    owner_role: &'a str,
    owner_organization_id: &'a str,
    subject_version: &'a str,
    business_object_id: &'a str,
    document_type_label: &'a str,
    document_no: &'a str,
    submitted_by: &'a str,
}

/// 在一个 MongoDB 事务内应用人员恢复全部正式事实。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `input` - 恢复计划、CAS 版本、任务元数据与审计
/// * `session` - 唯一事务会话
///
/// # 返回
/// 实例、执行、收据、新任务、通知与审计全部写入时返回 `Ok(())`。
///
/// # 错误
/// 任一历史任务守卫、CAS、唯一索引或实体写入失败时返回错误并回滚。
///
/// # 关键业务约束
/// 旧关闭任务保持不可变；恢复只能结束旧受阻执行并为新执行创建新任务。
async fn persist_resume_writes(
    db: &Database,
    input: ResumePersistInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let [new_execution] = input.writes.created_executions.as_slice() else {
        return Err(Error::Internal(
            "原审批人恢复必须且只能创建一个新执行".to_string(),
        ));
    };
    if let Some(guard) = input.closed_task_guard {
        let task = db
            .work_items()
            .find_document_approval_by_id(&guard.task_id, session)
            .await?
            .ok_or_else(|| Error::ConflictError("原关闭审批任务不存在".to_string()))?;
        if task.status != WorkItemStatus::Closed
            || task.base.version != guard.version
            || task.approval_node_execution_id.as_ref() != Some(&guard.execution_id)
        {
            return Err(Error::ConflictError(
                "原关闭审批任务已变化，请刷新后重试".to_string(),
            ));
        }
    }
    // 收据是完成全部只读验证后的第一笔物理写，用唯一身份仲裁同键并发。
    db.bpm_workflow()
        .insert_command_receipt(&input.writes.receipt, session)
        .await
        .map_err(map_receipt_first_write_error)?;
    let expected_execution_id = ApprovalNodeExecutionId::new(input.ended_execution_id);
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &input.writes.instance,
                input.expected_instance_version,
                &expected_execution_id,
                input.list_projection,
                session,
            )
            .await?,
        "审批实例",
    )?;
    for execution in &input.writes.updated_executions {
        if execution.base.id != input.ended_execution_id {
            return Err(Error::Internal("恢复计划包含非当前旧执行更新".to_string()));
        }
        require_cas_applied(
            db.bpm_workflow()
                .end_blocked_execution(execution, input.expected_execution_version, session)
                .await?,
            "受阻审批执行",
        )?;
    }
    if !input.writes.created_assignees.is_empty() {
        return Err(Error::Internal("原审批人恢复不得修改实例审批人绑定".to_string()));
    }
    db.bpm_workflow().insert_execution(new_execution, session).await?;
    create_open_tasks(
        db,
        CreateOpenTasksInput {
            writes: input.writes,
            new_task_ids: input.new_task_ids,
            owner_role: input.owner_role,
            owner_organization_id: input.owner_organization_id,
            subject_version: input.subject_version,
            business_object_id: input.business_object_id,
            now: input.now,
        },
        session,
    )
    .await?;
    persist_resume_notifications(
        db,
        input.writes,
        new_execution,
        input.submitted_by,
        input.document_type_label,
        input.document_no,
        input.now,
        session,
    )
    .await?;
    db.audit_logs().create(input.audit, session).await?;
    Ok(())
}

/// 在恢复事务内按新执行事实追加进入节点与原审批人恢复通知。
#[allow(clippy::too_many_arguments)]
async fn persist_resume_notifications(
    db: &Database,
    writes: &PlannedWrites,
    new_execution: &ApprovalNodeExecution,
    submitted_by: &str,
    document_type_label: &str,
    document_no: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    use entities::approval_integration::ApprovalNotificationEventKind as EventKind;

    if writes.notifications.len() != 2 {
        return Err(Error::Internal(
            "原审批人恢复必须产生进入节点和恢复两条通知意图".to_string(),
        ));
    }
    let mut seen = HashSet::with_capacity(2);
    for intent in &writes.notifications {
        if !seen.insert(intent.event_kind) {
            return Err(Error::Internal("原审批人恢复包含重复通知意图".to_string()));
        }
        let expected_dedup = match intent.event_kind {
            EventKind::Entered => format!("entered:{}", new_execution.base.id),
            EventKind::Resumed => format!("resumed:{}", new_execution.base.id),
            _ => {
                return Err(Error::Internal(
                    "原审批人恢复包含非进入节点或恢复通知".to_string(),
                ));
            }
        };
        if intent.dedup_key != expected_dedup {
            return Err(Error::Internal("原审批人恢复通知去重键不匹配".to_string()));
        }
        let primary = new_execution.assignee_participant_id.as_str();
        let recipients = match intent.event_kind {
            EventKind::Entered => vec![primary.to_string()],
            EventKind::Resumed => notification_recipients(primary, [submitted_by]),
            _ => unreachable!("unsupported resume event was rejected above"),
        };
        let record = entities::approval_integration::ApprovalNotificationOutbox::enqueue(
            entities::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            recipients,
            entities::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: document_type_label.to_string(),
                document_no: document_no.to_string(),
                current_node_name: new_execution.node_name.clone(),
                current_approver_display_name: new_execution.assignee_name_snapshot.clone(),
                round_no: new_execution.round_no,
                reject_reason_summary: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
    if !seen.contains(&EventKind::Entered) || !seen.contains(&EventKind::Resumed) {
        return Err(Error::Internal(
            "原审批人恢复缺少进入节点或恢复通知意图".to_string(),
        ));
    }
    Ok(())
}

/// 完成或关闭审批任务所需的同一决定上下文。
struct CompleteOrCloseTasksInput<'a> {
    complete_tasks: &'a [ApprovalNodeExecutionId],
    close_tasks: &'a [(ApprovalNodeExecutionId, TaskCloseReason)],
    work_item_id: &'a str,
    expected_task_version: u64,
    ended_execution_id: &'a str,
    actor_id: &'a str,
    now: Instant,
}

/// 完成或关闭当前执行对应的开放任务，CAS 保持任务版本不变式。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `input` - 当前决定涉及的任务结束事实与并发版本
/// * `session` - 调用方事务会话
///
/// # 返回
/// 当前执行对应任务完成或关闭并通过 CAS 写回时返回 `Ok(())`。
///
/// # 错误
/// 任务缺失、实体状态变更或 Repository CAS 失败时返回错误。
///
/// # 关键业务约束
/// 任务读取固定走单据审批语义 Repository，生命周期变更由 WorkItem 实体方法执行。
async fn complete_or_close_tasks(
    db: &Database,
    input: CompleteOrCloseTasksInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let execution_id = ApprovalNodeExecutionId::new(input.ended_execution_id);
    let Some(ending) = approval_task_ending(&input, &execution_id)? else {
        return Ok(());
    };
    let tasks = db
        .work_items()
        .open_approval_tasks_for_execution(&execution_id, session)
        .await?;
    let requested = tasks
        .iter()
        .find(|item| item.base.id == input.work_item_id)
        .ok_or_else(hidden_not_found)?;
    ensure_expected_version("审批任务", input.expected_task_version, requested.base.version)?;
    let tasks =
        WorkItem::end_all_for_approval_execution(tasks, &execution_id, input.actor_id, &ending, input.now)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
    db.work_items()
        .persist_ended_approval_tasks(&tasks, session)
        .await?;
    Ok(())
}

/// 解析当前结束执行唯一的任务终结方式。
///
/// # 错误
/// 同一执行同时被计划为完成和关闭，或出现不同关闭原因时返回冲突。
fn approval_task_ending(
    input: &CompleteOrCloseTasksInput<'_>,
    execution_id: &ApprovalNodeExecutionId,
) -> Result<Option<ApprovalRuntimeTaskEnding>> {
    let completes = input.complete_tasks.iter().any(|item| item == execution_id);
    let close_reasons = input
        .close_tasks
        .iter()
        .filter(|(item, _)| item == execution_id)
        .map(|(_, reason)| reason.as_str())
        .collect::<Vec<_>>();
    if completes && !close_reasons.is_empty() {
        return Err(Error::ConflictError("同一审批执行同时计划完成和关闭".to_string()));
    }
    let Some(first_reason) = close_reasons.first() else {
        return Ok(completes.then_some(ApprovalRuntimeTaskEnding::Complete));
    };
    if close_reasons.iter().any(|reason| reason != first_reason) {
        return Err(Error::ConflictError(
            "同一审批执行存在不同任务关闭原因".to_string(),
        ));
    }
    Ok(Some(ApprovalRuntimeTaskEnding::Close {
        reason: (*first_reason).to_string(),
    }))
}

/// 创建开放审批任务所需的决定输出与单据责任上下文。
struct CreateOpenTasksInput<'a> {
    writes: &'a PlannedWrites,
    new_task_ids: &'a [String],
    owner_role: &'a str,
    owner_organization_id: &'a str,
    subject_version: &'a str,
    business_object_id: &'a str,
    now: Instant,
}

/// 为 `HumanTaskRequested` 意图创建新开放任务。
///
/// # 错误
/// 任务实体构造失败或 Repository 写入失败时返回错误。
async fn create_open_tasks(
    db: &Database,
    input: CreateOpenTasksInput<'_>,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for (index, intent) in input.writes.create_tasks.iter().enumerate() {
        let TaskIntent::HumanTaskRequested {
            execution_id,
            assignee,
            ..
        } = intent
        else {
            continue;
        };
        let item = WorkItem::new_document_approval(
            WorkItemId::new(input.new_task_ids.get(index).cloned().unwrap_or_else(next_id)),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: execution_id.clone(),
                business_object_type: input.writes.instance.subject.subject_kind().to_string(),
                business_object_id: input.business_object_id.to_string(),
                subject_version: input.subject_version.to_string(),
                owner_role: input.owner_role.to_string(),
                owner_organization_id: input.owner_organization_id.to_string(),
                owner_user_id: assignee.as_str().to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            input.now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.work_items().create(&item, session).await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};
    use serde_json::json;

    use bpm::engine::TaskCloseReason;
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::{
        ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource,
        ApprovalProcessInstanceStatus,
    };
    use bpm::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, NewProcessInstance, ParticipantId,
        Timestamp,
    };
    use bpm::{ProcessKind, SubjectRef};
    use database::repository::approval_integration::{ApprovalRuntimeReadRow, ApprovalRuntimeReadTypeScope};
    use database::repository::bpm::{
        ApprovalInstanceListView, ApprovalInstanceSummary, APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX,
    };
    use entities::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
    use entities::common::time::Instant;
    use entities::document_registry::DocumentType;
    use entities::ids::{ApprovalSubjectSnapshotId, WorkItemId};
    use entities::money::Quantity;
    use entities::AccountKind;

    use entities::work_item::{
        ApprovalRuntimeTaskEnding, AssignmentSource, DocumentApprovalWorkItemData, WorkItem,
        WorkItemPriority, WorkItemStatus,
    };

    use crate::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
    use crate::approval::execution::idempotency::{
        command_may_have_committed, command_recovery_delay, map_receipt_first_write_error,
    };
    use crate::approval::ApprovalCancelBlockedCommand;
    use crate::audit::AuditActor;
    use crate::errors::{Error, ErrorCode};

    use super::{
        approval_task_ending, blocked_cancel_notification_recipients, cancel_blocked_terminal_facts_match,
        cursor_from_summary, decision_receipt_lookup_gate, decision_terminal_actor,
        decision_terminal_fresh_error, ensure_cancel_blocked_instance_preconditions,
        ensure_mine_page_integrity, item_from_runtime_read_row, item_from_summary,
        legacy_decision_terminal_facts_match, management_runtime_read_allowed, map_approval_task_error,
        mine_execution_ids, mine_instance_ids, mine_runtime_chain_matches, notification_recipients,
        ordinary_runtime_read_allowed, runtime_object_readable, started_runtime_read_allowed,
        task_proves_current_responsibility, unique_by_id, CancelBlockedTerminalFacts,
        CompleteOrCloseTasksInput, DecisionReceiptLookup, RuntimeDecisionCommand, RuntimeInstanceListView,
        RuntimeReadAuthorizationFacts, RuntimeReadSubject,
    };

    fn summary() -> ApprovalInstanceSummary {
        ApprovalInstanceSummary {
            id: "inst-1".to_string(),
            process_kind: ProcessKind::StockAdjustment,
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            definition_version: 2,
            subject: SubjectRef::new("stock_adjustment", "adj-1").expect("主体"),
            subject_version: 1,
            status: ApprovalProcessInstanceStatus::Running,
            current_round_no: 1,
            current_node_execution_id: Some(ApprovalNodeExecutionId::new("exec-1")),
            current_node_key: Some("review".to_string()),
            current_node_name: Some("仓储复核".to_string()),
            current_assignee_participant_id: Some("warehouse-1".to_string()),
            current_assignee_name: Some("仓库1".to_string()),
            latest_rejected_execution_id: None,
            latest_rejection_summary: None,
            last_status_changed_at: Some(20),
            started_by: "starter".to_string(),
            started_at: 10,
            blocked_at: None,
            version: 1,
            updated_at: 20,
        }
    }

    fn snapshot() -> ApprovalSubjectSnapshot {
        ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snapshot-1"),
            ApprovalProcessInstanceId::new("inst-1"),
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            ApprovalSubjectSnapshotPayload {
                document_no: "ADJ-0001".to_string(),
                responsible_org_id: "org-1".to_string(),
                submitted_by: "starter".to_string(),
                submitted_at: Instant::from_unix_secs(10),
                counterparty: None,
                total_amount: None,
                total_quantity: Some(Quantity::from_str("1").expect("数量")),
                line_count: 1,
            },
        )
        .expect("快照")
    }

    fn active_execution(execution_id: &str, instance_id: &str) -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new(execution_id),
            process_instance_id: ApprovalProcessInstanceId::new(instance_id),
            node_key: "review".to_string(),
            node_name: "仓储复核".to_string(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("warehouse-1").expect("审批人"),
            assignee_name_snapshot: "仓库1".to_string(),
            at: Timestamp::from_unix_secs(10).expect("时间"),
        })
        .expect("执行")
    }

    fn approval_task(task_id: &str, execution_id: &str) -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new(task_id),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new(execution_id),
                business_object_type: "stock_adjustment".to_string(),
                business_object_id: "adj-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "warehouse-1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .expect("任务")
    }

    fn assert_hidden_not_found(error: Error) {
        assert!(matches!(
            error,
            Error::NotFound(message) if message == "审批实例不存在"
        ));
    }

    fn assert_same_error_semantics(actual: Error, expected: Error) {
        assert_eq!(std::mem::discriminant(&actual), std::mem::discriminant(&expected));
        assert_eq!(actual.code(), expected.code());
        assert_eq!(actual.to_string(), expected.to_string());
    }

    fn runtime_responsibility_fixture() -> (RuntimeReadSubject, WorkItem) {
        let at = Timestamp::from_unix_secs(10).expect("时间");
        let instance_id = ApprovalProcessInstanceId::new("inst-1");
        let execution_id = ApprovalNodeExecutionId::new("exec-1");
        let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: instance_id.clone(),
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            definition_version: 2,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").expect("主体"),
            subject_version: 1,
            started_by: ParticipantId::new("starter").expect("启动人"),
            at,
        })
        .expect("实例");
        let execution = active_execution("exec-1", "inst-1");
        instance
            .set_current_execution(execution_id.clone(), at)
            .expect("当前执行");
        let task = approval_task("wi-1", execution_id.as_ref());
        (
            RuntimeReadSubject {
                instance,
                current_execution: Some(execution),
                snapshot: snapshot(),
                document_type: DocumentType::StockAdjustment,
            },
            task,
        )
    }

    #[test]
    fn started_item_uses_snapshot_document_number_and_runtime_projection() {
        let snapshot = snapshot();
        let item = item_from_summary(summary(), Some(&snapshot)).expect("列表行");

        assert_eq!(item.document_type.as_deref(), Some("stock_adjustment"));
        assert_eq!(item.document_id.as_deref(), Some("adj-1"));
        assert_eq!(item.document_label.as_deref(), Some("ADJ-0001"));
        assert_eq!(item.current_assignee_name.as_deref(), Some("仓库1"));
        assert_eq!(item.process_version, Some(2));
        assert_eq!(item.started_at, Some(10));
    }

    #[test]
    fn started_cursor_uses_started_time_and_stable_instance_id() {
        let cursor = cursor_from_summary(ApprovalInstanceListView::Started, &summary());
        assert_eq!(cursor.sort_time, 10);
        assert_eq!(cursor.id, "inst-1");
    }

    #[test]
    fn ordinary_read_matrix_supports_only_signed_sources() {
        let denied = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: false,
            current_responsibility: false,
            object_readable: false,
            scope_covers: false,
            runtime_admin: false,
        };
        assert!(!ordinary_runtime_read_allowed(denied));
        assert!(ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            initiator: true,
            ..denied
        }));
        assert!(ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            current_responsibility: true,
            ..denied
        }));
        assert!(ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            object_readable: true,
            scope_covers: true,
            ..denied
        }));
        assert!(!ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            actor_active: false,
            initiator: true,
            current_responsibility: true,
            object_readable: true,
            scope_covers: true,
            ..denied
        }));
    }

    #[test]
    fn current_responsibility_requires_exact_open_runtime_task_chain() {
        let (subject, task) = runtime_responsibility_fixture();
        let execution = subject.current_execution.as_ref().expect("当前执行");
        assert!(task_proves_current_responsibility(
            &task,
            execution,
            &subject,
            "warehouse-1",
            "stock_adjustment_approver",
        ));

        let mut wrong_source = task.clone();
        wrong_source.assignment_source = AssignmentSource::SystemRule;
        let mut wrong_role = task.clone();
        wrong_role.owner_role = "other-role".to_string();
        let mut wrong_org = task.clone();
        wrong_org.owner_organization_id = "org-2".to_string();
        let mut wrong_version = task.clone();
        wrong_version.subject_version = "01".to_string();
        let mut closed = task.clone();
        closed.status = WorkItemStatus::Closed;
        for candidate in [wrong_source, wrong_role, wrong_org, wrong_version, closed] {
            assert!(!task_proves_current_responsibility(
                &candidate,
                execution,
                &subject,
                "warehouse-1",
                "stock_adjustment_approver",
            ));
        }

        let mut ended_subject = subject;
        ended_subject.instance.status = ApprovalProcessInstanceStatus::Approved;
        assert!(!task_proves_current_responsibility(
            &task,
            ended_subject.current_execution.as_ref().expect("当前执行"),
            &ended_subject,
            "warehouse-1",
            "stock_adjustment_approver",
        ));
    }

    #[test]
    fn mine_chain_uses_runtime_identity_and_treats_snapshot_as_optional_label() {
        let (subject, task) = runtime_responsibility_fixture();
        let execution = subject.current_execution.as_ref().expect("当前执行");
        let row = summary();
        assert!(
            mine_runtime_chain_matches(&task, execution, &row, Some(&subject.snapshot), "warehouse-1",)
                .expect("责任链")
        );

        let mut drifted = subject.snapshot.clone();
        drifted.subject_version = 2;
        assert!(
            mine_runtime_chain_matches(&task, execution, &row, Some(&drifted), "warehouse-1",)
                .expect("漂移快照不撤销 WorkItem 责任")
        );

        let mut wrong_projection = row;
        wrong_projection.current_node_name = Some("错误节点".to_string());
        assert!(
            !mine_runtime_chain_matches(&task, execution, &wrong_projection, None, "warehouse-1",)
                .expect("实例投影漂移")
        );
    }

    #[test]
    fn mine_page_rejects_two_tasks_for_the_same_execution() {
        let tasks = [approval_task("wi-1", "exec-1"), approval_task("wi-2", "exec-1")];

        let error = mine_execution_ids(&tasks).expect_err("重复 execution 必须整页失败关闭");

        assert_hidden_not_found(error);
    }

    #[test]
    fn mine_page_hides_repository_wide_integrity_conflicts() {
        ensure_mine_page_integrity(0).expect("无完整性冲突");
        let error = ensure_mine_page_integrity(1).expect_err("跨页冲突必须隐藏式失败关闭");
        assert_hidden_not_found(error);
    }

    #[test]
    fn mine_page_rejects_two_executions_for_the_same_instance() {
        let execution_ids = vec![
            ApprovalNodeExecutionId::new("exec-1"),
            ApprovalNodeExecutionId::new("exec-2"),
        ];
        let execution_by_id = unique_by_id(
            vec![
                active_execution("exec-1", "inst-1"),
                active_execution("exec-2", "inst-1"),
            ],
            |execution| execution.base.id.clone(),
        )
        .expect("执行主键唯一");

        let error = mine_instance_ids(&execution_ids, &execution_by_id)
            .expect_err("不同 execution 指向同一 instance 必须整页失败关闭");

        assert_hidden_not_found(error);
    }

    #[test]
    fn mine_page_preserves_distinct_task_execution_instance_chains() {
        let tasks = [approval_task("wi-1", "exec-1"), approval_task("wi-2", "exec-2")];
        let execution_ids = mine_execution_ids(&tasks).expect("不同执行");
        let execution_by_id = unique_by_id(
            vec![
                active_execution("exec-1", "inst-1"),
                active_execution("exec-2", "inst-2"),
            ],
            |execution| execution.base.id.clone(),
        )
        .expect("执行主键唯一");

        let instance_ids = mine_instance_ids(&execution_ids, &execution_by_id).expect("不同实例");

        assert_eq!(
            instance_ids.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            vec!["inst-1", "inst-2"]
        );
    }

    #[test]
    fn management_requires_every_gate_while_started_uses_initiator_fact() {
        let allowed = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: true,
            current_responsibility: false,
            object_readable: true,
            scope_covers: true,
            runtime_admin: true,
        };
        assert!(management_runtime_read_allowed(allowed));
        assert!(started_runtime_read_allowed(allowed));
        for denied in [
            RuntimeReadAuthorizationFacts {
                actor_active: false,
                ..allowed
            },
            RuntimeReadAuthorizationFacts {
                object_readable: false,
                ..allowed
            },
            RuntimeReadAuthorizationFacts {
                scope_covers: false,
                ..allowed
            },
        ] {
            assert!(!management_runtime_read_allowed(denied));
        }
        assert!(!started_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            actor_active: false,
            ..allowed
        }));
        assert!(started_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            object_readable: false,
            scope_covers: false,
            runtime_admin: false,
            ..allowed
        }));
        assert!(!management_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            runtime_admin: false,
            ..allowed
        }));
        assert!(!started_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            initiator: false,
            ..allowed
        }));
    }

    #[test]
    fn scoped_row_revalidates_snapshot_and_view_authorization() {
        let actor = AuditActor::new("starter".to_string(), "starter".to_string(), AccountKind::Admin);
        let type_scopes = [ApprovalRuntimeReadTypeScope {
            process_kind: ProcessKind::StockAdjustment,
            organization_ids: None,
        }];
        let item = item_from_runtime_read_row(
            ApprovalRuntimeReadRow {
                instance: summary(),
                snapshot: Some(snapshot()),
            },
            &actor,
            RuntimeInstanceListView::Started,
            &type_scopes,
        )
        .expect("Started 发起人事实成立");
        assert_eq!(item.instance_id, "inst-1");

        assert!(item_from_runtime_read_row(
            ApprovalRuntimeReadRow {
                instance: summary(),
                snapshot: Some(snapshot()),
            },
            &actor,
            RuntimeInstanceListView::Managed,
            &[],
        )
        .is_err());

        let mut drifted = snapshot();
        drifted.subject_version = 2;
        assert_eq!(
            item_from_summary(summary(), Some(&drifted))
                .expect("漂移快照仅清空标签")
                .document_label,
            None
        );
        assert_eq!(
            item_from_summary(summary(), None)
                .expect("缺失快照保留运行实例")
                .document_label,
            None
        );
    }

    #[test]
    fn approval_task_ending_rejects_conflicting_plan() {
        let execution_id = ApprovalNodeExecutionId::new("exec-1");
        let complete_tasks = vec![execution_id.clone()];
        let close_tasks = Vec::new();
        let input = CompleteOrCloseTasksInput {
            complete_tasks: &complete_tasks,
            close_tasks: &close_tasks,
            work_item_id: "wi-1",
            expected_task_version: 1,
            ended_execution_id: "exec-1",
            actor_id: "u1",
            now: Instant::from_unix_secs(20),
        };
        assert_eq!(
            approval_task_ending(&input, &execution_id).unwrap(),
            Some(ApprovalRuntimeTaskEnding::Complete)
        );

        let close_tasks = vec![(execution_id.clone(), TaskCloseReason::ApprovalRuntimeBlocked)];
        let conflicting = CompleteOrCloseTasksInput {
            close_tasks: &close_tasks,
            ..input
        };
        assert!(approval_task_ending(&conflicting, &execution_id).is_err());
    }

    #[test]
    fn decision_and_blocked_cancel_notification_recipients_are_contract_exact() {
        assert_eq!(
            notification_recipients("submitter", ["runtime-admin", "submitter", "runtime-admin"]),
            vec!["submitter".to_string(), "runtime-admin".to_string()]
        );
        assert_eq!(
            blocked_cancel_notification_recipients("submitter", "executing-admin"),
            vec!["submitter".to_string(), "executing-admin".to_string()]
        );
        assert_eq!(
            blocked_cancel_notification_recipients("same-user", "same-user"),
            vec!["same-user".to_string()]
        );
    }

    fn duplicate_key_error(index_name: Option<&str>) -> database::Error {
        let message = index_name.map_or_else(
            || "E11000 duplicate key error".to_string(),
            |index| format!("E11000 duplicate key error collection: erp.receipts index: {index} dup key"),
        );
        let write_error: WriteError = serde_json::from_value(json!({
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": message,
            "errInfo": null,
        }))
        .expect("duplicate key fixture");
        let mongo_error: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        database::Error::from(mongo_error)
    }

    fn runtime_source_fn(start: &str, end: &str) -> &'static str {
        let source = include_str!("runtime_service.rs");
        let start = source.find(start).expect("运行时函数起点必须存在");
        let end = source[start..]
            .find(end)
            .map(|offset| start + offset)
            .expect("运行时函数终点必须存在");
        &source[start..end]
    }

    #[test]
    fn decision_recovery_only_polls_receipt_competition_and_uncertain_commits() {
        let duplicate = map_receipt_first_write_error(duplicate_key_error(Some(
            APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX,
        )));
        assert!(command_may_have_committed(&duplicate));
        assert!(matches!(duplicate, Error::ReceiptDuplicate(_)));

        for unrelated in [Some("_id_"), Some("uk_approval_command_receipts_id"), None] {
            let error = map_receipt_first_write_error(duplicate_key_error(unrelated));
            assert!(!command_may_have_committed(&error));
            assert!(matches!(error, Error::ConflictError(_)));
        }
        let transient = Error::from(database::Error::TransientTransactionConflict(
            mongodb::error::Error::custom("write conflict"),
        ));
        assert!(command_may_have_committed(&transient));
        assert!(matches!(transient, Error::TransientTransaction(_)));
        assert!(command_may_have_committed(&Error::OutcomeUnknown(
            database::Error::CommitOutcomeUnknown(mongodb::error::Error::custom("unknown commit")),
        )));
        assert!(!command_may_have_committed(&Error::ConflictError(
            "数据已存在，请勿重复提交".to_string()
        )));
        assert!(!command_may_have_committed(&Error::ConflictError(
            "并发事务冲突，请重试".to_string()
        )));
        assert!(!command_may_have_committed(&Error::ConflictError(
            "审批任务已结束".to_string()
        )));
        assert!(!command_may_have_committed(&Error::ValidationError(
            "请求无效".to_string()
        )));
        assert_eq!(command_recovery_delay(0).as_millis(), 5);
        assert_eq!(command_recovery_delay(5).as_millis(), 160);
        assert_eq!(command_recovery_delay(99).as_millis(), 160);
    }

    #[test]
    fn resume_persistence_keeps_receipt_as_first_physical_write() {
        let source = runtime_source_fn(
            "async fn persist_resume_writes(",
            "async fn persist_resume_notifications(",
        );
        let receipt = source.find("insert_command_receipt").expect("恢复必须写命令收据");
        assert!(source[..receipt].contains("find_document_approval_by_id"));
        assert!(!source[..receipt].contains("advance_instance"));
        assert!(!source[..receipt].contains("end_blocked_execution"));
        assert!(!source[..receipt].contains("insert_execution"));
        assert!(!source[..receipt].contains("create_open_tasks"));
        assert!(!source[..receipt].contains("persist_resume_notifications"));
        assert!(!source[..receipt].contains("audit_logs().create"));
        assert!(source[receipt..].contains("map_err(map_receipt_first_write_error)"));
        for later_write in [
            "advance_instance",
            "end_blocked_execution",
            "insert_execution",
            "create_open_tasks",
            "persist_resume_notifications",
            "audit_logs().create",
        ] {
            assert!(
                receipt < source.find(later_write).expect("恢复后续写入必须存在"),
                "receipt 必须先于 {later_write}",
            );
        }
    }

    #[test]
    fn resume_uncertain_result_recovery_always_opens_a_fresh_transaction() {
        let endpoint = runtime_source_fn("pub async fn resume_current_approver(", "async fn replay_resume(");
        assert!(endpoint.contains("command_may_have_committed"));
        assert!(endpoint.contains("recover_resume_after_competing_commit"));

        let replay = runtime_source_fn(
            "async fn replay_resume(",
            "async fn recover_resume_after_competing_commit(",
        );
        assert!(replay.contains("with_transaction"));
        assert!(replay.contains("replay_resume_in_transaction"));

        let recovery = runtime_source_fn(
            "async fn recover_resume_after_competing_commit(",
            "pub async fn cancel_blocked(",
        );
        assert!(recovery.contains("const RECOVERY_ATTEMPTS"));
        assert!(recovery.contains("self.replay_resume"));
        assert!(recovery.contains("command_recovery_delay"));
        assert!(!recovery.contains("ClientSession"));
    }

    fn decided_fixture(
        reason: Option<&str>,
        expected_task_version: u64,
    ) -> (WorkItem, ApprovalNodeExecution) {
        let mut execution = active_execution("exec-legacy", "inst-legacy");
        execution
            .record_approve(
                ParticipantId::new("warehouse-1").expect("决定人"),
                reason.map(ToOwned::to_owned),
                Timestamp::from_unix_secs(20).expect("决定时间"),
            )
            .expect("记录终态决定");
        let mut item = approval_task("wi-legacy", "exec-legacy");
        item.complete_by_approval_runtime("warehouse-1", Instant::from_unix_secs(20))
            .expect("完成审批任务");
        item.base.version = expected_task_version + 1;
        (item, execution)
    }

    fn runtime_decision_command(
        reason: Option<&str>,
        expected_task_version: u64,
        _actor_id: &str,
    ) -> RuntimeDecisionCommand {
        RuntimeDecisionCommand {
            work_item_id: "wi-legacy".to_string(),
            decision: ApprovalDecision::Approve,
            reason: reason.map(ToOwned::to_owned),
            expected_task_version,
            idempotency_key: crate::approval::execution::idempotency::normalize_idempotency_key("legacy-key")
                .expect("幂等键"),
        }
    }

    #[test]
    fn decision_existing_and_missing_keys_hide_from_outsider_and_revoked_actor() {
        let (item, execution) = decided_fixture(None, 3);
        assert_eq!(decision_terminal_actor(&item, &execution), Some("warehouse-1"));

        let missing_outsider = map_approval_task_error(
            item.approval_execution_for_decision("outsider", 3)
                .expect_err("非原责任人 Fresh 路径必须拒绝"),
        );
        let existing_outsider =
            decision_receipt_lookup_gate(&item, "outsider", 3).expect_err("非原责任人不得进入收据查询");
        assert_same_error_semantics(existing_outsider, missing_outsider);

        let missing_original = map_approval_task_error(
            item.approval_execution_for_decision("warehouse-1", 3)
                .expect_err("已完成任务 Fresh 路径必须稳定返回 NotOpen"),
        );
        assert!(matches!(
            decision_receipt_lookup_gate(&item, "warehouse-1", 3).expect("原责任人可继续证明回放"),
            DecisionReceiptLookup::Terminal(execution_id) if execution_id.as_ref() == "exec-legacy"
        ));
        let existing_revoked = decision_terminal_fresh_error();
        assert_same_error_semantics(existing_revoked, missing_original);
        assert_eq!(
            decision_terminal_fresh_error().code(),
            Some(ErrorCode::ApprovalTaskNotOpen)
        );
    }

    #[test]
    fn cancel_existing_and_missing_keys_share_fresh_terminal_errors_before_digest() {
        let (mut subject, _) = runtime_responsibility_fixture();
        let expected_instance_version = subject.instance.base.version;
        subject
            .instance
            .cancel(Timestamp::from_unix_secs(20).expect("取消时间"))
            .expect("构造已取消终态");
        let command = ApprovalCancelBlockedCommand {
            approval_process_instance_id: subject.instance.base.id.clone(),
            expected_instance_version,
            expected_execution_version: 7,
            expected_task_version: None,
            reason: "结构受损退出".to_string(),
            idempotency_key: "cancel-key".to_string(),
            actor_id: "runtime-admin".to_string(),
        };
        let missing_stale = ensure_cancel_blocked_instance_preconditions(&subject.instance, &command)
            .expect_err("Fresh 路径先返回实例版本冲突");
        let existing_non_actor = ensure_cancel_blocked_instance_preconditions(&subject.instance, &command)
            .expect_err("existing key 非原 actor 必须复用同一 Fresh 冲突");
        assert_same_error_semantics(existing_non_actor, missing_stale);

        let mut current_version = command.clone();
        current_version.expected_instance_version = subject.instance.base.version;
        let missing_status =
            ensure_cancel_blocked_instance_preconditions(&subject.instance, &current_version)
                .expect_err("伪造当前版本仍必须按 Fresh 状态失败");
        let existing_status =
            ensure_cancel_blocked_instance_preconditions(&subject.instance, &current_version)
                .expect_err("existing key 不得改为摘要冲突");
        assert_same_error_semantics(existing_status, missing_status);

        let facts = CancelBlockedTerminalFacts {
            blocker: ApprovalBlockerCode::DefinitionGraphCorrupted,
            actor_id: "runtime-admin".to_string(),
            reason: command.reason.clone(),
            execution_version: command.expected_execution_version + 1,
            task_versions: Vec::new(),
        };
        assert!(cancel_blocked_terminal_facts_match(
            &subject.instance,
            &facts,
            &command,
            "runtime-admin"
        ));
        assert!(!cancel_blocked_terminal_facts_match(
            &subject.instance,
            &facts,
            &command,
            "other-admin"
        ));
    }

    #[test]
    fn legacy_decision_requires_exact_terminal_facts() {
        let (none_item, none_execution) = decided_fixture(None, 3);
        let exact_none = runtime_decision_command(None, 3, "warehouse-1");
        let literal_null = runtime_decision_command(Some("NULL"), 3, "warehouse-1");
        assert!(legacy_decision_terminal_facts_match(
            &none_item,
            &none_execution,
            &exact_none,
            "warehouse-1"
        ));
        assert!(!legacy_decision_terminal_facts_match(
            &none_item,
            &none_execution,
            &literal_null,
            "warehouse-1"
        ));

        let (separator_item, separator_execution) = decided_fixture(Some("x\u{1f}3"), 4);
        let separator_exact = runtime_decision_command(Some("x\u{1f}3"), 4, "warehouse-1");
        let separator_relocated = runtime_decision_command(Some("x"), 3, "4\u{1f}warehouse-1");
        assert!(legacy_decision_terminal_facts_match(
            &separator_item,
            &separator_execution,
            &separator_exact,
            "warehouse-1"
        ));
        assert!(!legacy_decision_terminal_facts_match(
            &separator_item,
            &separator_execution,
            &separator_relocated,
            "4\u{1f}warehouse-1"
        ));
    }

    #[test]
    fn stock_adjustment_runtime_object_read_uses_registered_permission_scope() {
        let spec = adapter_spec_of(DocumentType::StockAdjustment).expect("库存调整适配器");
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "submitter".to_string(),
        };
        assert!(runtime_object_readable(&spec, &context, "approver", true).expect("已登记读权且范围覆盖"));
        assert!(!runtime_object_readable(&spec, &context, "approver", false).expect("范围不覆盖必须拒绝"));
    }
}
