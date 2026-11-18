//! 审批实例的真实执行事实与等价查询映射。
use std::collections::BTreeSet;
use std::sync::Arc;

use async_trait::async_trait;
use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelatedObjects, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessFactsPort, AccessMapping, Error, ResourceTargets, Result};
use erp_permissions::{Permission, approval_instance, internal, inventory};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::work_item::WorkItem;
use crate::ports::ApprovalSubjectFactsPort;
use crate::repository::approval_integration::ApprovalSubjectSnapshotRepositoryExt;
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;
use crate::service::approval::execution::runtime_service::{
    RuntimeReadSubject, open_tasks_match, task_proves_current_responsibility,
};
use crate::{process_kind_of, subject_ref_for};

/// 绑定审批实例唯一事实读取器及实例列表查询映射。
/// 供完整授权 Builder 消费的资源事实与固定查询入口。
///
/// 固定模型标识非法时返回接入错误；不读取数据库。
///
/// db 为审批实例和节点执行所在数据库；subjects 为组合根提供的真实业务来源读取端口。
pub fn bindings(
    db: Database,
    subjects: Arc<dyn ApprovalSubjectFactsPort>,
) -> Result<(Arc<dyn AccessFactsPort>, Arc<dyn AccessMapping>)> {
    let facts: Arc<dyn AccessFactsPort> = Arc::new(InstanceFacts { db, subjects });
    let mapping: Arc<dyn AccessMapping> = Arc::new(InstanceMapping {
        model: QueryModel {
            key: QueryModelKey::parse("approval_instances")?,
            resource: "approval_instance".into(),
            collection: Database::APPROVAL_PROCESS_INSTANCES.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner, FactKind::Assignee, FactKind::Participants, FactKind::Completed].into(),
        },
    });
    Ok((facts, mapping))
}

struct InstanceFacts {
    db: Database,
    subjects: Arc<dyn ApprovalSubjectFactsPort>,
}

#[async_trait]
impl AccessFactsPort for InstanceFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        if context.permission == approval_instance::SUBMIT_DECISION {
            return self.decision_tasks(context, ids, executor).await;
        }

        let mut facts = Vec::new();
        for id in ids.ids() {
            let Some(instance) = self.db.approval_process_instances().find_by_id(id, executor).await? else {
                continue;
            };
            if context.permission == internal::APPROVAL_STARTED_LIST {
                facts.push(started_facts(&instance));
                continue;
            }

            let executions = self
                .db
                .approval_node_executions()
                .find_many(doc! {"process_instance_id": id}, executor)
                .await?;
            let mut object = instance_facts(context.actor.id(), &instance, &executions)?;
            let snapshot = self.subject(context, &instance, &mut object, executor).await?;
            if context.permission == internal::APPROVAL_INSTANCE_READ {
                object.assignee_id = FactValue::Known(
                    self.current_responsibility(context, &instance, &executions, snapshot, executor).await?,
                );
            }
            facts.push(object);
        }
        Ok(facts)
    }
}

// 为本人发起的审批进展权限提供真实实例及不可变发起人事实，不把当前执行或来源单据是否还存在作为前提。
fn started_facts(instance: &ApprovalProcessInstance) -> ObjectFacts {
    ObjectFacts {
        resource: "approval_instance".into(),
        id: instance.base.id.clone(),
        version: instance.base.version.to_string(),
        owner_id: FactValue::Known(Some(instance.started_by.as_str().into())),
        ..Default::default()
    }
}

impl InstanceFacts {
    // 精确审批办理取证使用真实工作项的身份，不能用关联审批实例的 ID 代替待办目标。
    async fn decision_tasks(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            if let Some(object) = self.decision_task(context, id, executor).await? {
                objects.push(object);
            }
        }
        Ok(objects)
    }

    // 为单条办理授权取出真实任务、当前执行、冻结主体和该执行的全部开放责任。
    async fn decision_task(
        &self,
        context: &AccessContext,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ObjectFacts>> {
        let Some(task) = self.db.work_items().find_document_approval_by_id(id, executor).await? else {
            return Ok(None);
        };

        let execution_id = task
            .approval_node_execution_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("审批任务缺少精确执行引用".into()))?;
        let execution = self
            .db
            .approval_node_executions()
            .find_by_id(execution_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ValidationError("审批任务执行不存在".into()))?;
        let instance = self
            .db
            .approval_process_instances()
            .find_by_id(execution.process_instance_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ValidationError("审批任务实例不存在".into()))?;

        let mut object = instance_facts(context.actor.id(), &instance, std::slice::from_ref(&execution))?;
        let snapshot = self.subject(context, &instance, &mut object, executor).await?;
        let tasks = self.db.work_items().open_approval_tasks_for_execution(execution_id, executor).await?;

        let subject = RuntimeReadSubject {
            document_type: snapshot.document_type,
            instance,
            snapshot,
            current_execution: Some(execution),
        };
        Ok(Some(decision_task_object(context.actor.id(), &task, &subject, &tasks, object)?))
    }

    async fn subject(
        &self,
        context: &AccessContext,
        instance: &ApprovalProcessInstance,
        object: &mut ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<ApprovalSubjectSnapshot> {
        let snapshot = self
            .db
            .approval_subject_snapshots()
            .find_by_process_instance_id(&instance.base.id, executor)
            .await?
            .ok_or_else(|| Error::ValidationError("缺少提交审批时的单据资料，请联系审批管理员核对".into()))?;

        let expected_subject = subject_ref_for(snapshot.document_type, &snapshot.business_object_id)
            .map_err(|error| Error::ValidationError(error.to_string()))?;
        if expected_subject != instance.subject
            || process_kind_of(snapshot.document_type) != instance.process_kind
            || snapshot.subject_version != instance.subject_version
        {
            return Err(Error::ValidationError("审批实例与冻结业务快照身份或版本不符".into()));
        }

        let document_type = snapshot.document_type.as_str();
        subject_facts(&context.permission, &snapshot, object);
        if context.permission == internal::APPROVAL_INSTANCE_READ {
            return Ok(snapshot);
        }

        let related =
            self.subjects.load(context, document_type, &snapshot.business_object_id, executor).await?;
        object.relations.insert(RelationKey::parse("subject_source")?, related);
        Ok(snapshot)
    }

    // 计算当前办理关系前，将开放任务集合与实例、当前执行和冻结快照闭合；不以某条孤立任务证明责任。
    async fn current_responsibility(
        &self,
        context: &AccessContext,
        instance: &ApprovalProcessInstance,
        executions: &[ApprovalNodeExecution],
        snapshot: ApprovalSubjectSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<Option<String>> {
        let Some(execution) = current_execution(instance, executions)? else { return Ok(None) };
        let tasks = self
            .db
            .work_items()
            .open_approval_tasks_for_execution(&ApprovalNodeExecutionId::new(&execution.base.id), executor)
            .await?;

        let role = adapter_spec_of(snapshot.document_type)
            .map_err(|error| Error::ValidationError(error.to_string()))?
            .owner_role;
        let subject = RuntimeReadSubject {
            instance: instance.clone(),
            current_execution: Some(execution.clone()),
            document_type: snapshot.document_type,
            snapshot,
        };

        Ok((open_tasks_match(execution, &tasks)
            && tasks.iter().any(|task| {
                task_proves_current_responsibility(
                    task,
                    execution,
                    &subject,
                    context.actor.id(),
                    role.as_str(),
                )
            }))
        .then(|| context.actor.id().to_owned()))
    }
}

// 普通单据撤回使用冻结业务提交人判断本人归属；其他实例入口保留真实实例发起人事实。
fn subject_facts(permission: &Permission, snapshot: &ApprovalSubjectSnapshot, object: &mut ObjectFacts) {
    object.subject_type = FactValue::Known(snapshot.document_type.as_str().into());
    if permission == &inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL {
        object.owner_id = FactValue::Known(Some(snapshot.payload.submitted_by.clone()));
    }
}

// 从真实任务取授权对象身份和版本。任务、当前执行或冻结主体不一致时，不提供可成立的办理事实。
fn decision_task_object(
    actor: &str,
    task: &WorkItem,
    subject: &RuntimeReadSubject,
    tasks: &[WorkItem],
    mut object: ObjectFacts,
) -> Result<ObjectFacts> {
    let execution = subject
        .current_execution
        .as_ref()
        .ok_or_else(|| Error::ValidationError("审批任务缺少当前执行".into()))?;
    let role = adapter_spec_of(subject.document_type)
        .map_err(|error| Error::ValidationError(error.to_string()))?
        .owner_role;
    if !open_tasks_match(execution, tasks)
        || !task_proves_current_responsibility(task, execution, subject, actor, role.as_str())
    {
        return Err(Error::Forbidden("当前审批任务责任或冻结身份不一致".into()));
    }

    object.relations.insert(
        RelationKey::parse("decision_instance")?,
        RelatedObjects {
            expected_ids: [subject.instance.base.id.clone()].into(),
            objects: vec![ObjectFacts {
                resource: "approval_instance".into(),
                id: subject.instance.base.id.clone(),
                version: subject.instance.base.version.to_string(),
                subject_type: object.subject_type.clone(),
                ..Default::default()
            }],
        },
    );

    object.id = task.base.id.clone();
    object.version = task.base.version.to_string();
    object.assignee_id = FactValue::Known(task.owner_user_id.clone());
    Ok(object)
}

fn instance_facts(
    actor: &str,
    instance: &ApprovalProcessInstance,
    executions: &[ApprovalNodeExecution],
) -> Result<ObjectFacts> {
    if executions.iter().any(|execution| execution.process_instance_id.as_ref() != instance.base.id) {
        return Err(Error::ValidationError("审批节点执行不属于精确实例".into()));
    }

    let mut participants = BTreeSet::from([instance.started_by.as_str().to_string()]);
    for execution in executions {
        participants.extend(historical_decider(execution).map(str::to_owned));
    }

    let current = current_execution(instance, executions)?;
    let assignee = current.map(|execution| {
        if execution.has_participant(actor) {
            actor.to_string()
        } else {
            execution.assignee_participant_id.as_str().to_string()
        }
    });

    Ok(ObjectFacts {
        resource: "approval_instance".into(),
        id: instance.base.id.clone(),
        version: instance.base.version.to_string(),
        owner_id: FactValue::Known(Some(instance.started_by.as_str().into())),
        assignee_id: FactValue::Known(assignee),
        participants: FactValue::Known(participants),
        completed: FactValue::Known(!matches!(
            instance.status,
            ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked
        )),
        ..Default::default()
    })
}

// 审批历史参与只承认实际完成该决定且属于相同执行冻结参与集合的账号。
fn historical_decider(execution: &ApprovalNodeExecution) -> Option<&str> {
    let actor = execution.decided_by.as_ref()?.as_str();
    (execution.decided_at.is_some()
        && matches!(
            execution.status,
            ApprovalNodeExecutionStatus::Approved | ApprovalNodeExecutionStatus::Rejected
        )
        && execution.has_participant(actor))
    .then_some(actor)
}

fn current_execution<'a>(
    instance: &ApprovalProcessInstance,
    executions: &'a [ApprovalNodeExecution],
) -> Result<Option<&'a ApprovalNodeExecution>> {
    let Some(id) = &instance.current_node_execution_id else {
        return Ok(None);
    };
    let execution = executions
        .iter()
        .find(|execution| execution.base.id == id.as_ref())
        .ok_or_else(|| Error::ValidationError("审批实例当前执行缺失".into()))?;
    let running = matches!(
        instance.status,
        ApprovalProcessInstanceStatus::Running | ApprovalProcessInstanceStatus::Blocked
    );
    Ok((running
        && matches!(
            execution.status,
            ApprovalNodeExecutionStatus::Active | ApprovalNodeExecutionStatus::Blocked
        ))
    .then_some(execution))
}

struct InstanceMapping {
    model: QueryModel,
}

impl AccessMapping for InstanceMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> Result<QueryFragment> {
        Ok(QueryFragment::Exists {
            collection: Database::APPROVAL_PROCESS_INSTANCES.into(),
            bindings: doc! {"instance": "$id"},
            pipeline: vec![
                doc! {"$match": {"deleted_at": 0_i64, "$expr": {"$eq": ["$id", "$$instance"]}}},
                doc! {"$lookup": {
                    "from": Database::APPROVAL_NODE_EXECUTIONS,
                    "let": {"instance": "$id", "execution": "$current_node_execution_id"},
                    "pipeline": [{"$match": {"deleted_at": 0_i64, "$expr": {"$and": [
                        {"$eq": ["$id", "$$execution"]}, {"$eq": ["$process_instance_id", "$$instance"]}
                    ]}}}],
                    "as": "__approval_current",
                }},
                doc! {"$match": {"$or": [{"current_node_execution_id": Bson::Null}, {"__approval_current.0": {"$exists": true}}]}},
            ],
        })
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"started_by": context.actor.id()})),
            AccessFact::AssignedTo => Ok(assigned(context.actor.id())),
            AccessFact::Participant => Ok(participant(context.actor.id())),
            AccessFact::Completed(value) => {
                let operator = if *value { "$nin" } else { "$in" };
                Ok(QueryFragment::Match(doc! {"status": {operator: ["RUNNING", "BLOCKED"]}}))
            },
            _ => Err(Error::ValidationError("审批实例映射不支持该事实".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> Result<RelationMapping> {
        Err(Error::ValidationError("审批实例没有来源映射".into()))
    }
}

fn assigned(actor: &str) -> QueryFragment {
    QueryFragment::Exists {
        collection: Database::APPROVAL_NODE_EXECUTIONS.into(),
        bindings: doc! {"instance": "$id", "execution": "$current_node_execution_id", "instance_status": "$status"},
        pipeline: vec![doc! {"$match": {
            "deleted_at": 0_i64,
            "status": {"$in": ["ACTIVE", "BLOCKED"]},
            "$expr": {"$and": [
                {"$eq": ["$id", "$$execution"]},
                {"$eq": ["$process_instance_id", "$$instance"]},
                {"$in": ["$$instance_status", ["RUNNING", "BLOCKED"]]},
            ]},
            "$or": [{"assignee_participant_id": actor}, {"additional_assignees.participant_id": actor}],
        }}],
    }
}

fn participant(actor: &str) -> QueryFragment {
    QueryFragment::Exists {
        collection: Database::APPROVAL_PROCESS_INSTANCES.into(),
        bindings: doc! {"instance": "$id"},
        pipeline: vec![
            doc! {"$match": {"deleted_at": 0_i64, "$expr": {"$eq": ["$id", "$$instance"]}}},
            doc! {"$lookup": {
                "from": Database::APPROVAL_NODE_EXECUTIONS,
                "let": {"instance": "$id"},
                "pipeline": [{"$match": participant_execution_filter(actor)}],
                "as": "__approval_participation",
            }},
            doc! {"$match": {"$or": [{"started_by": actor}, {"__approval_participation.0": {"$exists": true}}]}},
        ],
    }
}

fn participant_execution_filter(actor: &str) -> Document {
    doc! {
        "deleted_at": 0_i64,
        "status": {"$in": ["APPROVED", "REJECTED"]},
        "decided_by": actor,
        "decided_at": {"$ne": Bson::Null},
        "$expr": {"$eq": ["$process_instance_id", "$$instance"]},
        "$or": [
            {"assignee_participant_id": actor},
            {"additional_assignees.participant_id": actor},
        ],
    }
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        ApprovalParticipant, NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef,
        Timestamp,
    };
    use erp_core::common::time::Instant;
    use erp_core::ids::{ApprovalSubjectSnapshotId, WorkItemId};

    use super::*;
    use crate::entity::approval_integration::ApprovalSubjectSnapshotPayload;
    use crate::entity::document_registry::DocumentType;
    use crate::entity::work_item::{DocumentApprovalWorkItemData, WorkItemPriority};

    fn fixture() -> (ApprovalProcessInstance, ApprovalNodeExecution) {
        let at = Timestamp::from_unix_secs(1).unwrap();
        let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "business-id").unwrap(),
            subject_version: 9,
            started_by: ParticipantId::new("starter").unwrap(),
            at,
        })
        .unwrap();
        let execution = ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("execution"),
            process_instance_id: instance.typed_id(),
            node_key: "node".into(),
            node_name: "审批".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("primary").unwrap(),
            assignee_name_snapshot: "主审批人".into(),
            at,
        })
        .unwrap()
        .with_additional_assignees(vec![ApprovalParticipant {
            participant_id: ParticipantId::new("additional").unwrap(),
            name: "或签人".into(),
        }])
        .unwrap();
        instance.current_node_execution_id = Some(ApprovalNodeExecutionId::new(&execution.base.id));
        instance.base.version = 7;
        (instance, execution)
    }

    #[test]
    fn current_alternative_assignee_uses_exact_instance_identity_and_version() {
        let (instance, execution) = fixture();
        let facts = instance_facts("additional", &instance, &[execution]).unwrap();
        assert_eq!(facts.id, "instance");
        assert_eq!(facts.version, "7");
        assert_eq!(facts.assignee_id, FactValue::Known(Some("additional".into())));
        assert_eq!(facts.participants, FactValue::Known(["starter".into()].into()));
        assert_eq!(facts.completed, FactValue::Known(false));
    }

    #[test]
    fn finished_execution_retains_participation_and_stops_current_assignment() {
        let (instance, mut execution) = fixture();
        execution
            .record_approve(
                ParticipantId::new("additional").unwrap(),
                None,
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        let facts = instance_facts("additional", &instance, &[execution]).unwrap();
        assert_eq!(facts.assignee_id, FactValue::Known(None));
        assert!(matches!(facts.participants, FactValue::Known(ids) if ids.contains("additional")));
    }

    #[test]
    fn unused_alternative_and_replaced_assignees_do_not_become_historical_participants() {
        let (instance, mut execution) = fixture();
        execution
            .record_approve(
                ParticipantId::new("additional").unwrap(),
                None,
                Timestamp::from_unix_secs(2).unwrap(),
            )
            .unwrap();
        let facts = instance_facts("primary", &instance, &[execution.clone()]).unwrap();
        assert_eq!(facts.assignee_id, FactValue::Known(None));
        assert_eq!(facts.participants, FactValue::Known(["starter".into(), "additional".into()].into()));

        execution.decided_at = None;
        assert_eq!(historical_decider(&execution), None);
        execution.decided_at = Some(Timestamp::from_unix_secs(2).unwrap());
        execution.decided_by = Some(ParticipantId::new("outsider").unwrap());
        assert_eq!(historical_decider(&execution), None);
    }

    #[test]
    fn participant_query_requires_a_completed_decision_by_the_actual_actor() {
        let filter = participant_execution_filter("actor");
        assert_eq!(filter.get_str("decided_by").unwrap(), "actor");
        assert_eq!(filter.get_document("decided_at").unwrap(), &doc! {"$ne": Bson::Null});
        assert_eq!(filter.get_document("status").unwrap(), &doc! {"$in": ["APPROVED", "REJECTED"]});
        assert_eq!(
            filter.get_array("$or").unwrap(),
            &vec![
                Bson::Document(doc! {"assignee_participant_id": "actor"}),
                Bson::Document(doc! {"additional_assignees.participant_id": "actor"}),
            ]
        );
    }

    #[test]
    fn terminal_instance_does_not_keep_current_assignee() {
        let (mut instance, execution) = fixture();
        instance.status = ApprovalProcessInstanceStatus::Approved;
        let facts = instance_facts("primary", &instance, &[execution]).unwrap();
        assert_eq!(facts.assignee_id, FactValue::Known(None));
        assert_eq!(facts.completed, FactValue::Known(true));
    }

    #[test]
    fn started_progress_facts_use_only_the_actual_instance_owner_and_version() {
        let (instance, _) = fixture();
        let facts = started_facts(&instance);
        assert_eq!(facts.id, instance.base.id);
        assert_eq!(facts.version, instance.base.version.to_string());
        assert_eq!(facts.owner_id, FactValue::Known(Some("starter".into())));
        assert!(facts.relations.is_empty());
        assert_eq!(facts.assignee_id, FactValue::Missing);
    }

    #[test]
    fn cancellation_owner_uses_frozen_submitter_while_other_reads_keep_instance_starter() {
        let (instance, execution) = fixture();
        let snapshot = ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snapshot"),
            instance.typed_id(),
            DocumentType::StockAdjustment,
            "business-id",
            9,
            ApprovalSubjectSnapshotPayload {
                document_no: "ADJ-1".into(),
                responsible_org_id: "org".into(),
                submitted_by: "submitter".into(),
                submitted_at: Instant::from_unix_secs(1),
                counterparty: None,
                total_amount: None,
                total_quantity: Some("1".parse().unwrap()),
                line_count: 1,
            },
        )
        .unwrap();
        for (permission, owner) in [
            (inventory::STOCK_ADJUSTMENT_CANCEL_APPROVAL, "submitter"),
            (internal::APPROVAL_INSTANCE_READ, "starter"),
            (internal::APPROVAL_RECOVER, "starter"),
        ] {
            let mut object = instance_facts("primary", &instance, std::slice::from_ref(&execution)).unwrap();
            subject_facts(&permission, &snapshot, &mut object);
            assert_eq!(object.owner_id, FactValue::Known(Some(owner.into())));
            assert_eq!(object.subject_type, FactValue::Known("stock_adjustment".into()));
            assert_eq!(object.id, instance.base.id);
            assert_eq!(object.version, instance.base.version.to_string());
        }
    }

    #[test]
    fn missing_or_foreign_current_execution_is_rejected() {
        let (instance, mut execution) = fixture();
        assert!(matches!(instance_facts("primary", &instance, &[]), Err(Error::ValidationError(_))));
        execution.process_instance_id = ApprovalProcessInstanceId::new("foreign");
        assert!(matches!(instance_facts("primary", &instance, &[execution]), Err(Error::ValidationError(_))));
    }

    #[test]
    fn assignment_query_binds_current_execution_and_instance_without_changing_root_cardinality() {
        let QueryFragment::Exists { collection, bindings, pipeline } = assigned("additional") else {
            panic!("必须沿真实执行检查指派");
        };
        assert_eq!(collection, Database::APPROVAL_NODE_EXECUTIONS);
        assert_eq!(
            bindings,
            doc! {"instance": "$id", "execution": "$current_node_execution_id", "instance_status": "$status"}
        );
        assert_eq!(
            pipeline[0].get_document("$match").unwrap().get_array("$or").unwrap(),
            &vec![
                Bson::Document(doc! {"assignee_participant_id": "additional"}),
                Bson::Document(doc! {"additional_assignees.participant_id": "additional"}),
            ]
        );
    }

    #[test]
    fn decision_facts_keep_exact_task_identity_and_reject_a_different_execution_or_owner() {
        let (instance, execution) = fixture();
        let snapshot = ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snapshot"),
            instance.typed_id(),
            DocumentType::StockAdjustment,
            "business-id",
            9,
            ApprovalSubjectSnapshotPayload {
                document_no: "ADJ-1".into(),
                responsible_org_id: "org".into(),
                submitted_by: "starter".into(),
                submitted_at: Instant::from_unix_secs(1),
                counterparty: None,
                total_amount: None,
                total_quantity: Some("1".parse().unwrap()),
                line_count: 1,
            },
        )
        .unwrap();
        let mut task = WorkItem::new_document_approval(
            WorkItemId::new("real-task"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new(&execution.base.id),
                business_object_type: "stock_adjustment".into(),
                business_object_id: "business-id".into(),
                subject_version: "9".into(),
                owner_role: "stock_adjustment_approver".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "additional".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(1),
        )
        .unwrap();
        task.base.version = 13;
        let raw = instance_facts("additional", &instance, std::slice::from_ref(&execution)).unwrap();
        let subject = RuntimeReadSubject {
            instance,
            current_execution: Some(execution),
            snapshot,
            document_type: DocumentType::StockAdjustment,
        };

        let facts =
            decision_task_object("additional", &task, &subject, std::slice::from_ref(&task), raw.clone())
                .unwrap();
        assert_eq!(facts.id, "real-task");
        assert_eq!(facts.version, "13");
        assert_eq!(facts.assignee_id, FactValue::Known(Some("additional".into())));

        assert!(matches!(
            decision_task_object("primary", &task, &subject, std::slice::from_ref(&task), raw.clone()),
            Err(Error::Forbidden(_))
        ));

        task.approval_node_execution_id = Some(ApprovalNodeExecutionId::new("other-execution"));
        assert!(matches!(
            decision_task_object("additional", &task, &subject, std::slice::from_ref(&task), raw),
            Err(Error::Forbidden(_))
        ));
    }
}
