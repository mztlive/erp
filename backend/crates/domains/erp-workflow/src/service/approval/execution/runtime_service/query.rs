//! 运行实例列表、详情、历史与恢复选项查询。

use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{approval_instance as approval_permissions, internal as internal_permissions};
mod detail;
mod query_list;

use std::collections::BTreeMap;

use application_core::AuditActor;
use bpm::engine::TaskIntent;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::ApprovalNodeExecution;
use bpm::model::types::ApprovalProcessInstanceStatus;
use erp_core::common::time::Instant;
use persistence_core::{Executor, Transactional};
use serde::{Deserialize, Serialize};

use super::super::apply_plan::PlannedWrites;
use super::super::runtime_history::{
    ExecutionHistoryOrder, RuntimeHistoryPage, history_item_from_execution, history_page_from,
};
use super::super::runtime_query::{
    RuntimeInstanceListView, RuntimeInstanceStatusFilter, RuntimeRecoveryAction, recovery_options_for,
};
use super::super::view::OpenTaskSummary;
pub(crate) use super::query_contract::parse_document_type;
pub use super::query_contract::{RuntimeInstanceListCursor, RuntimeInstanceListQuery};
use super::read_auth::{RuntimeReadSubject, current_execution_matches_instance};
use super::{ApprovalRuntimeService, hidden_not_found};
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::approval_integration::{ApprovalRuntimeReadRow, ApprovalRuntimeReadTypeScope};
use crate::repository::bpm::{
    ApprovalInstanceListCursor, ApprovalInstanceListFilter, ApprovalInstanceListProjection,
    ApprovalInstanceListView, ApprovalInstanceSummary, ApprovalInstanceTextQuery,
};
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::require_approval_management_with_executor;
use crate::service::work_item::approval_rework::open_rework_id;

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

/// 已授权实例详情及当前读者持有的开放驳回责任。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeInstanceDetail {
    /// 保持详情已有的实例字段布局。
    #[serde(flatten)]
    pub instance: RuntimeInstanceListItem,
    /// 精确实例对应的开放待办；办理权限仍由任务详情与命令重验。
    pub rework_task_id: Option<String>,
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
    /// 本人发起视图的完整单据类型数量；其他视图不提供。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_type_counts: Option<BTreeMap<String, u64>>,
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

impl<A: WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 按固定 view 查询实例摘要。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `query` - 已规范化查询
    ///
    /// # 返回
    /// 返回当前视图的一页实例摘要。
    ///
    /// # 错误
    /// 查询合同不成立、读者无效或列表读取失败时返回错误。
    pub async fn instance_list(
        &self,
        actor: &AuditActor,
        query: RuntimeInstanceListQuery,
    ) -> Result<RuntimeInstanceListPage> {
        query.validate()?;
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
    /// 返回实例状态、当前审批人及该实例冻结的单据身份、金额与版本。
    ///
    /// # 错误
    /// 不存在或无权时不泄露存在性；仓储失败时返回对应错误。
    pub async fn instance_detail(
        &self,
        actor: &AuditActor,
        instance_id: &str,
    ) -> Result<RuntimeInstanceDetail> {
        let service = self.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.instance_detail_stage(&actor, &instance_id, executor).await })
            })
            .await
    }

    /// 在原快照中执行实例读取资格、主体和当前返工任务读取。
    /// # 参数
    /// actor、instance_id 为已认证读者与精确实例，executor 为原快照执行器。
    /// # 返回
    /// 同一快照授权的实例详情。
    /// # 错误
    /// 无权、主体链不一致或仓储读取失败时拒绝。
    pub(in crate::service::approval) async fn instance_detail_stage(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<RuntimeInstanceDetail> {
        self.ensure_ordinary_runtime_read_stage(actor, instance_id, executor).await?;
        let subject = self.load_runtime_read_subject_stage(instance_id, executor).await?;
        let instance = RuntimeInstanceListItem::from_read_subject(&subject)?;
        let rework_task_id = open_rework_id(&self.db, &subject.instance, actor.id(), executor).await?;
        Ok(RuntimeInstanceDetail { instance, rework_task_id })
    }

    /// 读取实例执行历史。
    ///
    /// # 参数
    /// * `actor` - 操作人
    /// * `instance_id` - 实例 ID
    /// * `after_execution_no` - 上一页最后执行序号；首页为空
    /// * `limit` - 页大小
    /// * `order` - 分页方向。
    ///
    /// # 返回
    /// 返回指定方向的历史页，字段对齐审批 Tab。
    ///
    /// # 错误
    /// 读者无效或不存在、无权时不泄露存在性；仓储失败时返回对应错误。
    pub async fn instance_history(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        after_execution_no: Option<u32>,
        limit: u32,
        order: ExecutionHistoryOrder,
    ) -> Result<RuntimeHistoryPage> {
        let service = self.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .instance_history_stage(
                            &actor,
                            &instance_id,
                            after_execution_no,
                            limit,
                            order,
                            executor,
                        )
                        .await
                })
            })
            .await
    }

    /// 历史授权、执行记录和冻结路线沿同一事务读取。
    async fn instance_history_stage(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        after_execution_no: Option<u32>,
        limit: u32,
        order: ExecutionHistoryOrder,
        executor: &mut dyn Executor,
    ) -> Result<RuntimeHistoryPage> {
        self.ensure_ordinary_runtime_read_stage(actor, instance_id, executor).await?;
        let subject = self.load_runtime_read_subject_stage(instance_id, executor).await?;

        let fetch_limit = limit.saturating_add(1);
        let rows = self
            .db
            .bpm_workflow()
            .list_execution_history_ordered(
                &ApprovalProcessInstanceId::new(instance_id),
                after_execution_no,
                fetch_limit,
                order,
                executor,
            )
            .await?;

        let items = rows.iter().map(history_item_from_execution).collect();
        let mut page = history_page_from(items, limit);
        if after_execution_no.is_none() {
            page.route = self.sales_approval_route(&subject, executor).await?;
        }
        page.observe_progress(&subject.instance, actor.id(), after_execution_no, order);
        Ok(page)
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
        let this = self.clone();
        let actor = actor.clone();
        let instance_id = instance_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { this.recovery_options_stage(&actor, &instance_id, executor).await })
            })
            .await
    }

    async fn recovery_options_stage(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<RuntimeRecoveryOptionsView> {
        self.ensure_management_runtime_read_stage(actor, instance_id, executor).await?;
        let subject = self.load_runtime_read_subject_stage(instance_id, executor).await?;

        let blocked = subject.instance.status == ApprovalProcessInstanceStatus::Blocked;
        let candidates = recovery_options_for(blocked, subject.instance.blocker_code);
        let actions = authorized_recovery_actions(
            &self.authorization,
            actor,
            subject.document_type,
            &subject.instance.base.id,
            candidates,
            executor,
        )
        .await?;

        let hints = self.resume_version_hints(&subject, executor).await?;
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
    async fn resume_version_hints(
        &self,
        subject: &RuntimeReadSubject,
        executor: &mut dyn Executor,
    ) -> Result<ResumeVersionHints> {
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
            .find_assignee_for_node(&instance_id, &execution.node_key, executor)
            .await?;
        let assignment_version = assignee.map(|item| item.base.version.to_string());
        let closed_task_version = self.closed_resume_task_version(execution, executor).await?;
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
    async fn closed_resume_task_version(
        &self,
        execution: &ApprovalNodeExecution,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let execution_id = ApprovalNodeExecutionId::new(execution.base.id.clone());
        let tasks = self.db.work_items().approval_tasks_for_execution(&execution_id, executor).await?;
        let [task] = tasks.as_slice() else {
            return Ok(None);
        };
        Ok(Some(task.base.version.to_string()))
    }

    /// 在原执行器读取并核对实例、当前执行与冻结快照。
    pub(super) async fn load_runtime_read_subject_stage(
        &self,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<RuntimeReadSubject> {
        let instance_id = ApprovalProcessInstanceId::new(instance_id);
        let instance = self
            .db
            .bpm_workflow()
            .find_instance_by_id(&instance_id, executor)
            .await?
            .ok_or_else(hidden_not_found)?;
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(instance_id.as_ref(), executor)
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

        let current_execution = self.db.bpm_workflow().find_current_execution(&instance_id, executor).await?;
        if !current_execution_matches_instance(&instance, current_execution.as_ref()) {
            return Err(hidden_not_found());
        }
        Ok(RuntimeReadSubject { instance, current_execution, snapshot, document_type })
    }

    /// 普通与管理读取由同一固定入口选择，业务只保留运行主体完整性检查。
    pub(super) async fn ensure_ordinary_runtime_read_stage(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .require(
                actor,
                &internal_permissions::APPROVAL_INSTANCE_READ,
                AccessTarget::record(instance_id)?,
                executor,
            )
            .await
            .map_err(|error| match Error::from(error) {
                Error::Forbidden(_) | Error::NotFound(_) => hidden_not_found(),
                other => other,
            })
    }

    async fn ensure_management_runtime_read_stage(
        &self,
        actor: &AuditActor,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .require(
                actor,
                &internal_permissions::APPROVAL_MANAGEMENT_READ,
                AccessTarget::record(instance_id)?,
                executor,
            )
            .await
            .map_err(|error| match Error::from(error) {
                Error::Forbidden(_) | Error::NotFound(_) => hidden_not_found(),
                other => other,
            })
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
///
/// # 参数
/// * `view` - 仓储列表视图。
/// * `row` - 当前页最后一行摘要。
///
/// # 返回
/// `Started` 用启动时间，`Blocked` 用受阻时间或回退到更新时间，其余用更新时间；ID 取实例 ID。
///
/// # 错误
/// 不返回错误。
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
        RuntimeInstanceStatusFilter::Rejected => ApprovalProcessInstanceStatus::Rejected,
        RuntimeInstanceStatusFilter::Cancelled => bpm::model::types::ApprovalProcessInstanceStatus::Cancelled,
        RuntimeInstanceStatusFilter::Blocked => bpm::model::types::ApprovalProcessInstanceStatus::Blocked,
    }
}

/// 由仓储摘要与启动快照映射列表行。
///
/// # 参数
/// * `row` - 仓储实例摘要。
/// * `snapshot` - 同实例的冻结快照；缺失或不匹配时只清空展示字段。
///
/// # 返回
/// 返回带运行投影的列表行；快照不匹配时单据号和金额为空。
///
/// # 错误
/// 主体种类与流程种类不能解析为正式单据类型时隐藏为不存在。
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

/// 核对已授权聚合行的视图归属和流程类型，然后映射公开列表行。
///
/// # 参数
/// * `row` - 已聚合的实例与可选快照。
/// * `actor` - 当前读者。
/// * `view` - 对外列表视图。
/// * `type_scopes` - 本视图允许进入仓储的流程种类。
///
/// # 返回
/// 授权通过后返回公开列表行。
///
/// # 错误
/// `Mine`、类型不在范围内、流程种类不一致或主体无法解析时隐藏为不存在。
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
    let allowed = match view {
        RuntimeInstanceListView::Started => row.instance.started_by == actor.id(),
        RuntimeInstanceListView::Managed | RuntimeInstanceListView::Blocked => type_allowed,
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

/// 由决定后的写入构造实例列表投影。
///
/// # 参数
/// * `writes` - 决定计划。
/// * `ended_execution_id` - 刚结束的执行 ID。
/// * `reject_reason` - 驳回原因；通过时为空。
/// * `now` - 状态变更时间。
///
/// # 返回
/// 当前节点取最后一条新建执行；有驳回原因时记录刚结束的执行。
///
/// # 错误
/// 不返回错误。
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
        current_assignee_name: current.map(|item| item.assignee_display_name()),
        latest_rejected_execution_id: reject_reason.as_ref().map(|_| ended_execution_id.to_string()),
        latest_rejection_summary: reject_reason,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// 视图中的首个新建开放任务摘要。
///
/// # 参数
/// * `writes` - 决定或恢复计划。
/// * `new_task_ids` - 与新建任务意图对齐的任务 ID。
///
/// # 返回
/// 首个意图是人工任务且已分配 ID 时返回摘要；否则返回 `None`。版本固定为 `"1"`。
///
/// # 错误
/// 不返回错误。
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

/// 恢复动作展示复用写端的完整同角色权限及真实来源授权。
async fn authorized_recovery_actions(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    document_type: DocumentType,
    document_id: &str,
    candidates: Vec<RuntimeRecoveryAction>,
    executor: &mut dyn Executor,
) -> Result<Vec<RuntimeRecoveryAction>> {
    let mut actions = Vec::new();
    for action in candidates {
        let permission = match action {
            RuntimeRecoveryAction::ResumeCurrentApprover => &internal_permissions::APPROVAL_RECOVER,
            RuntimeRecoveryAction::CancelBlocked => &approval_permissions::CANCEL_BLOCKED,
        };
        match require_approval_management_with_executor(
            authorization,
            actor,
            permission,
            document_type,
            document_id,
            executor,
        )
        .await
        {
            Ok(()) => actions.push(action),
            Err(Error::Forbidden(_)) => {},
            Err(error) => return Err(error),
        }
    }
    Ok(actions)
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

#[cfg(test)]
mod authorization_tests {
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::FailClosedWorkflowAuthorizationPort;
    use crate::service::approval::definition::test_support::unavailable_service;

    #[tokio::test]
    async fn recovery_options_stop_before_subject_reads_when_authorization_fails() {
        let (definition, actor) = unavailable_service().await;
        let runtime = ApprovalRuntimeService::new(
            definition.db,
            definition.identity,
            definition.authorization,
            FailClosedWorkflowAuthorizationPort,
        );
        let result =
            runtime.recovery_options_stage(&actor, "unread-instance", &mut SnapshotExecutor::default()).await;
        assert!(matches!(result, Err(Error::Internal(reason)) if reason == "测试授权设施不可用"));
    }
}
