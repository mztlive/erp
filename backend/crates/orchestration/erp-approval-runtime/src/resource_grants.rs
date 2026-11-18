//! 正式审批节点任务与读取授权事件保持同一提交边界。
use bpm::engine::TaskIntent;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_identity::entity::resource_access::event::{GrantEventEvidence, GrantEventPayload};
use erp_identity::service::resource_access::append_event;
use erp_workflow::service::approval::execution::PlannedWrites;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 为实际创建的销售、采购审批任务追加读取授权事件。
/// # 参数
/// db 为数据库；writes 为正式计划；executor 为保存节点与任务的原事务。
/// # 返回
/// 全部对应事件写入成功。
/// # 错误
/// 缺失节点证据、类型关联错误或事件写入失败时回滚原事务。
pub async fn append_assignments(
    db: &Database,
    writes: &PlannedWrites,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !matches!(writes.instance.subject.subject_kind(), "sales_order" | "purchase_order") {
        return Ok(());
    }
    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested { execution_id, assignee, .. } = intent else {
            continue;
        };
        let execution = writes
            .created_executions
            .iter()
            .find(|e| e.base.id == execution_id.as_ref())
            .ok_or_else(|| Error::Internal("正式分配缺少节点执行证据".into()))?;
        append_event(db, assignment_evidence(&writes.instance, execution, assignee.as_str())?, executor)
            .await?;
    }

    Ok(())
}

/// 将已创建的节点与实际任务接收人映射为稳定事件，不接受错配执行。
fn assignment_evidence(
    instance: &ApprovalProcessInstance,
    execution: &ApprovalNodeExecution,
    recipient: &str,
) -> Result<GrantEventEvidence> {
    if execution.process_instance_id.as_ref() != instance.base.id || !execution.has_participant(recipient) {
        return Err(Error::ConflictError("审批授权事件与正式分配不一致".into()));
    }
    let payload = match instance.subject.subject_kind() {
        "sales_order" => GrantEventPayload::SalesOrderApprovalAssigned {
            sales_order_id: instance.subject.subject_id().into(),
            approval_instance_id: instance.base.id.clone(),
            node_id: execution.node_key.clone(),
            assignment_id: execution.base.id.clone(),
        },
        "purchase_order" => GrantEventPayload::PurchaseOrderApprovalAssigned {
            purchase_order_id: instance.subject.subject_id().into(),
            approval_instance_id: instance.base.id.clone(),
            node_id: execution.node_key.clone(),
            assignment_id: execution.base.id.clone(),
        },
        _ => return Err(Error::ValidationError("该审批类型不产生订单读取授权".into())),
    };
    Ok(GrantEventEvidence {
        payload,
        recipient: recipient.into(),
        fact_version: instance.base.version,
        occurred_at: execution.base.created_at,
    })
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{
        ApprovalParticipant, NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind, SubjectRef,
        Timestamp,
    };

    use super::*;

    fn instance(kind: &str, process: ProcessKind) -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 1,
            process_kind: process,
            subject: SubjectRef::new(kind, "order").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("applicant").unwrap(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap()
    }
    fn execution(instance_id: &str) -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("execution"),
            process_instance_id: ApprovalProcessInstanceId::new(instance_id),
            node_key: "review".into(),
            node_name: "复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("reviewer").unwrap(),
            assignee_name_snapshot: "复核人".into(),
            at: Timestamp::from_unix_secs(11).unwrap(),
        })
        .unwrap()
    }
    #[test]
    fn exact_assignment_maps_kind_identity_recipient_and_fact_version() {
        for (kind, process, rule) in [
            ("sales_order", ProcessKind::SalesOrder, "sales_order_approval_assigned"),
            ("purchase_order", ProcessKind::PurchaseOrder, "purchase_order_approval_assigned"),
        ] {
            let mut subject = instance(kind, process);
            subject.base.version = 7;
            let mut node = execution("instance");
            node.additional_assignees.push(ApprovalParticipant {
                participant_id: ParticipantId::new("additional-reviewer").unwrap(),
                name: "另一位复核人".into(),
            });
            assert_eq!(
                assignment_evidence(&subject, &node, "additional-reviewer").unwrap().recipient,
                "additional-reviewer"
            );
            let event = assignment_evidence(&subject, &node, "reviewer").unwrap();
            assert_eq!(event.fact_version, 7);
            assert_eq!(event.occurred_at, node.base.created_at);
            assert_eq!(event.recipient, "reviewer");
            assert_eq!(event.payload.source("reviewer").unwrap().rule, rule);
            match event.payload {
                GrantEventPayload::SalesOrderApprovalAssigned {
                    sales_order_id,
                    assignment_id,
                    node_id,
                    ..
                } => assert_eq!(
                    (sales_order_id.as_str(), assignment_id.as_str(), node_id.as_str()),
                    ("order", "execution", "review")
                ),
                GrantEventPayload::PurchaseOrderApprovalAssigned {
                    purchase_order_id,
                    assignment_id,
                    node_id,
                    ..
                } => assert_eq!(
                    (purchase_order_id.as_str(), assignment_id.as_str(), node_id.as_str()),
                    ("order", "execution", "review")
                ),
                _ => panic!("事件类型错配"),
            }
            assert!(assignment_evidence(&subject, &node, "candidate").is_err());
            assert!(assignment_evidence(&subject, &execution("other"), "reviewer").is_err());
        }
    }
}
