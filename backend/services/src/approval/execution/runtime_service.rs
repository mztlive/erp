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
    ApprovalBlockerCode, ApprovalCommandKind, ApprovalDecision, ApprovalNodeExecutionStatus,
    ApprovalProcessInstanceStatus, ModelError,
};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp};
use database::repository::approval_integration::{
    ApprovalRuntimeReadRepository, ApprovalRuntimeReadRow, ApprovalRuntimeReadScope,
    ApprovalRuntimeReadTypeScope,
};
use database::repository::bpm::{
    ApprovalInstanceListFilter, ApprovalInstanceListProjection, ApprovalInstanceListView,
    ApprovalInstanceSummary, ApprovalInstanceTextQuery,
};
use database::{AccessControlExt, ApprovalIntegrationExt, BpmExt, NoTransaction, Transactional, WorkItemExt};
use entities::approval_integration::ApprovalSubjectSnapshot;
use entities::common::time::Instant;
use entities::document_registry::DocumentType;
use entities::ids::WorkItemId;
use entities::work_item::{
    ApprovalDecisionTaskError, ApprovalRuntimeTaskEnding, AssignmentSource, DocumentApprovalWorkItemData,
    WorkItem, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use id_generator::next_id;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::apply_plan::PlannedWrites;
use super::authorization::{converge_eligibility, AuthorizationFailure};
use super::decision::prepare_decision;
use super::idempotency::{
    cancel_blocked_digest, normalize_idempotency_key, payload_conflict_error, resume_digest,
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
use crate::approval::binding::upgrade_unsubmitted_document_definition;
use crate::approval::business_adapter::{
    adapter_object_read_decision, adapter_spec_of, document_type_from_subject_kind,
    BindingRevalidationContext,
};
use crate::approval::policy::{
    policy_of, DocumentApprovalPolicy, ALL_DOCUMENT_TYPES, STATIC_APPROVE_PERMISSION,
};
use crate::approval::process_kind::process_kind_of;
use crate::approval::scope::{
    approval_actor_is_active, approval_document_read_scope, definition_management_visibility,
};
use crate::approval::{
    approval_recovery_scope, ApprovalActionContext, ApprovalCancelBlockedCommand, ApprovalDomainActionPort,
    ApprovalResumeCommand, FailClosedApprovalActionPort,
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

        // ---------- 任务：存在、责任、开放与版本 ----------
        let item = self
            .db
            .work_items()
            .find_document_approval_by_id(work_item_id, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let execution_id = item
            .approval_execution_for_decision(actor.id(), expected_task_version)
            .map_err(map_approval_task_error)?
            .as_ref()
            .to_string();

        // ---------- 执行、实例与定义图 ----------
        let execution = self
            .db
            .bpm_workflow()
            .find_execution_by_id(&ApprovalNodeExecutionId::new(&execution_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        // CAS 期望版本 = 加载时的持久化版本（引擎计划会在快照上自增版本）。
        let expected_execution_version = execution.base.version;
        let instance_id = execution.process_instance_id.as_ref().to_string();
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let expected_instance_version = instance.base.version;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&instance.process_definition_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
        let document_type = document_type_from_subject_kind(instance.subject.subject_kind())?;
        let spec = adapter_spec_of(document_type)?;
        let organization_id = item.owner_organization_id.clone();

        // ---------- 写时资格：当前决定人与下一节点审批人 ----------
        let current_eligibility = self
            .revalidate_approver(
                actor.id(),
                &execution.assignee_name_snapshot,
                &organization_id,
                &spec,
            )
            .await?;
        let target_node_key = graph
            .decision_target_node_key(&execution.node_key, decision)
            .map_err(map_runtime_graph_error)?;
        let next_eligibility = match &target_node_key {
            Some(node_key) => {
                let node = graph
                    .node(node_key)
                    .ok_or_else(|| Error::ConflictError("审批定义缺少目标节点".to_string()))?;
                self.revalidate_approver(
                    node.assignee_participant_id.as_str(),
                    &node.assignee_label_snapshot,
                    &organization_id,
                    &spec,
                )
                .await?
            }
            None => current_eligibility.clone(),
        };

        // ---------- 幂等与开放任务数 ----------
        let key = normalize_idempotency_key(idempotency_key)?;
        let receipt = self
            .db
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::SubmitDecision,
                &execution_id,
                &key,
                &mut NoTransaction,
            )
            .await?;
        let open_task_count = self
            .db
            .work_items()
            .count_open_document_approval_by_execution(
                &ApprovalNodeExecutionId::new(&execution_id),
                &mut NoTransaction,
            )
            .await?;
        let open_task_count = usize::try_from(open_task_count)
            .map_err(|_| Error::Internal("开放审批任务数量溢出".to_string()))?;

        // ---------- 规划决定 ----------
        let now = Instant::now();
        let instance_assignee_id = self
            .instance_assignee_id(&instance_id, &execution.node_key)
            .await?;
        let prepared = prepare_decision(DecisionExecutionInput {
            command: ExecutionCommandInput {
                graph,
                current_eligibility,
                next_eligibility,
                receipt,
                idempotency_key: key.clone(),
                now: Timestamp::from_utc(now.as_utc()),
            },
            instance,
            current: execution.clone(),
            work_item_id: work_item_id.to_string(),
            task_owner_id: item.owner_user_id.clone().unwrap_or_default(),
            instance_assignee_id,
            decision,
            reason: reason.clone(),
            expected_task_version,
            actor: ParticipantId::new(actor.id())
                .map_err(|_| Error::ValidationError("决定人引用无效".to_string()))?,
            next_execution_id: ApprovalNodeExecutionId::new(next_id()),
            next_execution_no: execution.execution_no + 1,
            receipt_id: ApprovalCommandReceiptId::new(next_id()),
            open_task_count,
        })?;

        match prepared {
            PreparedExecution::Replay { .. } => {
                // 同载荷回读：以当前持久化事实构造视图，不得重放写入。
                let current = self
                    .db
                    .bpm_workflow()
                    .find_current_execution(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
                    .await?;
                let instance = self
                    .db
                    .bpm_workflow()
                    .find_instance_by_id(&ApprovalProcessInstanceId::new(&instance_id), &mut NoTransaction)
                    .await?
                    .ok_or_else(hidden_not_found)?;
                Ok(map_command_view(
                    &instance,
                    current.as_ref(),
                    reason,
                    None,
                    None,
                    CommitRequired::Proceed,
                    true,
                ))
            }
            PreparedExecution::Apply(writes) => {
                let db = self.db.clone();
                let action_port = Arc::clone(&self.action_port);
                let actor = actor.clone();
                let actor_id = actor.id().to_string();
                let owner_role = spec.owner_role.as_str().to_string();
                let owner_organization_id = organization_id.clone();
                let subject_version = writes.instance.subject_version.to_string();
                let business_object_id = writes.instance.subject.subject_id().to_string();
                let document_type_label = document_type.label().to_string();
                let current_node_name = execution.node_name.clone();
                let should_finalize = writes.commit == CommitRequired::TerminalApproved;
                let action_context = ApprovalActionContext {
                    approval_process_instance_id: instance_id.clone(),
                    approval_node_execution_id: Some(execution_id.clone()),
                    work_item_id: Some(work_item_id.to_string()),
                    business_object_type: document_type.as_str().to_string(),
                    business_object_id: business_object_id.clone(),
                    subject_version: subject_version.clone(),
                    actor_id: actor_id.clone(),
                    reason: reason.clone(),
                    idempotency_key: key,
                };
                let final_action = spec.on_final_approve;
                let new_task_ids: Vec<String> = writes.create_tasks.iter().map(|_| next_id()).collect();
                let list_projection =
                    list_projection_from_writes(&writes, &execution_id, reason.clone(), now);
                let audit = actor.clone().resource_log_with_message(
                    "approval.decide",
                    "approval_process_instance",
                    instance_id.clone(),
                    Some(format!(
                        "decision={} reason={:?} work_item={}",
                        decision.as_str(),
                        reason,
                        work_item_id
                    )),
                )?;
                let client = self.db.client().clone();
                let work_item_id_owned = work_item_id.to_string();
                let view = client
                    .with_transaction(move |session| {
                        Box::pin(async move {
                            if should_finalize {
                                action_port
                                    .execute(final_action, &action_context, &actor, session)
                                    .await?;
                            }
                            persist_decision_writes(
                                &db,
                                &writes,
                                &execution_id,
                                expected_instance_version,
                                expected_execution_version,
                                expected_task_version,
                                &work_item_id_owned,
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
                                &current_node_name,
                                session,
                            )
                            .await?;
                            Ok::<ApprovalCommandView, crate::errors::Error>(map_command_view(
                                &writes.instance,
                                writes.created_executions.last(),
                                reason,
                                None,
                                first_open_task(&writes, &new_task_ids),
                                writes.commit,
                                false,
                            ))
                        })
                    })
                    .await?;
                Ok(view)
            }
        }
    }

    /// 写时重验审批人资格：账号启用、具备 `approval_instance:decide`、能读取
    /// 被审单据。任一失败收敛为对应人员 blocker，不得回滚为空。
    ///
    /// # 参数
    /// * `assignee_id` - 当前或下一节点审批人账号 ID
    /// * `assignee_name` - 定义或执行中的显示名快照
    /// * `organization_id` - 被审单据责任组织
    /// * `spec` - 单据类型审批适配器规格
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
        organization_id: &str,
        spec: &crate::approval::business_adapter::ApprovalAdapterSpec,
    ) -> Result<Eligibility> {
        let failure = match self
            .db
            .accounts()
            .find_approval_assignee_by_id(assignee_id, &mut NoTransaction)
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
                        organization_id: organization_id.to_string(),
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

    /// 读取实例节点当前审批人，作为三方责任校验的实例侧事实。
    ///
    /// # 参数
    /// * `instance_id` - 审批实例 ID
    /// * `node_key` - 当前定义节点键
    ///
    /// # 返回
    /// 返回实例审批人绑定中的当前参与人 ID。
    ///
    /// # 错误
    /// Repository 查询失败或实例缺少节点审批人绑定时返回错误。
    ///
    /// # 关键业务约束
    /// 查询条件由 BPM Repository 封装，Service 不拼装实例与节点过滤文档。
    async fn instance_assignee_id(&self, instance_id: &str, node_key: &str) -> Result<String> {
        let assignee = self
            .db
            .bpm_workflow()
            .find_assignee_for_node(
                &ApprovalProcessInstanceId::new(instance_id),
                node_key,
                &mut NoTransaction,
            )
            .await?
            .ok_or_else(|| Error::ConflictError("实例缺少节点审批人绑定".to_string()))?;
        Ok(assignee.current_assignee_participant_id.as_str().to_string())
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
        let digest = resume_digest(
            command.expected_instance_version,
            command.expected_execution_version,
            command.expected_assignment_version,
            command.expected_closed_task_version,
            actor.id(),
        );
        if let Some(receipt) = self
            .db
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::ResumeApprover,
                &instance_id,
                &idempotency_key,
                &mut NoTransaction,
            )
            .await?
        {
            receipt.reconcile(&digest).map_err(|_| payload_conflict_error())?;
            return self
                .persisted_command_view(&instance_id, CommitRequired::Proceed, true)
                .await;
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
                &snapshot.payload.responsible_org_id,
                &spec,
            )
            .await?;
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
            assignee,
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
        let actor_id = actor.id().to_string();
        let owner_role = spec.owner_role.as_str().to_string();
        let owner_organization_id = snapshot.payload.responsible_org_id.clone();
        let subject_version = snapshot.subject_version.to_string();
        let business_object_id = snapshot.business_object_id.clone();
        let document_type_label = document_type.label().to_string();
        let current_node_name = current.node_name.clone();
        let ended_execution_id = current.base.id.clone();
        let view = client
            .with_transaction(move |session| {
                Box::pin(async move {
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
                            actor_id: &actor_id,
                            owner_role: &owner_role,
                            owner_organization_id: &owner_organization_id,
                            subject_version: &subject_version,
                            business_object_id: &business_object_id,
                            document_type_label: &document_type_label,
                            current_node_name: &current_node_name,
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
            .await?;
        Ok(view)
    }

    /// 取消允许人工终止的非人员一致性受阻实例。
    ///
    /// # 参数
    /// * `actor` - 当前认证且具备恢复权限的审计主体
    /// * `command` - 受阻实例、执行和任务期望版本、取消原因及幂等键
    ///
    /// # 返回
    /// 返回取消后的审批命令视图；幂等回放返回已持久化视图。
    ///
    /// # 错误
    /// 人员失效 blocker、权限不足、版本冲突、快照不一致或事务写入失败时返回错误。
    ///
    /// # 关键业务约束
    /// 仅允许无开放任务的受阻端口取消，冻结快照三项主体引用必须全部精确匹配。
    pub async fn cancel_blocked(
        &self,
        actor: &AuditActor,
        command: ApprovalCancelBlockedCommand,
    ) -> Result<ApprovalCommandView> {
        ensure_command_actor(actor, &command.actor_id)?;
        let instance_id = command.approval_process_instance_id.clone();
        let reason = command.reason.trim().to_string();
        if reason.is_empty() {
            return Err(Error::ValidationError("受阻取消原因不能为空".to_string()));
        }
        let idempotency_key = normalize_idempotency_key(&command.idempotency_key)?;
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(&instance_id, &mut NoTransaction)
            .await?
            .ok_or_else(hidden_not_found)?;
        let recovery_scope = approval_recovery_scope(&self.db, &self.rbac, actor).await?;
        if !recovery_scope.covers(&snapshot.payload.responsible_org_id) {
            return Err(Error::Forbidden("无权取消该责任组织的受阻审批实例".to_string()));
        }
        if let Some(receipt) = self
            .db
            .bpm_workflow()
            .find_command_receipt(
                ApprovalCommandKind::CancelBlocked,
                &instance_id,
                &idempotency_key,
                &mut NoTransaction,
            )
            .await?
        {
            reconcile_cancel_blocked_receipt(&receipt, &command, &reason, actor.id())?;
            return self
                .persisted_command_view(&instance_id, CommitRequired::Cancelled, true)
                .await;
        }

        self.require_recovery_action(&instance_id, RuntimeRecoveryAction::CancelBlocked)
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
        let task_policy = instance
            .cancellation_task_policy()
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        if task_policy.closes_open_task() {
            return Err(Error::ConflictError("受阻取消不得处理运行中审批实例".to_string()));
        }
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
        let execution_id = ApprovalNodeExecutionId::new(current.base.id.clone());
        let open_tasks = self
            .db
            .work_items()
            .open_approval_tasks_for_execution(&execution_id, &mut NoTransaction)
            .await?;
        task_policy
            .ensure_open_task_count(open_tasks.len())
            .map_err(|error| Error::ConflictError(error.to_string()))?;
        self.validate_cancel_task_version(&execution_id, command.expected_task_version)
            .await?;

        let document_type = document_type_from_subject_kind(instance.subject.subject_kind())?;
        snapshot
            .ensure_matches_runtime_subject(
                document_type,
                instance.subject.subject_id(),
                instance.subject_version,
            )
            .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
        let spec = adapter_spec_of(document_type)?;
        let graph = self
            .db
            .bpm_workflow()
            .load_definition_graph(&instance.process_definition_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
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
            reason: reason.clone(),
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
            approval_process_instance_id: instance_id.clone(),
            approval_node_execution_id: Some(current.base.id.clone()),
            work_item_id: None,
            business_object_type: document_type.as_str().to_string(),
            business_object_id: snapshot.business_object_id.clone(),
            subject_version: snapshot.subject_version.to_string(),
            actor_id: actor.id().to_string(),
            reason: Some(reason.clone()),
            idempotency_key,
        };
        let audit = actor.clone().resource_log_with_message(
            "approval.cancel_blocked",
            "approval_process_instance",
            instance_id.clone(),
            Some(format!("execution={} reason={reason}", current.base.id)),
        )?;
        let db = self.db.clone();
        let action_port = Arc::clone(&self.action_port);
        let actor = actor.clone();
        let business_object_id = snapshot.business_object_id.clone();
        let document_type_label = document_type.label().to_string();
        let current_node_name = current.node_name.clone();
        let actor_id = actor.id().to_string();
        let cancel_action = spec.cancel_action;
        let client = self.db.client().clone();
        let view = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    action_port
                        .execute(cancel_action, &action_context, &actor, session)
                        .await?;
                    db.bpm_workflow()
                        .persist_cancelled_runtime(
                            &writes.instance,
                            &writes.updated_executions,
                            &writes.receipt,
                            session,
                        )
                        .await?;
                    persist_cancel_notifications(
                        &db,
                        &writes,
                        &actor_id,
                        &document_type_label,
                        &business_object_id,
                        &current_node_name,
                        now,
                        session,
                    )
                    .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ApprovalCommandView, crate::errors::Error>(map_command_view(
                        &writes.instance,
                        None,
                        None,
                        Some("DRAFT".to_string()),
                        None,
                        writes.commit,
                        false,
                    ))
                })
            })
            .await?;
        Ok(view)
    }

    /// 升级未提交单据绑定到当前发布定义。
    ///
    /// # 错误
    /// 已提交或版本冲突时返回错误。
    pub async fn upgrade_binding(
        &self,
        actor: &AuditActor,
        command: UpgradeBindingCommand,
    ) -> Result<ApprovalCommandView> {
        let context = crate::approval::business_adapter::BindingRevalidationContext {
            organization_id: String::new(),
            creator_id: actor.id().to_string(),
        };
        let _ = upgrade_unsubmitted_document_definition(
            &self.db,
            &self.rbac,
            &crate::approval::binding::UpgradeUnsubmittedDefinitionCommand {
                document_id: command.document_id.clone(),
                expected_document_version: command.expected_document_version,
                expected_binding_version: command.expected_approval_binding_version,
                reason: command.reason,
                context,
            },
            actor,
            &mut NoTransaction,
        )
        .await?;
        Ok(ApprovalCommandView {
            instance_id: command.document_id,
            instance_status: "DRAFT".to_string(),
            current_round_no: 0,
            current_node_key: None,
            current_node_name: None,
            current_assignee_participant_id: None,
            current_assignee_name: None,
            subject_status: Some("DRAFT".to_string()),
            latest_rejection_reason: None,
            next_open_task: None,
            outcome: super::view::ApprovalCommandOutcome::Applied,
        })
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

    async fn validate_cancel_task_version(
        &self,
        execution_id: &ApprovalNodeExecutionId,
        expected_task_version: Option<u64>,
    ) -> Result<()> {
        let Some(expected) = expected_task_version else {
            return Ok(());
        };
        let tasks = self
            .db
            .work_items()
            .approval_tasks_for_execution(execution_id, &mut NoTransaction)
            .await?;
        if tasks.len() != 1 {
            return Err(Error::ConflictError(
                "调用方声明了审批任务版本，但受阻执行未关联唯一历史任务".to_string(),
            ));
        }
        ensure_expected_version("审批任务", expected, tasks[0].base.version)
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
                    task_version: task.base.version,
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

/// 校验终态受阻取消收据，允许在 blocker 投影已清除后安全回放。
fn reconcile_cancel_blocked_receipt(
    receipt: &bpm::model::ApprovalCommandReceipt,
    command: &ApprovalCancelBlockedCommand,
    reason: &str,
    actor_id: &str,
) -> Result<()> {
    const STRUCTURAL_BLOCKERS: [ApprovalBlockerCode; 5] = [
        ApprovalBlockerCode::DefinitionGraphCorrupted,
        ApprovalBlockerCode::InstanceLinkCorrupted,
        ApprovalBlockerCode::OpenTaskConflict,
        ApprovalBlockerCode::SubjectVersionConflict,
        ApprovalBlockerCode::InternalInvariantBroken,
    ];
    let same_payload = STRUCTURAL_BLOCKERS.into_iter().any(|blocker| {
        receipt.payload_digest
            == cancel_blocked_digest(
                blocker.as_str(),
                command.expected_instance_version,
                command.expected_execution_version,
                command.expected_task_version,
                reason,
                actor_id,
            )
    });
    if same_payload {
        return Ok(());
    }
    Err(payload_conflict_error())
}

/// 在受阻取消事务内追加通知 outbox。
#[allow(clippy::too_many_arguments)]
async fn persist_cancel_notifications(
    db: &Database,
    writes: &PlannedWrites,
    actor_id: &str,
    document_type_label: &str,
    business_object_id: &str,
    current_node_name: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    if !writes.create_tasks.is_empty() || !writes.complete_tasks.is_empty() || !writes.close_tasks.is_empty()
    {
        return Err(Error::Internal(
            "受阻取消计划不得创建、完成或关闭审批任务".to_string(),
        ));
    }
    for intent in &writes.notifications {
        let record = entities::approval_integration::ApprovalNotificationOutbox::enqueue(
            entities::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            vec![actor_id.to_string()],
            entities::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: document_type_label.to_string(),
                document_no: business_object_id.to_string(),
                current_node_name: current_node_name.to_string(),
                current_approver_display_name: actor_id.to_string(),
                round_no: writes.instance.current_round_no,
                reject_reason_summary: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
    Ok(())
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
        task_version: 1,
        owner_user_id: assignee.as_str().to_string(),
    })
}

/// 单事务应用决定写入：实例 CAS 推进、执行结束/插入、审批人绑定、收据、
/// 任务完成/关闭/新建、通知 outbox 与审计。
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
    current_node_name: &str,
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
    let mut assignees = writes.created_assignees.clone();
    assignees.extend(writes.updated_assignees.clone());
    if !assignees.is_empty() {
        db.bpm_workflow().insert_assignees(&assignees, session).await?;
    }
    // 命令收据。
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, session)
        .await?;
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
    // 通知 outbox（按去重键追加）。
    for intent in &writes.notifications {
        let record = entities::approval_integration::ApprovalNotificationOutbox::enqueue(
            entities::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            vec![actor_id.to_string()],
            entities::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: document_type_label.to_string(),
                document_no: business_object_id.to_string(),
                current_node_name: current_node_name.to_string(),
                current_approver_display_name: actor_id.to_string(),
                round_no: writes.instance.current_round_no,
                reject_reason_summary: None,
            },
            now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
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
    actor_id: &'a str,
    owner_role: &'a str,
    owner_organization_id: &'a str,
    subject_version: &'a str,
    business_object_id: &'a str,
    document_type_label: &'a str,
    current_node_name: &'a str,
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
    if !input.writes.created_assignees.is_empty() || !input.writes.updated_assignees.is_empty() {
        return Err(Error::Internal("原审批人恢复不得修改实例审批人绑定".to_string()));
    }
    for execution in &input.writes.created_executions {
        db.bpm_workflow().insert_execution(execution, session).await?;
    }
    db.bpm_workflow()
        .insert_command_receipt(&input.writes.receipt, session)
        .await?;
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
    let recipient = input
        .writes
        .created_executions
        .last()
        .map(|execution| execution.assignee_participant_id.as_str().to_string())
        .unwrap_or_else(|| input.actor_id.to_string());
    for intent in &input.writes.notifications {
        let record = entities::approval_integration::ApprovalNotificationOutbox::enqueue(
            entities::ids::ApprovalNotificationOutboxId::new(intent.dedup_key.clone()),
            intent.dedup_key.clone(),
            intent.event_kind,
            vec![recipient.clone()],
            entities::approval_integration::ApprovalNotificationTemplateParams {
                document_type_label: input.document_type_label.to_string(),
                document_no: input.business_object_id.to_string(),
                current_node_name: input.current_node_name.to_string(),
                current_approver_display_name: recipient.clone(),
                round_no: input.writes.instance.current_round_no,
                reject_reason_summary: None,
            },
            input.now,
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        db.approval_notification_outbox().create(&record, session).await?;
    }
    db.audit_logs().create(input.audit, session).await?;
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

    use bpm::engine::TaskCloseReason;
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::{ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus};
    use bpm::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, NewProcessInstance, ParticipantId,
        Timestamp,
    };
    use bpm::{ProcessKind, SubjectRef};
    use database::repository::approval_integration::{ApprovalRuntimeReadRow, ApprovalRuntimeReadTypeScope};
    use database::repository::bpm::{ApprovalInstanceListView, ApprovalInstanceSummary};
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

    use crate::audit::AuditActor;
    use crate::errors::Error;

    use super::{
        approval_task_ending, cursor_from_summary, ensure_mine_page_integrity, item_from_runtime_read_row,
        item_from_summary, management_runtime_read_allowed, mine_execution_ids, mine_instance_ids,
        mine_runtime_chain_matches, ordinary_runtime_read_allowed, started_runtime_read_allowed,
        task_proves_current_responsibility, unique_by_id, CompleteOrCloseTasksInput, RuntimeInstanceListView,
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
}
