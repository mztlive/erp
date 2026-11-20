//! 授权编排使用的审批事实快照与正式分派证据。
use bpm::SubjectRef;
use bpm::model::types::{ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_identity::entity::resource_access::event::{GrantEventEvidence, GrantEventPayload};
use mongodb::Database;
use persistence_core::{Executor, Result};

use crate::repository::{BpmExt, BpmWorkflowRepository};

/// 供记录授权核对审批状态、关联单据和事实版本。
#[derive(Debug, PartialEq, Eq)]
pub struct ApprovalGrantSnapshot {
    /// 审批实例主键。
    pub id: String,
    /// 审批实例的持久化版本。
    pub version: u64,
    /// 实例是否已审批通过。
    pub approved: bool,
    /// 冻结的关联单据类型。
    pub subject_kind: String,
    /// 冻结的关联单据主键。
    pub subject_id: String,
    /// 提交审批时的单据版本。
    pub subject_version: u32,
}

impl ApprovalGrantSnapshot {
    fn from_instance(instance: ApprovalProcessInstance) -> Self {
        Self {
            id: instance.base.id,
            version: instance.base.version,
            approved: instance.status == ApprovalProcessInstanceStatus::Approved,
            subject_kind: instance.subject.subject_kind().into(),
            subject_id: instance.subject.subject_id().into(),
            subject_version: instance.subject_version,
        }
    }
}

/// 供记录授权核对节点与实例关系及接收人的正式分派。
#[derive(Debug, PartialEq, Eq)]
pub struct NodeExecutionGrantSnapshot {
    /// 节点执行所属的审批实例主键。
    pub process_instance_id: String,
    /// 冻结的审批节点键。
    pub node_key: String,
    /// 节点是否是尚未正常分派的受阻执行。
    pub blocked: bool,
    /// 指定接收人是否属于节点的冻结参与人。
    pub has_recipient: bool,
}

impl NodeExecutionGrantSnapshot {
    fn from_execution(execution: ApprovalNodeExecution, recipient: &str) -> Self {
        Self {
            has_recipient: execution.has_participant(recipient),
            process_instance_id: execution.process_instance_id.to_string(),
            node_key: execution.node_key,
            blocked: execution.status == ApprovalNodeExecutionStatus::Blocked,
        }
    }
}

/// 用当前执行器按主键读取审批实例的授权事实，缺失时返回空。
pub async fn find_approval_snapshot(
    db: &Database,
    approval_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalGrantSnapshot>> {
    Ok(db
        .approval_process_instances()
        .find_by_id(approval_id, executor)
        .await?
        .map(ApprovalGrantSnapshot::from_instance))
}

/// 读取指定开票申请的最近审批；非法申请引用和没有历史实例均无可用快照。
pub async fn find_latest_invoice_approval_snapshot(
    db: &Database,
    request_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalGrantSnapshot>> {
    let Ok(subject) = SubjectRef::new("sales_invoice_request", request_id) else {
        return Ok(None);
    };
    Ok(BpmWorkflowRepository::new(db)
        .find_latest_by_subject(&subject, executor)
        .await?
        .map(ApprovalGrantSnapshot::from_instance))
}

/// 用同一执行器依次读取实例和节点；任一缺失时返回空，由调用方核对关联及接收人。
pub async fn find_assignment_snapshots(
    db: &Database,
    instance_id: &str,
    execution_id: &str,
    recipient: &str,
    executor: &mut dyn Executor,
) -> Result<Option<(ApprovalGrantSnapshot, NodeExecutionGrantSnapshot)>> {
    let Some(approval) = find_approval_snapshot(db, instance_id, executor).await? else {
        return Ok(None);
    };
    Ok(db
        .approval_node_executions()
        .find_by_id(execution_id, executor)
        .await?
        .map(|execution| (approval, NodeExecutionGrantSnapshot::from_execution(execution, recipient))))
}

/// 由正式审批实例及节点执行构造订单读取授权证据。
/// 拒绝跨实例或非节点参与人的分派。
pub fn assignment_evidence(
    instance: &ApprovalProcessInstance,
    execution: &ApprovalNodeExecution,
    recipient: &str,
) -> crate::Result<GrantEventEvidence> {
    if execution.process_instance_id.as_ref() != instance.base.id || !execution.has_participant(recipient) {
        return Err(crate::Error::ConflictError("审批查看权限与实际分派人员不符，请联系管理员核对".into()));
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
        _ => return Err(crate::Error::ValidationError("该审批类型不产生订单读取授权".into())),
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
    fn grant_snapshots_keep_approval_version_and_frozen_assignment_facts() {
        let mut instance = instance("sales_invoice_request", ProcessKind::SalesInvoiceRequest);
        assert!(!ApprovalGrantSnapshot::from_instance(instance.clone()).approved);
        instance.complete_approved(Timestamp::from_unix_secs(15).unwrap()).unwrap();
        let version = instance.base.version;
        assert_eq!(
            ApprovalGrantSnapshot::from_instance(instance),
            ApprovalGrantSnapshot {
                id: "instance".into(),
                version,
                approved: true,
                subject_kind: "sales_invoice_request".into(),
                subject_id: "order".into(),
                subject_version: 1,
            },
        );

        let mut node = execution("instance");
        assert!(!NodeExecutionGrantSnapshot::from_execution(node.clone(), "candidate").has_recipient);
        assert!(!NodeExecutionGrantSnapshot::from_execution(node.clone(), "reviewer").blocked);
        node.additional_assignees.push(ApprovalParticipant {
            participant_id: ParticipantId::new("additional-reviewer").unwrap(),
            name: "另一位复核人".into(),
        });
        node.status = ApprovalNodeExecutionStatus::Blocked;
        assert_eq!(
            NodeExecutionGrantSnapshot::from_execution(node, "additional-reviewer"),
            NodeExecutionGrantSnapshot {
                process_instance_id: "instance".into(),
                node_key: "review".into(),
                blocked: true,
                has_recipient: true,
            },
        );
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
