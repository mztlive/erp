//! 统一取消计划：状态、开放任务数量与 blocker 端口分类。
//!
//! 采购单与采购变更单等所有单据的撤回共用本规则，禁止在 Service 复制状态机
//! 判断或开放任务约束。调用方只加载运行事实并统计开放任务数量。

use super::EngineResult;
use crate::model::{ApprovalCancellationTaskPolicy, ApprovalNodeExecution, ApprovalProcessInstance};

/// 取消计划输入：运行事实由调用方加载，任务数量由调用方统计。
#[derive(Debug, Clone, Copy)]
pub struct CancelPlanInput<'a> {
    /// 当前实例。
    pub instance: &'a ApprovalProcessInstance,
    /// 当前执行。
    pub current: &'a ApprovalNodeExecution,
    /// 当前执行关联的开放审批任务数量。
    pub open_task_count: usize,
}

/// 统一取消计划。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CancelPlan {
    /// 开放任务关闭策略。
    pub task_policy: ApprovalCancellationTaskPolicy,
    /// 是否必须关闭唯一开放任务。
    pub close_open_task: bool,
    /// 当前 blocker 是否要求受阻取消端口。
    pub blocked_port_required: bool,
}

/// 撤回用例加载当前执行并统计开放任务后，用本计划确定关闭策略和取消端口。
/// 运行中的单人节点须有一个开放任务，或签节点须有至少一个且不超过冻结人数；受阻节点不得有开放任务。
/// 人员失效仍走业务取消，其他阻塞走受阻取消。终态、缺少当前执行引用或任务数量不符时拒绝规划。
///
/// 本函数不查仓储；调用方提交时仍须以实例、执行和任务版本做 CAS，与并发审批仲裁。
pub fn plan_cancel(input: CancelPlanInput<'_>) -> EngineResult<CancelPlan> {
    let task_policy = input.instance.cancellation_task_policy()?;
    input.current.ensure_cancellation_tasks(task_policy, input.open_task_count)?;

    let blocker = input.instance.blocker_code.or(input.current.blocker_code);
    let blocked_port_required = blocker.is_some_and(|code| !code.allows_assignee_recovery());
    Ok(CancelPlan { task_policy, close_open_task: task_policy.closes_open_task(), blocked_port_required })
}

#[cfg(test)]
mod tests {
    use super::super::test_fixtures::at;
    use super::{CancelPlan, CancelPlanInput, plan_cancel};
    use crate::engine::EngineError;
    use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use crate::model::types::{
        ApprovalBlockerCode, ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus, ModelError,
    };
    use crate::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, ParticipantId, ProcessKind,
        SubjectRef,
    };

    /// 构造运行中且已进入当前执行的实例夹具。
    fn running_instance() -> ApprovalProcessInstance {
        let mut inst = ApprovalProcessInstance::start_running(crate::model::NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::PurchaseOrder,
            subject: SubjectRef::new("purchase_order", "po-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("starter").unwrap(),
            at: at(10),
        })
        .unwrap();
        inst.set_current_execution(ApprovalNodeExecutionId::new("e1"), at(11)).unwrap();
        inst
    }

    /// 构造受阻实例夹具。
    fn blocked_instance(code: ApprovalBlockerCode) -> ApprovalProcessInstance {
        let mut inst = running_instance();
        inst.enter_blocked(code, at(12)).unwrap();
        inst
    }

    /// 构造当前执行夹具。
    fn current_execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst"),
            node_key: "n1".into(),
            node_name: "采购确认".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: at(11),
        })
        .expect("当前执行夹具")
    }

    // 验证待审节点只有一条开放任务时，业务撤回会同时安排关闭该任务。
    #[test]
    fn running_with_single_open_task_plans_close_and_business_port() {
        let plan = plan_cancel(CancelPlanInput {
            instance: &running_instance(),
            current: &current_execution(),
            open_task_count: 1,
        })
        .unwrap();
        assert!(plan.close_open_task);
        assert!(!plan.blocked_port_required);
        assert_eq!(plan.task_policy, crate::model::ApprovalCancellationTaskPolicy::CloseOpenTask);
    }

    // 运行中的单人节点不应无待办；缺任务时撤回不能掩盖这项不一致。
    #[test]
    fn running_without_open_task_fails_closed() {
        let error = plan_cancel(CancelPlanInput {
            instance: &running_instance(),
            current: &current_execution(),
            open_task_count: 0,
        })
        .unwrap_err();
        assert_eq!(
            error,
            EngineError::Model(ModelError::InvalidStatus("运行中审批实例必须恰有一个开放任务"))
        );
    }

    // 单人节点出现多条开放任务时应拒绝撤回，而非随意选择一条关闭。
    #[test]
    fn running_with_multiple_open_tasks_fails_closed() {
        let error = plan_cancel(CancelPlanInput {
            instance: &running_instance(),
            current: &current_execution(),
            open_task_count: 2,
        })
        .unwrap_err();
        assert_eq!(
            error,
            EngineError::Model(ModelError::InvalidStatus("运行中审批实例必须恰有一个开放任务"))
        );
    }

    // 人员受阻且任务已关闭时，撤回仍走业务端口，不会重复关闭任务。
    #[test]
    fn blocked_without_open_task_plans_no_close() {
        let plan = plan_cancel(CancelPlanInput {
            instance: &blocked_instance(ApprovalBlockerCode::ApproverAccountInactive),
            current: &current_execution(),
            open_task_count: 0,
        })
        .unwrap();
        assert!(!plan.close_open_task);
        assert!(!plan.blocked_port_required);
        assert_eq!(plan.task_policy, crate::model::ApprovalCancellationTaskPolicy::NoOpenTask);
    }

    // 验证受阻节点遗留开放任务时不会生成撤回计划。
    #[test]
    fn blocked_with_open_task_fails_closed() {
        let error = plan_cancel(CancelPlanInput {
            instance: &blocked_instance(ApprovalBlockerCode::ApproverAccountInactive),
            current: &current_execution(),
            open_task_count: 1,
        })
        .unwrap_err();
        assert_eq!(error, EngineError::Model(ModelError::InvalidStatus("受阻审批实例不得存在开放任务")));
    }

    // 以职责分离冲突为例，验证可原人恢复的阻塞仍走业务撤回，而不是受阻取消端口。
    #[test]
    fn personnel_blocker_uses_business_cancel_port() {
        let plan = plan_cancel(CancelPlanInput {
            instance: &blocked_instance(ApprovalBlockerCode::SeparationOfDutiesViolation),
            current: &current_execution(),
            open_task_count: 0,
        })
        .unwrap();
        assert!(!plan.blocked_port_required);
        assert_eq!(
            plan,
            CancelPlan {
                task_policy: crate::model::ApprovalCancellationTaskPolicy::NoOpenTask,
                close_open_task: false,
                blocked_port_required: false,
            }
        );
    }

    // 以开放任务冲突为例，验证一致性阻塞必须走受阻取消端口，不能按普通撤回处理。
    #[test]
    fn structural_blocker_requires_blocked_cancel_port() {
        let plan = plan_cancel(CancelPlanInput {
            instance: &blocked_instance(ApprovalBlockerCode::OpenTaskConflict),
            current: &current_execution(),
            open_task_count: 0,
        })
        .unwrap();
        assert!(plan.blocked_port_required);
    }

    // 最终通过后，即使提供先前执行也不能生成撤回计划。
    #[test]
    fn approved_instance_fails_closed() {
        let mut approved = running_instance();
        approved.complete_approved(at(13)).unwrap();
        let error = plan_cancel(CancelPlanInput {
            instance: &approved,
            current: &current_execution(),
            open_task_count: 0,
        })
        .unwrap_err();
        assert_eq!(error, EngineError::Model(ModelError::InvalidStatus("已最终通过的审批实例不得撤回")));
    }

    // 已取消实例不会再次得到撤回计划，避免重复处理任务。
    #[test]
    fn cancelled_instance_fails_closed() {
        let mut cancelled = running_instance();
        cancelled.cancel(at(13)).unwrap();
        let error = plan_cancel(CancelPlanInput {
            instance: &cancelled,
            current: &current_execution(),
            open_task_count: 0,
        })
        .unwrap_err();
        assert_eq!(error, EngineError::Model(ModelError::InvalidStatus("已取消的审批实例不得重复撤回")));
    }

    // 即使传入执行和任务数量，实例没有当前令牌也不能规划撤回。
    #[test]
    fn missing_current_execution_fails_closed() {
        let fresh = ApprovalProcessInstance::start_running(crate::model::NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::PurchaseOrder,
            subject: SubjectRef::new("purchase_order", "po-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("starter").unwrap(),
            at: at(10),
        })
        .unwrap();

        let error = plan_cancel(CancelPlanInput {
            instance: &fresh,
            current: &current_execution(),
            open_task_count: 1,
        })
        .unwrap_err();
        assert_eq!(error, EngineError::Model(ModelError::InvalidStatus("可撤回审批实例必须存在当前执行")));
        assert_eq!(fresh.status, ApprovalProcessInstanceStatus::Running);
    }
}
