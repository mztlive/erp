//! 受阻审批取消的不可变命令及终态事实。

use bpm::model::types::{
    ApprovalBlockerCode, ApprovalCommandKind, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
};
use bpm::model::{
    ApprovalCommandReceipt, ApprovalNodeExecution, ApprovalProcessInstance, SubjectRef, Timestamp,
};
use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::entity::work_item::{WorkItem, WorkItemStatus, WorkItemType};
use crate::{Error, Result};

/// 取消时冻结的历史任务身份，排序不依赖数据库返回次序。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub struct CancellationTaskFact {
    pub task_id: String,
    pub version: u64,
}

/// ERP 拥有的受阻取消事实，与 BPM 回执保持精确结构化关联。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct ApprovalCancellationFact {
    pub schema_version: u32,
    #[serde(flatten)]
    pub base: BaseModel,
    pub actor_id: String,
    pub reason: String,
    pub receipt: ApprovalCommandReceipt,
    pub audit_event_id: String,
    pub subject: SubjectRef,
    pub subject_version: u32,
    pub execution_id: String,
    pub blocker: ApprovalBlockerCode,
    pub instance_version: u64,
    pub execution_version: u64,
    pub tasks: Vec<CancellationTaskFact>,
    pub cancelled_at: Timestamp,
}

impl ApprovalCancellationFact {
    /// 从计划提交的取消终态及原历史任务形成不可变事实。
    /// # 参数
    /// * `receipt` - 同事务第一笔写入的审批命令回执。
    /// * `instance` - 取消后的实例。
    /// * `execution` - 取消后的唯一执行。
    /// * `actor_id` - 当前已授权操作人。
    /// * `reason` - 已规范化取消原因。
    /// * `tasks` - 原执行关联的全部历史任务。
    /// * `audit_event_id` - 同事务成功事件 ID，仅用于关联。
    /// # 返回
    /// 返回按实例唯一定位的取消事实。
    /// # 错误
    /// 回执、终态、blocker 或任务不一致。
    pub fn new(
        receipt: ApprovalCommandReceipt,
        instance: &ApprovalProcessInstance,
        execution: &ApprovalNodeExecution,
        actor_id: String,
        reason: String,
        tasks: &[WorkItem],
        audit_event_id: String,
    ) -> Result<Self> {
        let fact = Self {
            schema_version: 1,
            base: BaseModel::new(instance.base.id.clone()),
            actor_id,
            reason,
            receipt,
            audit_event_id,
            subject: instance.subject.clone(),
            subject_version: instance.subject_version,
            execution_id: execution.base.id.clone(),
            blocker: execution.blocker_code.ok_or_else(invalid_fact)?,
            instance_version: instance.base.version,
            execution_version: execution.base.version,
            tasks: task_facts(tasks),
            cancelled_at: instance.ended_at.ok_or_else(invalid_fact)?,
        };
        fact.ensure_runtime(instance, execution, tasks)?;
        Ok(fact)
    }

    /// 同时证明保存身份、取消实例、执行及全部历史任务。
    /// # 参数
    /// * `instance` - 同执行器读取的实例终态。
    /// * `execution` - 同执行器读取的原执行终态。
    /// * `tasks` - 同执行器读取的历史任务。
    /// # 返回
    /// 全部相互匹配返回空值。
    /// # 错误
    /// 缺失或损坏事实、终态或任务不匹配；不得回退审计。
    pub fn ensure_runtime(
        &self,
        instance: &ApprovalProcessInstance,
        execution: &ApprovalNodeExecution,
        tasks: &[WorkItem],
    ) -> Result<()> {
        if self.actor_id.trim().is_empty()
            || self.audit_event_id.trim().is_empty()
            || self.base.id.trim().is_empty()
            || self.execution_id.trim().is_empty()
            || self.subject.subject_kind().trim().is_empty()
            || self.subject.subject_id().trim().is_empty()
            || self.schema_version != 1
            || self.reason.trim().is_empty()
            || self.base.is_deleted()
            || self.receipt.base.is_deleted()
            || self.receipt.base.id.trim().is_empty()
            || self.subject_version == 0
            || self.instance_version == 0
            || self.execution_version == 0
            || self.base.id != instance.base.id
            || self.receipt.result_ref != instance.base.id
            || self.receipt.command_kind != ApprovalCommandKind::CancelBlocked
            || self.receipt.scope_id.trim().is_empty()
            || self.receipt.payload_digest.trim().is_empty()
            || self.subject != instance.subject
            || self.subject_version != instance.subject_version
            || self.blocker.allows_assignee_recovery()
            || instance.status != ApprovalProcessInstanceStatus::Cancelled
            || instance.base.is_deleted()
            || instance.current_node_execution_id.is_some()
            || instance.blocker_code.is_some()
            || instance.ended_at != Some(self.cancelled_at)
            || instance.base.version != self.instance_version
            || self.execution_id != execution.base.id
            || execution.process_instance_id.as_ref() != instance.base.id
            || execution.status != ApprovalNodeExecutionStatus::Cancelled
            || execution.base.is_deleted()
            || execution.blocker_code != Some(self.blocker)
            || execution.ended_at != Some(self.cancelled_at)
            || execution.base.version != self.execution_version
            || execution.round_no != instance.current_round_no
            || !self.matches_tasks(tasks)
        {
            return Err(invalid_fact());
        }
        Ok(())
    }

    /// 原执行的历史任务必须完整、唯一且保持业务主体引用。
    fn matches_tasks(&self, tasks: &[WorkItem]) -> bool {
        self.tasks == task_facts(tasks)
            && !self.tasks.windows(2).any(|pair| pair[0].task_id >= pair[1].task_id)
            && !self.tasks.iter().any(|task| task.task_id.trim().is_empty() || task.version == 0)
            && tasks.iter().all(|task| {
                task.status != WorkItemStatus::Open
                    && task.work_item_type == WorkItemType::DocumentApproval
                    && !task.base.is_deleted()
                    && task.approval_node_execution_id.as_ref().map(AsRef::as_ref)
                        == Some(self.execution_id.as_str())
                    && task.business_object_type == self.subject.subject_kind()
                    && task.business_object_id == self.subject.subject_id()
                    && task.subject_version == self.subject_version.to_string()
            })
    }
}

/// 保存任务 ID 与版本的确定排序。
fn task_facts(tasks: &[WorkItem]) -> Vec<CancellationTaskFact> {
    let mut facts = tasks
        .iter()
        .map(|task| CancellationTaskFact { task_id: task.base.id.clone(), version: task.base.version })
        .collect::<Vec<_>>();
    facts.sort();
    facts
}

/// 结构化取消证明失败时的稳定拒绝。
fn invalid_fact() -> Error {
    Error::ConflictError("审批取消结构化事实与终态不一致".into())
}

#[cfg(test)]
mod tests {
    use bpm::ids::{
        ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId,
        ApprovalProcessInstanceId,
    };
    use bpm::model::types::ApprovalExecutionAssignmentSource;
    use bpm::model::{IdempotencyKey, NewNodeExecution, NewProcessInstance, ParticipantId, ProcessKind};
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use super::*;
    use crate::entity::work_item::{DocumentApprovalWorkItemData, WorkItemCloseData, WorkItemPriority};
    use crate::service::approval::execution::idempotency::{
        CancelBlockedIdentityParams, cancel_blocked_identity,
    };

    fn runtime() -> (ApprovalProcessInstance, ApprovalNodeExecution, ApprovalCommandReceipt) {
        let at = Timestamp::from_unix_secs(10).unwrap();
        let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("instance"),
            process_definition_id: ApprovalProcessDefinitionId::new("definition"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("submitter").unwrap(),
            at,
        })
        .unwrap();
        let mut execution = ApprovalNodeExecution::new_blocked(
            NewNodeExecution {
                id: ApprovalNodeExecutionId::new("execution"),
                process_instance_id: instance.typed_id(),
                node_key: "review".into(),
                node_name: "复核".into(),
                round_no: 1,
                execution_no: 1,
                assignment_source: ApprovalExecutionAssignmentSource::Definition,
                replaces_execution_id: None,
                assignee_participant_id: ParticipantId::new("approver").unwrap(),
                assignee_name_snapshot: "审批人".into(),
                at,
            },
            ApprovalBlockerCode::DefinitionGraphCorrupted,
        )
        .unwrap();
        instance.set_current_execution(execution.typed_id(), at).unwrap();
        instance.enter_blocked(ApprovalBlockerCode::DefinitionGraphCorrupted, at).unwrap();
        let identity = cancel_blocked_identity(CancelBlockedIdentityParams {
            idempotency_key: IdempotencyKey::parse("key").unwrap(),
            instance_id: "instance",
            blocker: ApprovalBlockerCode::DefinitionGraphCorrupted.as_str(),
            expected_instance_version: instance.base.version,
            expected_execution_version: execution.base.version,
            expected_task_version: None,
            reason: "终止",
            actor_id: "operator",
        })
        .unwrap();
        let ended = Timestamp::from_unix_secs(20).unwrap();
        let receipt = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("receipt"),
            identity.current(),
            "instance",
            ended,
        )
        .unwrap();
        instance.cancel(ended).unwrap();
        execution.cancel(ended).unwrap();
        (instance, execution, receipt)
    }

    fn task() -> WorkItem {
        let mut task = WorkItem::new_document_approval(
            WorkItemId::new("task"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("execution"),
                business_object_type: "stock_adjustment".into(),
                business_object_id: "adj".into(),
                subject_version: "1".into(),
                owner_role: "stock_adjustment_approver".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "approver".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .unwrap();
        task.close_by_approval_runtime(
            "operator",
            WorkItemCloseData { close_reason: "BLOCKED".into() },
            Instant::from_unix_secs(15),
        )
        .unwrap();
        task
    }

    #[test]
    fn cancellation_fact_proves_receipt_execution_and_terminal_time() {
        let (instance, execution, receipt) = runtime();
        let fact = ApprovalCancellationFact::new(
            receipt,
            &instance,
            &execution,
            "operator".into(),
            "终止".into(),
            &[],
            "audit".into(),
        )
        .unwrap();
        assert!(fact.ensure_runtime(&instance, &execution, &[]).is_ok());
        for case in 0..9 {
            let mut damaged = fact.clone();
            match case {
                0 => damaged.actor_id.clear(),
                1 => damaged.reason.clear(),
                2 => damaged.receipt.result_ref = "foreign".into(),
                3 => damaged.receipt.command_kind = ApprovalCommandKind::CancelApproval,
                4 => damaged.execution_id = "foreign".into(),
                5 => damaged.instance_version += 1,
                6 => damaged.execution_version += 1,
                7 => damaged.cancelled_at = Timestamp::from_unix_secs(21).unwrap(),
                _ => damaged.blocker = ApprovalBlockerCode::ApproverAccountInactive,
            }
            assert!(damaged.ensure_runtime(&instance, &execution, &[]).is_err(), "case {case}");
        }
        let mut wrong = execution.clone();
        wrong.process_instance_id = ApprovalProcessInstanceId::new("foreign");
        assert!(fact.ensure_runtime(&instance, &wrong, &[]).is_err());
        let mut wrong = instance;
        wrong.current_node_execution_id = Some(execution.typed_id());
        assert!(fact.ensure_runtime(&wrong, &execution, &[]).is_err());
    }

    #[test]
    fn cancellation_fact_requires_exact_history_and_rejects_duplicate_or_open_tasks() {
        let (instance, execution, receipt) = runtime();
        let task = task();
        let fact = ApprovalCancellationFact::new(
            receipt.clone(),
            &instance,
            &execution,
            "operator".into(),
            "终止".into(),
            std::slice::from_ref(&task),
            "audit".into(),
        )
        .unwrap();
        assert!(fact.ensure_runtime(&instance, &execution, std::slice::from_ref(&task)).is_ok());
        assert!(fact.ensure_runtime(&instance, &execution, &[]).is_err());
        let mut changed = task.clone();
        changed.base.version += 1;
        assert!(fact.ensure_runtime(&instance, &execution, &[changed]).is_err());
        for case in 0..4 {
            let mut changed = task.clone();
            match case {
                0 => changed.business_object_id = "foreign".into(),
                1 => changed.business_object_type = "sales_order".into(),
                2 => changed.subject_version = "2".into(),
                _ => changed.work_item_type = WorkItemType::FulfillmentOperation,
            }
            assert!(fact.ensure_runtime(&instance, &execution, &[changed]).is_err(), "case {case}");
        }
        assert!(
            ApprovalCancellationFact::new(
                receipt.clone(),
                &instance,
                &execution,
                "operator".into(),
                "终止".into(),
                &[task.clone(), task.clone()],
                "audit".into()
            )
            .is_err()
        );
        let mut open = task;
        open.status = WorkItemStatus::Open;
        assert!(
            ApprovalCancellationFact::new(
                receipt,
                &instance,
                &execution,
                "operator".into(),
                "终止".into(),
                &[open],
                "audit".into()
            )
            .is_err()
        );
    }
}
