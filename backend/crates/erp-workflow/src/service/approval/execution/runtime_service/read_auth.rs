//! 运行读取授权矩阵与当前责任链判定。

use std::collections::{HashMap, HashSet};

use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::{AssignmentSource, WorkItem, WorkItemStatus, WorkItemType};
use crate::repository::bpm::ApprovalInstanceSummary;
use bpm::engine::Eligibility;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use mongodb::Database;
use persistence_core::Executor;

use super::super::authorization::{converge_eligibility, AuthorizationFailure};
use super::hidden_not_found;
use crate::error::{Error, ErrorCode, Result};
use crate::ports::{ApprovalObjectReadPort, OrderTaskSource};
use crate::service::approval::business_adapter::{
    adapter_object_read_decision_with, adapter_spec_of, ensure_separation_of_duties,
    BindingRevalidationContext,
};
use crate::service::approval::policy::{policy_of, DocumentApprovalPolicy, SeparationOfDutiesPolicy};
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::{
    approval_decide_scope_with_executor, approval_document_read_scope_with_executor,
};
use application_core::AuditActor;

/// 单实例读取授权所需的持久化事实。
pub(super) struct RuntimeReadSubject {
    pub(super) instance: ApprovalProcessInstance,
    pub(super) current_execution: Option<ApprovalNodeExecution>,
    pub(super) snapshot: ApprovalSubjectSnapshot,
    pub(super) document_type: DocumentType,
}

/// 纯授权矩阵输入；I/O 与政策解析由 Service 先完成。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct RuntimeReadAuthorizationFacts {
    pub(super) actor_active: bool,
    pub(super) initiator: bool,
    pub(super) current_responsibility: bool,
    pub(super) object_readable: bool,
    pub(super) scope_covers: bool,
    pub(super) runtime_admin: bool,
}

/// 决定审批人写时重验输入。
///
/// # 用途
/// 打包 [`revalidate_decision_approver`] 的审批人与主体事实。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 认证主体若提供，必须与账号 ID/类型一致。
pub(super) struct RevalidateDecisionApproverInput<'a> {
    pub(super) assignee_id: &'a str,
    pub(super) assignee_name: &'a str,
    pub(super) authenticated_actor: Option<&'a AuditActor>,
    pub(super) snapshot: &'a ApprovalSubjectSnapshot,
    pub(super) spec: &'a crate::service::approval::business_adapter::ApprovalAdapterSpec,
    pub(super) separation_policy: SeparationOfDutiesPolicy,
}

/// 按冻结快照重验审批人账号、动作权限、对象读取、DataScopeFact 与岗位分离。
///
/// # 用途
/// 将审批人资格收敛为 BPM 可消费的 Eligible/Blocked。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `object_read` - 注入的对象读取端口
/// * `input` - 审批人、快照与分离政策
/// * `executor` - 调用方持有的数据库执行器
///
/// # 返回
/// 返回有效或结构化受阻资格。
///
/// # 错误
/// Repository、权限解析、RBAC 或对象读取适配器失败时返回错误。
///
/// # 关键业务约束
/// 任一资格失败必须收敛为 blocker，不得回滚为空。
pub(super) async fn revalidate_decision_approver(
    _db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    input: RevalidateDecisionApproverInput<'_>,
    executor: &mut dyn Executor,
) -> Result<Eligibility> {
    let RevalidateDecisionApproverInput {
        assignee_id,
        assignee_name,
        authenticated_actor,
        snapshot,
        spec,
        separation_policy,
    } = input;
    let account = rbac.load_account(assignee_id, executor).await?;
    let failure = match account {
        None => Some(AuthorizationFailure::AccountInactive),
        Some(account)
            if !account.is_active_backoffice()
                || authenticated_actor
                    .is_some_and(|actor| actor.id() != account.id || actor.kind() != account.kind) =>
        {
            Some(AuthorizationFailure::AccountInactive)
        }
        Some(account) => {
            let scope_actor = authenticated_actor
                .cloned()
                .unwrap_or_else(|| AuditActor::new(account.id.clone(), account.id.clone(), account.kind));
            if OrderTaskSource::approval_kind(snapshot.document_type).is_some()
                && !rbac
                    .order_approval_readable(
                        &scope_actor,
                        snapshot.document_type,
                        &snapshot.business_object_id,
                        executor,
                    )
                    .await?
            {
                return converge_eligibility(
                    assignee_id,
                    assignee_name,
                    Some(AuthorizationFailure::CannotReadSubject),
                );
            }
            let decide_scope = approval_decide_scope_with_executor(rbac, &scope_actor, executor).await?;
            if decide_scope.is_empty() {
                Some(AuthorizationFailure::NotEligible)
            } else if !decide_scope.covers(&snapshot.payload.responsible_org_id) {
                Some(AuthorizationFailure::OutOfDataScope)
            } else {
                let read_scope = approval_document_read_scope_with_executor(
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
                    match runtime_object_readable(spec, &context, assignee_id, true, object_read)? {
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
/// `stock_adjustment:detail` 与当前组织 DataScopeFact 交集；不得回退到已删除的
/// 常量 helper。其它类型仍要求业务 Adapter 显式接线。
pub(super) fn runtime_object_readable(
    spec: &crate::service::approval::business_adapter::ApprovalAdapterSpec,
    context: &BindingRevalidationContext,
    actor_id: &str,
    read_scope_covers: bool,
    object_read: &dyn ApprovalObjectReadPort,
) -> Result<bool> {
    if spec.document_type == DocumentType::StockAdjustment {
        return Ok(read_scope_covers);
    }
    Ok(adapter_object_read_decision_with(spec, context, actor_id, object_read)?.unwrap_or(false))
}

/// 读取必须审批政策唯一签署的岗位分离规则。
pub(super) fn process_required_separation_policy(
    document_type: DocumentType,
) -> Result<SeparationOfDutiesPolicy> {
    match policy_of(document_type)? {
        DocumentApprovalPolicy::ProcessRequired(policy) => Ok(policy.separation_of_duties_policy),
        DocumentApprovalPolicy::NoApproval(_) => {
            Err(Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered))
        }
    }
}

/// 校验当前执行与实例持有的运行令牌完全一致。
pub(super) fn current_execution_matches_instance(
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

/// 普通详情/历史读取允许发起人、当前责任人，或对象读取与 DataScopeFact 同时成立。
pub(super) fn ordinary_runtime_read_allowed(facts: RuntimeReadAuthorizationFacts) -> bool {
    facts.actor_active
        && (facts.initiator || facts.current_responsibility || (facts.object_readable && facts.scope_covers))
}

/// 管理读取必须同时具备类型级运行管理、对象读取与 DataScopeFact。
pub(super) fn management_runtime_read_allowed(facts: RuntimeReadAuthorizationFacts) -> bool {
    facts.actor_active && facts.runtime_admin && facts.object_readable && facts.scope_covers
}

/// Started 视图由 BPM 启动人事实独立证明普通读取权。
pub(super) fn started_runtime_read_allowed(facts: RuntimeReadAuthorizationFacts) -> bool {
    facts.actor_active && facts.initiator
}

/// 当前开放审批任务是否精确证明 actor 对运行实例的当前责任。
pub(super) fn task_proves_current_responsibility(
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
pub(super) fn mine_runtime_chain_matches(
    task: &WorkItem,
    execution: &ApprovalNodeExecution,
    summary: &ApprovalInstanceSummary,
    snapshot: Option<&ApprovalSubjectSnapshot>,
    actor_id: &str,
) -> Result<bool> {
    let document_type =
        crate::entity::approval_integration::document_type_from_subject_kind(summary.subject.subject_kind())
            .map_err(|_| hidden_not_found())?;
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
pub(super) fn mine_execution_ids(tasks: &[WorkItem]) -> Result<Vec<ApprovalNodeExecutionId>> {
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
pub(super) fn ensure_mine_page_integrity(conflict_count: usize) -> Result<()> {
    if conflict_count == 0 {
        return Ok(());
    }
    Err(hidden_not_found())
}

/// 由 Mine 当前页 execution 解析实例 ID，并拒绝两个执行指向同一实例。
///
/// 执行结果缺失或实例重复均表示当前责任链不能形成唯一列表行，整页失败关闭。
pub(super) fn mine_instance_ids(
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
pub(super) fn unique_by_id<T, F>(items: Vec<T>, key_of: F) -> Result<HashMap<String, T>>
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
