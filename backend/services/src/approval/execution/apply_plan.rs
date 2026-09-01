//! 将 BPM 计划映射为可在同一事务内应用的写入意图。

use bpm::engine::{BpmEvent, CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};
use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::{
    ApprovalCommandReceipt, ApprovalInstanceAssignee, ApprovalNodeExecution, ApprovalProcessInstance,
};

use super::notification_outbox::{map_notification_intents, NotificationIntent};

/// 强类型领域动作。适配器尚未接线时由调用方失败关闭。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainActionKind {
    /// 启动提交动作。
    Start,
    /// 最终通过动作。必须先于实例终态写入。
    FinalApprove,
    /// 撤回或受阻取消动作。
    Cancel,
}

/// 计划应用所需的中性写入集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedWrites {
    /// 更新后的实例。
    pub instance: ApprovalProcessInstance,
    /// 新执行。
    pub created_executions: Vec<ApprovalNodeExecution>,
    /// 更新执行。
    pub updated_executions: Vec<ApprovalNodeExecution>,
    /// 新审批人绑定。
    pub created_assignees: Vec<ApprovalInstanceAssignee>,
    /// 需要创建的任务。
    pub create_tasks: Vec<TaskIntent>,
    /// 需要完成的任务执行。
    pub complete_tasks: Vec<ApprovalNodeExecutionId>,
    /// 需要关闭的任务。
    pub close_tasks: Vec<(ApprovalNodeExecutionId, TaskCloseReason)>,
    /// 命令收据。
    pub receipt: ApprovalCommandReceipt,
    /// 必须先执行的领域动作。
    pub domain_action: Option<DomainActionKind>,
    /// 提交类别。
    pub commit: CommitRequired,
    /// 中性领域事件，供审计映射。
    pub events: Vec<BpmEvent>,
    /// 事务内待追加的通知意图。
    pub notifications: Vec<NotificationIntent>,
}

/// 校验计划只包含 BPM 状态和中性意图，再展开写入。
///
/// 最终通过必须先登记 `FinalApprove` 动作，再允许应用实例终态。
///
/// # 参数
/// * `plan` - 引擎计划
/// * `receipt` - 已构造收据
/// * `domain_action` - 本命令对应的领域动作
///
/// # 返回
/// 返回可在一个事务中应用的写入集合。
pub fn apply_plan(
    plan: TransitionPlan,
    receipt: ApprovalCommandReceipt,
    domain_action: Option<DomainActionKind>,
) -> PlannedWrites {
    let mut create_tasks = Vec::new();
    let mut complete_tasks = Vec::new();
    let mut close_tasks = Vec::new();
    for intent in plan.task_intents {
        match intent {
            TaskIntent::HumanTaskRequested { .. } => create_tasks.push(intent),
            TaskIntent::CompleteTask { execution_id } => complete_tasks.push(execution_id),
            TaskIntent::CloseTask { execution_id, reason } => close_tasks.push((execution_id, reason)),
        }
    }
    PlannedWrites {
        instance: plan.instance,
        created_executions: plan.created_executions,
        updated_executions: plan.updated_executions,
        created_assignees: plan.created_assignees,
        create_tasks,
        complete_tasks,
        close_tasks,
        receipt,
        domain_action,
        commit: plan.commit,
        notifications: map_notification_intents(&plan.events),
        events: plan.events,
    }
}

/// 最终通过计划必须先执行领域动作。
///
/// # 参数
/// * `writes` - 计划写入
///
/// # 返回
/// 终态通过且未登记最终动作时返回 `false`。
pub fn final_approve_requires_domain_action(writes: &PlannedWrites) -> bool {
    writes.commit != CommitRequired::TerminalApproved
        || writes.domain_action == Some(DomainActionKind::FinalApprove)
}

#[cfg(test)]
mod tests {
    use super::{apply_plan, final_approve_requires_domain_action, DomainActionKind};
    use bpm::engine::{CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};
    use bpm::ids::{
        ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId,
        ApprovalProcessInstanceId,
    };
    use bpm::model::types::{ApprovalCommandKind, ApprovalProcessInstanceStatus};
    use bpm::model::{
        ApprovalCommandReceipt, ApprovalProcessInstance, NewProcessInstance, ParticipantId, ProcessKind,
        SubjectRef, Timestamp,
    };

    /// 任务意图按类型拆分，终态通过必须带最终动作。
    #[test]
    fn execution_apply_plan_splits_task_intents() {
        let instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("u1").unwrap(),
            at: Timestamp::from_unix_secs(1).unwrap(),
        })
        .unwrap();
        let mut plan = TransitionPlan::for_instance(instance, CommitRequired::TerminalApproved);
        plan.task_intents.push(TaskIntent::CompleteTask {
            execution_id: ApprovalNodeExecutionId::new("e1"),
        });
        plan.task_intents.push(TaskIntent::CloseTask {
            execution_id: ApprovalNodeExecutionId::new("e0"),
            reason: TaskCloseReason::ApprovalRuntimeBlocked,
        });
        let receipt = ApprovalCommandReceipt::new(
            ApprovalCommandReceiptId::new("r1"),
            ApprovalCommandKind::SubmitDecision,
            "e1",
            "key",
            "digest",
            "e1",
            Timestamp::from_unix_secs(1).unwrap(),
        )
        .unwrap();
        let missing = apply_plan(plan.clone(), receipt.clone(), None);
        assert!(!final_approve_requires_domain_action(&missing));
        let writes = apply_plan(plan, receipt, Some(DomainActionKind::FinalApprove));
        assert_eq!(writes.complete_tasks.len(), 1);
        assert_eq!(writes.close_tasks.len(), 1);
        assert!(final_approve_requires_domain_action(&writes));
        assert_eq!(writes.instance.status, ApprovalProcessInstanceStatus::Running);
    }
}
