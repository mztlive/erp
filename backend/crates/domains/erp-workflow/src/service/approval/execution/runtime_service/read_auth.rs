//! 审批资格与当前责任链判定。

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use bpm::engine::Eligibility;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_core::AccountKind;
use erp_identity::{AuthorizationService, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;

use super::super::authorization::{AuthorizationFailure, converge_eligibility};
use super::hidden_not_found;
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::approval_qualification::ApprovalQualification;
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::{AssignmentSource, WorkItem, WorkItemStatus, WorkItemType};
use crate::error::{Error, ErrorCode, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::bpm::ApprovalInstanceSummary;
use crate::service::approval::business_adapter::{adapter_spec_of, ensure_separation_of_duties};
use crate::service::approval::policy::{DocumentApprovalPolicy, SeparationOfDutiesPolicy, policy_of};
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::scope::approver_qualification;

/// 单实例读取授权所需的持久化事实。
pub(crate) struct RuntimeReadSubject {
    pub(crate) instance: ApprovalProcessInstance,
    pub(crate) current_execution: Option<ApprovalNodeExecution>,
    pub(crate) snapshot: ApprovalSubjectSnapshot,
    pub(crate) document_type: DocumentType,
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

// 执行审批决定前，以冻结提交快照和原执行器重验原审批人的有效账号、同角色静态权限及岗位分离。
//
// 资格丢失返回 BPM 可消费的结构化 blocker，不转为空值；快照类型不一致或取证故障仍返回错误。
pub(super) async fn revalidate_decision_approver(
    authorization: &AuthorizationService,
    db: &Database,
    identity_service: &SharedRbacService,
    _rbac: &impl WorkflowAuthorizationPort,
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
    if spec.document_type != snapshot.document_type {
        return Err(hidden_not_found());
    }

    let actor = authenticated_actor
        .cloned()
        .unwrap_or_else(|| AuditActor::new(assignee_id.into(), assignee_id.into(), AccountKind::Admin));
    let qualified = if actor.id() == assignee_id {
        approver_qualification(authorization, db, identity_service, &actor, executor).await?
    } else {
        ApprovalQualification::Inactive
    };

    let failure = match qualified {
        ApprovalQualification::Inactive => Some(AuthorizationFailure::AccountInactive),
        ApprovalQualification::MissingPermissions => Some(AuthorizationFailure::NotEligible),
        ApprovalQualification::Eligible(_)
            if ensure_separation_of_duties(
                separation_policy,
                &snapshot.payload.submitted_by,
                &[assignee_id.to_string()],
            )
            .is_err() =>
        {
            Some(AuthorizationFailure::SeparationOfDuties)
        },
        ApprovalQualification::Eligible(_) => None,
    };
    converge_eligibility(assignee_id, assignee_name, failure)
}

// 决定或恢复前读取该单据固定必须审批政策的岗位分离规则；政策未登记或为无审批类型时拒绝。
pub(super) fn process_required_separation_policy(
    document_type: DocumentType,
) -> Result<SeparationOfDutiesPolicy> {
    match policy_of(document_type)? {
        DocumentApprovalPolicy::ProcessRequired(policy) => Ok(policy.separation_of_duties_policy),
        DocumentApprovalPolicy::NoApproval(_) => {
            Err(Error::from_approval_code(ErrorCode::ApprovalPolicyNotRegistered))
        },
    }
}

// 核对实例当前执行令牌及执行的实例归属。仅两边都无当前执行，或执行 ID 与所属实例均精确匹配时成立。
pub(super) fn current_execution_matches_instance(
    instance: &ApprovalProcessInstance,
    current: Option<&ApprovalNodeExecution>,
) -> bool {
    match (&instance.current_node_execution_id, current) {
        (None, None) => true,
        (Some(expected), Some(execution)) => {
            expected.as_ref() == execution.base.id
                && execution.process_instance_id.as_ref() == instance.base.id
        },
        _ => false,
    }
}

// 办理审批前证明开放任务是当前操作人在此实例和冻结提交版本上的现行责任。任务、运行实例、开放执行、轮次、参与人、责任岗位、组织和对象身份均须一致。
pub(crate) fn task_proves_current_responsibility(
    task: &WorkItem,
    execution: &ApprovalNodeExecution,
    subject: &RuntimeReadSubject,
    actor_id: &str,
    expected_owner_role: &str,
) -> bool {
    subject.snapshot.approval_process_instance_id.as_ref() == subject.instance.base.id
        && subject
            .snapshot
            .ensure_matches_runtime_subject(
                subject.document_type,
                subject.instance.subject.subject_id(),
                subject.instance.subject_version,
            )
            .is_ok()
        && subject.instance.process_kind == process_kind_of(subject.document_type)
        && subject.instance.status == ApprovalProcessInstanceStatus::Running
        && execution.status == ApprovalNodeExecutionStatus::Active
        && execution.round_no == subject.instance.current_round_no
        && execution.has_participant(actor_id)
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

// Mine 页的 WorkItem、当前 execution 与实例摘要是否构成同一当前责任链。
//
// 运行链与快照责任组织都一致时返回 `Ok(true)`；任一事实不一致时返回 `Ok(false)`。
//
// 主体种类无法解析，或单据适配规格不存在时隐藏为不存在或返回适配错误。
//
// `task`：当前页任务；`snapshot`：冻结快照；缺失时责任链不成立。
// `actor_id`：当前读者。
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
        && execution.has_participant(actor_id)
        && summary.current_node_key.as_deref() == Some(execution.node_key.as_str())
        && summary.current_node_name.as_deref() == Some(execution.node_name.as_str())
        && summary.current_assignee_participant_id.as_deref()
            == Some(execution.assignee_participant_id.as_str())
        && summary.current_assignee_name.as_deref() == Some(execution.assignee_display_name().as_str());
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
        .is_some_and(|snapshot| snapshot.payload.responsible_org_id == task.owner_organization_id);
    Ok(snapshot_owner_matches)
}

// 提取 Mine 当前页的 execution ID，并在服务边界拒绝重复责任投影。
//
// 不得依赖唯一索引或静默去重；重复 WorkItem 会同时污染当前页行与
// Repository 返回的 total，因此必须按隐藏实例存在性的稳定语义整页失败。
//
// 返回与任务顺序一致且互不重复的执行 ID。
//
// 任务没有执行引用或执行 ID 重复时隐藏为不存在。
//
// `tasks`：当前页任务，顺序保留。
pub(super) fn mine_execution_ids(tasks: &[WorkItem]) -> Result<Vec<ApprovalNodeExecutionId>> {
    let mut seen = HashSet::with_capacity(tasks.len());
    let mut execution_ids = Vec::with_capacity(tasks.len());
    for task in tasks {
        let execution_id = task.approval_node_execution_id.clone().ok_or_else(hidden_not_found)?;
        if !seen.insert(execution_id.to_string()) {
            return Err(hidden_not_found());
        }
        execution_ids.push(execution_id);
    }
    Ok(execution_ids)
}

// 把 Repository 在完整过滤集合中发现的责任链冲突映射为隐藏式拒绝。
// 没有冲突时无返回值。
//
// 冲突数大于 0 时隐藏为不存在。
//
// `conflict_count`：仓储报告的责任链冲突数。
pub(super) fn ensure_mine_page_integrity(conflict_count: usize) -> Result<()> {
    if conflict_count == 0 {
        return Ok(());
    }
    Err(hidden_not_found())
}

// 由 Mine 当前页 execution 解析实例 ID，并拒绝两个执行指向同一实例。
//
// 执行结果缺失或实例重复均表示当前责任链不能形成唯一列表行，整页失败关闭。
//
// 返回与执行顺序一致且互不重复的实例 ID。
//
// 执行缺失或两个执行指向同一实例时隐藏为不存在。
//
// `execution_ids`：当前页执行 ID；`execution_by_id`：已按执行 ID 建表的执行。
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

// 将批量读取结果按稳定 ID 建表；重复 ID 按持久化身份损坏失败关闭。
// 返回以稳定 ID 为键的表。
//
// 同一 ID 出现多次时隐藏为不存在。
//
// `key_of`：从单项取出稳定 ID。
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

// 校验同节点开放任务的人员归属及每人唯一性。
// 非空、人数不超出冻结集合且每人恰有一项时返回 true。
//
// `execution` 为冻结节点执行，`tasks` 为该执行全部开放任务。
pub(crate) fn open_tasks_match(execution: &ApprovalNodeExecution, tasks: &[WorkItem]) -> bool {
    let mut owners = HashSet::new();
    !tasks.is_empty()
        && tasks.len() <= 1 + execution.additional_assignees.len()
        && tasks.iter().all(|task| {
            task.status == WorkItemStatus::Open
                && task.work_item_type == WorkItemType::DocumentApproval
                && task.assignment_source == AssignmentSource::ApprovalRuntime
                && task.approval_node_execution_id.as_ref().is_some_and(|id| id.as_ref() == execution.base.id)
                && task
                    .owner_user_id
                    .as_deref()
                    .is_some_and(|owner| execution.has_participant(owner) && owners.insert(owner))
        })
}
