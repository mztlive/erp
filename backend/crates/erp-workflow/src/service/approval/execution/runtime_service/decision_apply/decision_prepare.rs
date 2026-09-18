//! 决定 Fresh 路径的加载、资格重验与写入计划。

use application_core::AuditActor;
use bpm::engine::{CommitRequired, DefinitionGraph, Eligibility};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{
    ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp,
};
use erp_core::common::time::Instant;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::super::super::apply_plan::PlannedWrites;
use super::super::super::decision::prepare_decision;
use super::super::super::{DecisionExecutionInput, ExecutionCommandInput, PreparedExecution};
use super::super::hidden_not_found;
use super::super::notifications::runtime_admin_notification_recipients;
use super::super::query::list_projection_from_writes;
use super::super::read_auth::{
    RevalidateDecisionApproverInput, RuntimeReadSubject, process_required_separation_policy,
    revalidate_decision_approver, task_proves_current_responsibility,
};
use super::{RuntimeDecisionCommand, map_approval_task_error, map_runtime_graph_error};
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::WorkItem;
use crate::error::{Error, ErrorCode, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::bpm::ApprovalInstanceListProjection;
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::{ApprovalAdapterSpec, adapter_spec_of};
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::{ApprovalActionContext, DecisionActionParams};

pub(super) struct OpenDecisionRuntime {
    pub(super) item: WorkItem,
    pub(super) execution: ApprovalNodeExecution,
    pub(super) execution_id: ApprovalNodeExecutionId,
    pub(super) instance: ApprovalProcessInstance,
    pub(super) instance_id: ApprovalProcessInstanceId,
    pub(super) expected_instance_version: u64,
    pub(super) expected_execution_version: u64,
    pub(super) snapshot: ApprovalSubjectSnapshot,
    pub(super) document_type: DocumentType,
    pub(super) spec: ApprovalAdapterSpec,
    pub(super) open_tasks: Vec<WorkItem>,
    pub(super) instance_assignee: ApprovalInstanceAssignee,
}

pub(super) struct PreparedOpenDecision {
    pub(super) writes: PlannedWrites,
    pub(super) should_finalize: bool,
    pub(super) action_context: ApprovalActionContext,
    pub(super) new_task_ids: Vec<String>,
    pub(super) list_projection: ApprovalInstanceListProjection,
    pub(super) audit: PreparedWorkflowAudit,
    pub(super) now: Instant,
    pub(super) actor_id: String,
    pub(super) owner_role: String,
    pub(super) owner_organization_id: String,
    pub(super) subject_version: String,
    pub(super) business_object_id: String,
    pub(super) document_type_label: String,
    pub(super) runtime_admin_ids: Vec<String>,
}

pub(super) async fn load_open_decision_runtime(
    db: &Database,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    executor: &mut dyn Executor,
) -> Result<OpenDecisionRuntime> {
    let (item, execution_id, execution, expected_execution_version) =
        load_open_decision_execution(db, actor, command, executor).await?;
    load_open_decision_subject(
        db,
        actor,
        command,
        item,
        execution_id,
        execution,
        expected_execution_version,
        executor,
    )
    .await
}

async fn load_open_decision_execution(
    db: &Database,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    executor: &mut dyn Executor,
) -> Result<(WorkItem, ApprovalNodeExecutionId, ApprovalNodeExecution, u64)> {
    let item = db
        .work_items()
        .find_document_approval_by_id(&command.work_item_id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let execution_id = item
        .approval_execution_for_decision(actor.id(), command.expected_task_version)
        .map_err(map_approval_task_error)?
        .clone();
    let execution = db
        .bpm_workflow()
        .find_execution_by_id(&execution_id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let expected_execution_version = execution.base.version;
    Ok((item, execution_id, execution, expected_execution_version))
}

async fn load_open_decision_subject(
    db: &Database,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    item: WorkItem,
    execution_id: ApprovalNodeExecutionId,
    execution: ApprovalNodeExecution,
    expected_execution_version: u64,
    executor: &mut dyn Executor,
) -> Result<OpenDecisionRuntime> {
    let (instance_id, instance, expected_instance_version, document_type, snapshot, spec) =
        load_open_decision_instance(db, &execution, executor).await?;
    let (open_tasks, instance_assignee) = load_open_decision_tasks(
        db,
        actor,
        command,
        &item,
        &execution,
        &execution_id,
        &instance_id,
        &instance,
        &snapshot,
        document_type,
        &spec,
        executor,
    )
    .await?;
    Ok(OpenDecisionRuntime {
        item,
        execution,
        execution_id,
        instance,
        instance_id,
        expected_instance_version,
        expected_execution_version,
        snapshot,
        document_type,
        spec,
        open_tasks,
        instance_assignee,
    })
}

async fn load_open_decision_instance(
    db: &Database,
    execution: &ApprovalNodeExecution,
    executor: &mut dyn Executor,
) -> Result<(
    ApprovalProcessInstanceId,
    ApprovalProcessInstance,
    u64,
    DocumentType,
    ApprovalSubjectSnapshot,
    ApprovalAdapterSpec,
)> {
    let instance_id = execution.process_instance_id.clone();
    let instance =
        db.bpm_workflow().find_instance_by_id(&instance_id, executor).await?.ok_or_else(hidden_not_found)?;
    let expected_instance_version = instance.base.version;
    let document_type =
        crate::entity::approval_integration::document_type_from_subject_kind(instance.subject.subject_kind())
            .map_err(|error| Error::ValidationError(error.to_string()))?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(Error::ConflictError("审批实例流程种类与单据类型不一致".to_string()));
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(instance_id.as_ref(), executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| Error::ConflictError("审批实例与冻结业务快照不一致".to_string()))?;
    Ok((
        instance_id,
        instance,
        expected_instance_version,
        document_type,
        snapshot,
        adapter_spec_of(document_type)?,
    ))
}

async fn load_open_decision_tasks(
    db: &Database,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    item: &WorkItem,
    execution: &ApprovalNodeExecution,
    execution_id: &ApprovalNodeExecutionId,
    instance_id: &ApprovalProcessInstanceId,
    instance: &ApprovalProcessInstance,
    snapshot: &ApprovalSubjectSnapshot,
    document_type: DocumentType,
    spec: &ApprovalAdapterSpec,
    executor: &mut dyn Executor,
) -> Result<(Vec<WorkItem>, ApprovalInstanceAssignee)> {
    let subject = RuntimeReadSubject {
        instance: instance.clone(),
        current_execution: Some(execution.clone()),
        snapshot: snapshot.clone(),
        document_type,
    };
    if !task_proves_current_responsibility(item, execution, &subject, actor.id(), spec.owner_role.as_str()) {
        return Err(Error::ConflictError("APPROVAL_RESPONSIBILITY_CONFLICT".to_string()));
    }
    let open_tasks = db.work_items().open_approval_tasks_for_execution(execution_id, executor).await?;
    if open_tasks.is_empty() || !open_tasks.iter().any(|task| task.base.id == command.work_item_id) {
        return Err(Error::from_approval_code(ErrorCode::ApprovalTaskNotOpen));
    }
    let instance_assignee = db
        .bpm_workflow()
        .find_assignee_for_node(instance_id, &execution.node_key, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("实例缺少节点审批人绑定".to_string()))?;
    Ok((open_tasks, instance_assignee))
}

pub(super) async fn prepare_open_decision(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn crate::ports::ApprovalObjectReadPort,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    loaded: &OpenDecisionRuntime,
    executor: &mut dyn Executor,
) -> Result<PreparedOpenDecision> {
    let now = Instant::now();
    let (graph, current_eligibility, next_eligibility) =
        revalidate_decision_eligibilities(db, rbac, object_read, actor, command, loaded, executor).await?;
    let writes =
        plan_open_decision(actor, command, loaded, graph, current_eligibility, next_eligibility, now)?;
    finish_prepared_open_decision(db, rbac, object_read, actor, command, loaded, writes, now, executor).await
}

async fn revalidate_decision_eligibilities(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn crate::ports::ApprovalObjectReadPort,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    loaded: &OpenDecisionRuntime,
    executor: &mut dyn Executor,
) -> Result<(DefinitionGraph, Eligibility, Eligibility)> {
    let separation_policy = process_required_separation_policy(loaded.document_type)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&loaded.instance.process_definition_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例绑定的定义不存在".to_string()))?;
    let current_eligibility = revalidate_decision_approver(
        db,
        rbac,
        object_read,
        RevalidateDecisionApproverInput {
            assignee_id: actor.id(),
            assignee_name: &loaded.execution.assignee_name_snapshot,
            authenticated_actor: Some(actor),
            snapshot: &loaded.snapshot,
            spec: &loaded.spec,
            separation_policy,
        },
        executor,
    )
    .await?;
    let decision_target = graph
        .decision_target_node_key(&loaded.execution.node_key, command.decision)
        .map_err(map_runtime_graph_error)?;
    let next_eligibility = match decision_target {
        Some(node_key) => match graph.node(&node_key) {
            Some(node) => {
                revalidate_decision_approver(
                    db,
                    rbac,
                    object_read,
                    RevalidateDecisionApproverInput {
                        assignee_id: node.assignee_participant_id.as_str(),
                        assignee_name: &node.assignee_label_snapshot,
                        authenticated_actor: None,
                        snapshot: &loaded.snapshot,
                        spec: &loaded.spec,
                        separation_policy,
                    },
                    executor,
                )
                .await?
            },
            None => return Err(Error::ConflictError("审批定义缺少目标节点".to_string())),
        },
        None => current_eligibility.clone(),
    };
    Ok((graph, current_eligibility, next_eligibility))
}

fn plan_open_decision(
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    loaded: &OpenDecisionRuntime,
    graph: DefinitionGraph,
    current_eligibility: Eligibility,
    next_eligibility: Eligibility,
    now: Instant,
) -> Result<PlannedWrites> {
    let prepared = prepare_decision(DecisionExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility,
            next_eligibility,
            receipt: None,
            idempotency_key: command.idempotency_key.clone(),
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance: loaded.instance.clone(),
        current: loaded.execution.clone(),
        work_item_id: command.work_item_id.clone(),
        task_owner_id: loaded.item.owner_user_id.clone().unwrap_or_default(),
        instance_assignee_id: loaded.instance_assignee.current_assignee_participant_id.as_str().to_string(),
        decision: command.decision,
        reason: command.reason.clone(),
        expected_task_version: command.expected_task_version,
        actor: ParticipantId::new(actor.id())
            .map_err(|_| Error::ValidationError("决定人引用无效".to_string()))?,
        next_execution_id: ApprovalNodeExecutionId::new(next_id()),
        next_execution_no: loaded
            .execution
            .execution_no
            .checked_add(1)
            .ok_or_else(|| Error::ConflictError("审批执行序号溢出".to_string()))?,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        open_task_count: loaded.open_tasks.len(),
    })?;
    let PreparedExecution::Apply(writes) = prepared else {
        return Err(Error::Internal("新决定命令不得进入幂等回放分支".to_string()));
    };
    Ok(*writes)
}

async fn finish_prepared_open_decision(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn crate::ports::ApprovalObjectReadPort,
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    loaded: &OpenDecisionRuntime,
    writes: PlannedWrites,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<PreparedOpenDecision> {
    let actor_id = actor.id().to_string();
    let owner_role = loaded.spec.owner_role.as_str().to_string();
    let owner_organization_id = loaded.snapshot.payload.responsible_org_id.clone();
    let subject_version = writes.instance.subject_version.to_string();
    let business_object_id = writes.instance.subject.subject_id().to_string();
    let document_type_label = loaded.document_type.label().to_string();
    let runtime_admin_ids =
        blocked_runtime_admin_ids(db, rbac, object_read, loaded, &writes, executor).await?;
    let should_finalize = writes.commit == CommitRequired::TerminalApproved;
    let action_context =
        decision_action_context(command, loaded, &actor_id, &business_object_id, &subject_version)?;
    let new_task_ids: Vec<String> = writes.create_tasks.iter().map(|_| next_id()).collect();
    let list_projection =
        list_projection_from_writes(&writes, loaded.execution_id.as_ref(), command.reason.clone(), now);
    let audit = decision_audit(actor, command, &loaded.instance_id)?;
    Ok(PreparedOpenDecision {
        writes,
        should_finalize,
        action_context,
        new_task_ids,
        list_projection,
        audit,
        now,
        actor_id,
        owner_role,
        owner_organization_id,
        subject_version,
        business_object_id,
        document_type_label,
        runtime_admin_ids,
    })
}

async fn blocked_runtime_admin_ids(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn crate::ports::ApprovalObjectReadPort,
    loaded: &OpenDecisionRuntime,
    writes: &PlannedWrites,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    if writes.notifications.iter().any(|intent| {
        intent.event_kind == crate::entity::approval_integration::ApprovalNotificationEventKind::Blocked
    }) {
        runtime_admin_notification_recipients(
            db,
            rbac,
            object_read,
            loaded.document_type,
            &loaded.snapshot,
            executor,
        )
        .await
    } else {
        Ok(Vec::new())
    }
}

fn decision_action_context(
    command: &RuntimeDecisionCommand,
    loaded: &OpenDecisionRuntime,
    actor_id: &str,
    business_object_id: &str,
    subject_version: &str,
) -> Result<ApprovalActionContext> {
    ApprovalActionContext::for_decision(DecisionActionParams {
        approval_process_instance_id: loaded.instance_id.to_string(),
        approval_node_execution_id: loaded.execution_id.to_string(),
        work_item_id: command.work_item_id.clone(),
        business_object_type: loaded.document_type.as_str().to_string(),
        business_object_id: business_object_id.to_string(),
        subject_version: subject_version.to_string(),
        actor_id: actor_id.to_string(),
        reason: command.reason.clone(),
        idempotency_key: command.idempotency_key.as_str().to_string(),
    })
}

fn decision_audit(
    actor: &AuditActor,
    command: &RuntimeDecisionCommand,
    instance_id: &ApprovalProcessInstanceId,
) -> Result<PreparedWorkflowAudit> {
    PreparedWorkflowAudit::resource_with_message(
        actor.clone(),
        "approval.decide",
        "approval_process_instance",
        instance_id.to_string(),
        Some(format!(
            "decision={} reason={:?} work_item={}",
            command.decision.as_str(),
            command.reason,
            command.work_item_id
        )),
    )
}
