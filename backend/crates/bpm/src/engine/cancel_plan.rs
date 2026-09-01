//! 统一取消计划：状态、开放任务数量与 blocker 端口分类。
//!
//! 采购单与采购变更单等所有单据的撤回共用本规则，禁止在 Service 复制状态机
//! 判断或开放任务约束。调用方只加载运行事实并统计开放任务数量。

use crate::model::{ApprovalCancellationTaskPolicy, ApprovalNodeExecution, ApprovalProcessInstance};

use super::EngineResult;

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

/// 构造统一取消计划。
///
/// 终态或缺少当前执行的实例失败关闭；运行中必须恰有一个开放任务，受阻必须
/// 没有开放任务；人员失效类 blocker 走业务取消端口，其余 blocker 必须走受阻
/// 取消端口。
///
/// # 参数
/// * `input` - 实例、当前执行与开放任务数量
///
/// # 返回
/// 返回任务关闭策略与取消端口分类。
///
/// # 错误
/// 实例已终态、缺少当前执行或开放任务数量与状态不一致时返回模型状态错误。
///
/// # 关键业务约束
/// 取消与并发审批写入的仲裁继续由调用方按实例、执行与任务版本 CAS 保证；
/// 本计划只计算纯规则，不读取仓储。
pub fn plan_cancel(input: CancelPlanInput<'_>) -> EngineResult<CancelPlan> {
    let task_policy = input.instance.cancellation_task_policy()?;
    task_policy.ensure_open_task_count(input.open_task_count)?;
    let blocker = input.instance.blocker_code.or(input.current.blocker_code);
    let blocked_port_required = blocker.is_some_and(|code| !code.allows_assignee_recovery());
    Ok(CancelPlan {
        task_policy,
        close_open_task: task_policy.closes_open_task(),
        blocked_port_required,
    })
}

#[cfg(test)]
mod tests {
    use super::{plan_cancel, CancelPlan, CancelPlanInput};
    use crate::engine::EngineError;
    use crate::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use crate::model::types::{
        ApprovalBlockerCode, ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus, ModelError,
    };
    use crate::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, ParticipantId, ProcessKind,
        SubjectRef, Timestamp,
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
        inst.set_current_execution(ApprovalNodeExecutionId::new("e1"), at(11))
            .unwrap();
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

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix_secs(secs).unwrap()
    }

    /// 运行中实例恰有一个开放任务：关闭任务并走业务取消端口。
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
        assert_eq!(
            plan.task_policy,
            crate::model::ApprovalCancellationTaskPolicy::CloseOpenTask
        );
    }

    /// 运行中实例零开放任务表示运行事实不一致，必须失败关闭。
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

    /// 运行中实例重复开放任务同样失败关闭。
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

    /// 受阻实例必须证明没有开放任务；人员失效 blocker 仍走业务取消端口。
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
        assert_eq!(
            plan.task_policy,
            crate::model::ApprovalCancellationTaskPolicy::NoOpenTask
        );
    }

    /// 受阻实例仍持有开放任务时失败关闭。
    #[test]
    fn blocked_with_open_task_fails_closed() {
        let error = plan_cancel(CancelPlanInput {
            instance: &blocked_instance(ApprovalBlockerCode::ApproverAccountInactive),
            current: &current_execution(),
            open_task_count: 1,
        })
        .unwrap_err();
        assert_eq!(
            error,
            EngineError::Model(ModelError::InvalidStatus("受阻审批实例不得存在开放任务"))
        );
    }

    /// 人员失效 blocker 只允许业务取消端口。
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

    /// 非人员 blocker 必须走受阻取消端口。
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

    /// 最终通过实例不得撤回。
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
        assert_eq!(
            error,
            EngineError::Model(ModelError::InvalidStatus("已最终通过的审批实例不得撤回"))
        );
    }

    /// 已取消实例不得重复撤回。
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
        assert_eq!(
            error,
            EngineError::Model(ModelError::InvalidStatus("已取消的审批实例不得重复撤回"))
        );
    }

    /// 可撤回实例缺少当前执行引用时失败关闭。
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
        assert_eq!(
            error,
            EngineError::Model(ModelError::InvalidStatus("可撤回审批实例必须存在当前执行"))
        );
        assert_eq!(fresh.status, ApprovalProcessInstanceStatus::Running);
    }
}
