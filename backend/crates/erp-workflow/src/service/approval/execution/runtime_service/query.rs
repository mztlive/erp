//! 运行实例列表、详情、历史与恢复选项查询。

mod query_list;

use application_core::AuditActor;
use bpm::engine::TaskIntent;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::ApprovalNodeExecution;
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use erp_core::common::time::Instant;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::super::apply_plan::PlannedWrites;
use super::super::runtime_history::{RuntimeHistoryPage, history_item_from_execution, history_page_from};
use super::super::runtime_query::{
    RuntimeInstanceListView, RuntimeInstanceStatusFilter, RuntimeRecoveryAction, recovery_options_for,
};
use super::super::view::OpenTaskSummary;
pub(crate) use super::query_contract::parse_document_type;
pub use super::query_contract::{RuntimeInstanceListCursor, RuntimeInstanceListQuery};
use super::read_auth::{
    RuntimeReadAuthorizationFacts, RuntimeReadSubject, current_execution_matches_instance,
    task_proves_current_responsibility,
};
use super::{ApprovalRuntimeService, hidden_not_found};
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::error::Result;
use crate::ports::OrderTaskSource;
use crate::repository::approval_integration::{ApprovalRuntimeReadRow, ApprovalRuntimeReadTypeScope};
use crate::repository::bpm::{
    ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListProjection,
    ApprovalInstanceListView, ApprovalInstanceSummary, ApprovalInstanceTextQuery,
};
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::scope::definition_management_visibility;
use crate::service::approval::{approval_actor_is_active, approval_document_read_scope};

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
    /// 审批对应的冻结业务版本，供只读单据摘要选择使用。
    #[serde(default)]
    pub subject_version: Option<u32>,
    /// 与实例、对象及提交版本匹配的启动快照金额；未知或非金额审批为 None。
    #[serde(default)]
    pub total_amount: Option<erp_core::money::Amount>,
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
    /// 恢复命令所需的期望实例版本，十进制字符串。
    #[serde(default)]
    pub expected_instance_version: String,
    /// 恢复命令所需的期望执行版本，无当前执行时为空。
    #[serde(default)]
    pub expected_execution_version: Option<String>,
    /// 恢复命令所需的期望审批人绑定版本，无绑定时为空。
    #[serde(default)]
    pub expected_assignment_version: Option<String>,
    /// 单一已关闭历史任务版本，无唯一关闭任务时为空；调用方可省略。
    #[serde(default)]
    pub expected_closed_task_version: Option<String>,
}

/// 恢复版本提示。
#[derive(Debug, Clone, PartialEq, Eq)]
struct ResumeVersionHints {
    /// 实例版本字符串。
    instance_version: String,
    /// 当前执行版本字符串。
    execution_version: Option<String>,
    /// 当前节点审批人绑定版本字符串。
    assignment_version: Option<String>,
    /// 唯一历史任务版本字符串。
    closed_task_version: Option<String>,
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
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
            execution.as_ref().map(|item| item.assignee_participant_id.as_str().to_string()),
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
    /// 返回实例 ID、允许的恢复动作集合与恢复命令所需的期望版本提示。
    ///
    /// # 错误
    /// 实例不存在或无权时不泄露存在性；版本提示读取失败时传播仓储错误。
    ///
    /// # 关键业务约束
    /// 版本提示与动作判定基于同一读取快照；调用方仍须按恢复命令重验版本。
    pub async fn recovery_options(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeRecoveryOptionsView> {
        self.ensure_active_instance_reader(actor).await?;
        let subject = self.load_runtime_read_subject(instance_id).await?;
        self.ensure_management_runtime_read(actor, &subject).await?;
        let blocked = subject.instance.status == ApprovalProcessInstanceStatus::Blocked;
        let actions = recovery_options_for(blocked, subject.instance.blocker_code);
        let hints = self.resume_version_hints(&subject).await?;
        Ok(RuntimeRecoveryOptionsView {
            instance_id: instance_id.to_string(),
            actions,
            expected_instance_version: hints.instance_version,
            expected_execution_version: hints.execution_version,
            expected_assignment_version: hints.assignment_version,
            expected_closed_task_version: hints.closed_task_version,
        })
    }

    /// 读取恢复命令所需的实例、执行、绑定与历史任务版本提示。
    ///
    /// # 参数
    /// * `subject` - 已加载的实例、当前执行、快照与单据类型
    ///
    /// # 返回
    /// 返回十进制字符串形式的版本提示；缺失执行、绑定或唯一历史任务时对应项为空。
    ///
    /// # 错误
    /// 仓储读取失败时传播持久化错误；多历史任务不断言失败，仅返回空提示。
    ///
    /// # 关键业务约束
    /// 提示仅供调用方构造恢复命令，事务提交前仍须按期望版本做 CAS 重验。
    async fn resume_version_hints(&self, subject: &RuntimeReadSubject) -> Result<ResumeVersionHints> {
        let instance_version = subject.instance.base.version.to_string();
        let Some(execution) = subject.current_execution.as_ref() else {
            return Ok(ResumeVersionHints {
                instance_version,
                execution_version: None,
                assignment_version: None,
                closed_task_version: None,
            });
        };
        let execution_version = Some(execution.base.version.to_string());
        let instance_id = ApprovalProcessInstanceId::new(subject.instance.base.id.clone());
        let assignee = self
            .db
            .bpm_workflow()
            .find_assignee_for_node(&instance_id, &execution.node_key, &mut NoTransaction)
            .await?;
        let assignment_version = assignee.map(|item| item.base.version.to_string());
        let closed_task_version = self.closed_resume_task_version(execution).await?;
        Ok(ResumeVersionHints {
            instance_version,
            execution_version,
            assignment_version,
            closed_task_version,
        })
    }

    /// 读取受阻执行关联的唯一历史任务版本。
    ///
    /// # 参数
    /// * `execution` - 当前受阻执行
    ///
    /// # 返回
    /// 唯一历史任务存在时返回其版本字符串；无任务或多任务时返回空。
    ///
    /// # 错误
    /// 仓储读取失败时传播持久化错误。
    ///
    /// # 关键业务约束
    /// 多任务属于恢复命令的失败关闭场景，此处不提前报错，由命令执行时判定。
    async fn closed_resume_task_version(&self, execution: &ApprovalNodeExecution) -> Result<Option<String>> {
        let execution_id = ApprovalNodeExecutionId::new(execution.base.id.clone());
        let tasks =
            self.db.work_items().approval_tasks_for_execution(&execution_id, &mut NoTransaction).await?;
        let [task] = tasks.as_slice() else {
            return Ok(None);
        };
        Ok(Some(task.base.version.to_string()))
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
        if approval_actor_is_active(&self.auth, actor).await? {
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
        let document_type = crate::entity::approval_integration::resolve_runtime_document_type(
            instance.subject.subject_kind(),
            instance.process_kind,
        )
        .map_err(|_| hidden_not_found())?;
        adapter_spec_of(document_type)?;
        if snapshot
            .ensure_matches_runtime_subject(
                document_type,
                instance.subject.subject_id(),
                instance.subject_version,
            )
            .is_err()
        {
            return Err(hidden_not_found());
        }
        let current_execution =
            self.db.bpm_workflow().find_current_execution(&instance_id, &mut NoTransaction).await?;
        if !current_execution_matches_instance(&instance, current_execution.as_ref()) {
            return Err(hidden_not_found());
        }
        Ok(RuntimeReadSubject { instance, current_execution, snapshot, document_type })
    }

    /// 订单审批详情不能由启动人或历史责任绕过当前订单详情范围。
    async fn ensure_order_approval_read(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<()> {
        if OrderTaskSource::approval_kind(subject.document_type).is_some()
            && !self
                .auth
                .order_approval_readable(
                    actor,
                    subject.document_type,
                    &subject.snapshot.business_object_id,
                    &mut NoTransaction,
                )
                .await?
        {
            return Err(hidden_not_found());
        }
        Ok(())
    }

    /// 校验普通详情/历史读取的三条互斥授权来源。
    async fn ensure_ordinary_runtime_read(
        &self,
        actor: &AuditActor,
        subject: &RuntimeReadSubject,
    ) -> Result<()> {
        self.ensure_order_approval_read(actor, subject).await?;
        let initiator = subject.instance.started_by.as_str() == actor.id();
        if (RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator,
            current_responsibility: false,
            object_readable: false,
            scope_covers: false,
            runtime_admin: false,
        })
        .ordinary_allowed()
        {
            return Ok(());
        }
        if self.current_runtime_responsibility(actor, subject).await? {
            return Ok(());
        }
        adapter_spec_of(subject.document_type)?;
        let scope = approval_document_read_scope(&self.auth, actor, subject.document_type).await?;
        let facts = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: false,
            current_responsibility: false,
            object_readable: !scope.is_empty(),
            scope_covers: scope.covers_object(
                &self
                    .auth
                    .approval_scope_object(
                        subject.document_type,
                        &subject.snapshot.business_object_id,
                        &mut NoTransaction,
                    )
                    .await?,
            ),
            runtime_admin: false,
        };
        if facts.ordinary_allowed() {
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
        self.ensure_order_approval_read(actor, subject).await?;
        let visibility = definition_management_visibility(&self.auth, actor).await?;
        adapter_spec_of(subject.document_type)?;
        let scope = approval_document_read_scope(&self.auth, actor, subject.document_type).await?;
        let facts = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: subject.instance.started_by.as_str() == actor.id(),
            current_responsibility: false,
            object_readable: !scope.is_empty(),
            scope_covers: scope.covers_object(
                &self
                    .auth
                    .approval_scope_object(
                        subject.document_type,
                        &subject.snapshot.business_object_id,
                        &mut NoTransaction,
                    )
                    .await?,
            ),
            runtime_admin: visibility.runtime_admin_types().contains(&subject.document_type),
        };
        if facts.management_allowed() {
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
        let tasks =
            self.db.work_items().open_approval_tasks_for_execution(&execution_id, &mut NoTransaction).await?;
        if tasks.len() != 1 {
            return Ok(false);
        }
        let owner_role = adapter_spec_of(subject.document_type)?.owner_role;
        Ok(task_proves_current_responsibility(&tasks[0], execution, subject, actor.id(), owner_role.as_str()))
    }
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
pub(super) fn instance_list_filter(
    actor: &AuditActor,
    query: &RuntimeInstanceListQuery,
) -> Result<ApprovalInstanceListFilter> {
    let view = match query.view {
        RuntimeInstanceListView::Started => ApprovalInstanceListView::Started,
        RuntimeInstanceListView::Blocked => ApprovalInstanceListView::Blocked,
        _ => ApprovalInstanceListView::Managed,
    };
    let process_kind =
        query.document_type.as_deref().map(parse_document_type).transpose()?.map(process_kind_of);
    Ok(ApprovalInstanceListFilter {
        view,
        process_kind,
        status: query.status.map(map_status_filter),
        started_by: (query.view == RuntimeInstanceListView::Started).then(|| actor.id().to_string()),
        subject_kind: None,
        authorized_instance_ids: None,
        subject_ids: None,
        text_query: query.query.as_ref().map(|text| ApprovalInstanceTextQuery { query: text.clone() }),
        cursor: query
            .cursor
            .as_ref()
            .map(|cursor| ApprovalInstanceListCursor { sort_time: cursor.sort_time, id: cursor.id.clone() }),
        limit: query.limit,
    })
}

/// 从当前视图最后一行生成下一页游标。
pub(super) fn cursor_from_summary(
    view: ApprovalInstanceListView,
    row: &ApprovalInstanceSummary,
) -> RuntimeInstanceListCursor {
    let updated_at = i64::try_from(row.updated_at).unwrap_or(i64::MAX);
    let sort_time = match view {
        ApprovalInstanceListView::Started => row.started_at,
        ApprovalInstanceListView::Blocked => row.blocked_at.unwrap_or(updated_at),
        ApprovalInstanceListView::Managed => updated_at,
    };
    RuntimeInstanceListCursor { sort_time, id: row.id.clone() }
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
pub(super) fn item_from_summary(
    row: ApprovalInstanceSummary,
    snapshot: Option<&ApprovalSubjectSnapshot>,
) -> Result<RuntimeInstanceListItem> {
    let document_type = crate::entity::approval_integration::resolve_runtime_document_type(
        row.subject.subject_kind(),
        row.process_kind,
    )
    .map_err(|_| hidden_not_found())?;
    let document_id = row.subject.subject_id().to_string();
    let snapshot = snapshot
        .filter(|snapshot| snapshot.approval_process_instance_id.as_ref() == row.id.as_str())
        .filter(|snapshot| {
            snapshot.ensure_matches_runtime_subject(document_type, &document_id, row.subject_version).is_ok()
        });
    let document_label = snapshot.map(|snapshot| snapshot.payload.document_no.clone());
    let total_amount = snapshot.and_then(|snapshot| snapshot.payload.total_amount);
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
        subject_version: Some(row.subject_version),
        total_amount,
        process_version: Some(row.definition_version),
        started_at: Some(row.started_at),
        latest_rejection_summary: row.latest_rejection_summary,
    })
}

/// 对聚合页行重新执行对象读取与授权矩阵，然后映射公开列表行。
pub(super) fn item_from_runtime_read_row(
    row: ApprovalRuntimeReadRow,
    actor: &AuditActor,
    view: RuntimeInstanceListView,
    type_scopes: &[ApprovalRuntimeReadTypeScope],
) -> Result<RuntimeInstanceListItem> {
    let document_type = crate::entity::approval_integration::document_type_from_subject_kind(
        row.instance.subject.subject_kind(),
    )
    .map_err(|_| hidden_not_found())?;
    let type_allowed = type_scopes.iter().any(|scope| scope.process_kind == row.instance.process_kind);
    let facts = RuntimeReadAuthorizationFacts {
        actor_active: true,
        initiator: row.instance.started_by == actor.id(),
        current_responsibility: false,
        object_readable: type_allowed,
        scope_covers: type_allowed,
        runtime_admin: type_allowed,
    };
    let allowed = match view {
        RuntimeInstanceListView::Started => facts.started_allowed(),
        RuntimeInstanceListView::Managed | RuntimeInstanceListView::Blocked => facts.management_allowed(),
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
        subject_version: None,
        total_amount: None,
        process_version: None,
        started_at: None,
        latest_rejection_summary: None,
    }
}

/// 由决定后的写入构造实例列表投影。
pub(super) fn list_projection_from_writes(
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

/// 视图中的首个新建开放任务摘要。
pub(super) fn first_open_task(writes: &PlannedWrites, new_task_ids: &[String]) -> Option<OpenTaskSummary> {
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

#[cfg(test)]
mod runtime_recovery_options_view_tests {
    use serde_json::json;

    use super::{RuntimeRecoveryAction, RuntimeRecoveryOptionsView};

    /// 旧恢复选项载荷缺版本提示时须兼容反序列化为空提示。
    #[test]
    fn legacy_payload_without_version_hints_deserializes_with_defaults() {
        let view: RuntimeRecoveryOptionsView = serde_json::from_value(json!({
            "instance_id": "inst-1",
            "actions": ["RESUME_CURRENT_APPROVER"],
        }))
        .expect("旧载荷");
        assert_eq!(view.instance_id, "inst-1");
        assert_eq!(view.actions, vec![RuntimeRecoveryAction::ResumeCurrentApprover]);
        assert_eq!(view.expected_instance_version, String::new());
        assert_eq!(view.expected_execution_version, None);
        assert_eq!(view.expected_assignment_version, None);
        assert_eq!(view.expected_closed_task_version, None);
    }

    /// 版本提示序列化往返须保留十进制字符串。
    #[test]
    fn version_hints_round_trip_preserves_strings() {
        let view = RuntimeRecoveryOptionsView {
            instance_id: "inst-1".to_string(),
            actions: vec![RuntimeRecoveryAction::ResumeCurrentApprover],
            expected_instance_version: "3".to_string(),
            expected_execution_version: Some("2".to_string()),
            expected_assignment_version: Some("1".to_string()),
            expected_closed_task_version: None,
        };
        let value = serde_json::to_value(&view).expect("序列化");
        let round_trip: RuntimeRecoveryOptionsView = serde_json::from_value(value).expect("反序列化");
        assert_eq!(round_trip, view);
    }
}
