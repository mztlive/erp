//! BPM 纯状态引擎：节点、事件、连线上的单令牌计算。
//!
//! 只接收 BPM 模型、命令值对象、已收敛资格结果和调用方提供的时间/ID。
//! 不得读取时钟、生成 ID、打开事务或访问 Repository。

mod cancel;
mod cancel_plan;
mod decision;
mod enter_node;
mod event;
mod resume;
mod start;
mod start_plan;
#[cfg(test)]
pub(crate) mod test_fixtures;
mod transition_plan;

use std::collections::HashSet;

pub use cancel::{CancelCommand, cancel};
pub use cancel_plan::{CancelPlan, CancelPlanInput, plan_cancel};
pub use decision::{DecideCommand, block_current, decide, reject_terminal};
pub use event::{BpmEvent, BpmEventKind};
pub use resume::{ResumeCommand, resume};
pub use start::{StartAssigneeBinding, StartCommand, start};
pub use start_plan::{StartBindingInput, StartPlan, StartPlanInput, plan_start};
pub use transition_plan::{CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};

pub use crate::graph::DefinitionGraph;
use crate::model::types::{ApprovalBlockerCode, ModelError};
use crate::model::{ApprovalNodeDefinition, ParticipantId};

/// 引擎计算失败。不可提交的不变量错误不得被应用层改写为半结构终态。
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum EngineError {
    /// 命令字段或前置条件非法。
    #[error("命令无效: {0}")]
    InvalidCommand(&'static str),
    /// 定义图损坏。
    #[error("定义图损坏")]
    GraphCorrupted,
    /// 无法形成合法快照，禁止提交。
    #[error("无法形成合法快照: {0}")]
    Uncommittable(&'static str),
    /// 领域模型不变式失败。
    #[error("{0}")]
    Model(ModelError),
}

impl From<ModelError> for EngineError {
    // 用 `?` 从模型操作返回时保留原始不变式错误，不把它转换成可提交的阻塞计划。
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

/// 引擎计算结果。
pub type EngineResult<T> = std::result::Result<T, EngineError>;

/// 调用方已收敛的人员资格。BPM 不再访问账号或权限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eligibility {
    /// 同节点或签资格；首位仅用于保持旧快照字段兼容。
    AnyOf {
        /// 首位参与人资格。
        primary: Box<Eligibility>,
        /// 其余参与人资格。
        additional: Vec<Eligibility>,
    },
    /// 责任人当前有效。
    Eligible {
        /// 责任人。
        participant: ParticipantId,
        /// 显示名快照。
        assignee_name_snapshot: String,
    },
    /// 责任人当前无效。
    Blocked {
        /// 责任人。
        participant: ParticipantId,
        /// 稳定阻塞码。
        code: ApprovalBlockerCode,
        /// 显示名快照。
        assignee_name_snapshot: String,
    },
}

impl Eligibility {
    /// 调用方收敛节点内各人的资格后，按定义顺序合成进入节点所用的结果。单人保持原形，多人保留首位以兼容旧快照。
    ///
    /// # 错误
    /// 人数不在 1 至 50、存在重复人员或传入嵌套资格时拒绝组合。
    pub fn any_of(mut members: Vec<Self>) -> EngineResult<Self> {
        if members.is_empty() || members.len() > 50 {
            return Err(EngineError::InvalidCommand("节点审批人数必须为 1 至 50"));
        }
        let mut seen = HashSet::new();
        for item in &members {
            if matches!(item, Self::AnyOf { .. }) || !seen.insert(item.participant_ref()) {
                return Err(EngineError::InvalidCommand("或签资格不能重复或嵌套"));
            }
        }

        let primary = members.remove(0);
        if members.is_empty() {
            Ok(primary)
        } else {
            Ok(Self::AnyOf { primary: Box::new(primary), additional: members })
        }
    }

    /// 按定义顺序展开逐人资格，供启动核验全部参与人、进入节点只向有效成员派发任务。
    pub fn members(&self) -> Vec<&Self> {
        match self {
            Self::AnyOf { primary, additional } => {
                std::iter::once(primary.as_ref()).chain(additional.iter()).collect()
            },
            _ => vec![self],
        }
    }

    pub(crate) fn validate_node(&self, node: &ApprovalNodeDefinition) -> EngineResult<()> {
        let members = self.members();
        let people = node
            .assignee_participant_id
            .iter()
            .chain(node.additional_assignees.iter().map(|person| &person.participant_id));
        if members.iter().any(|member| matches!(member, Self::AnyOf { .. }))
            || !members.into_iter().map(Self::participant_ref).eq(people)
        {
            return Err(EngineError::InvalidCommand("资格结果必须属于定义审批人"));
        }
        Ok(())
    }

    /// 为节点执行取资格携带的显示名，不另查账号；或签时返回首位参与人快照以沿用旧字段。
    pub fn name_snapshot(&self) -> &str {
        match self {
            Self::AnyOf { primary, .. } => primary.name_snapshot(),
            Self::Eligible { assignee_name_snapshot, .. } | Self::Blocked { assignee_name_snapshot, .. } => {
                assignee_name_snapshot
            },
        }
    }

    /// 进入节点时用此结果判断是否无人可办；单人失效返回其 [`ApprovalBlockerCode`]，或签仅在全员受阻时返回首位阻塞码。
    pub fn blocked_code(&self) -> Option<ApprovalBlockerCode> {
        match self {
            Self::AnyOf { primary, additional } => {
                let code = primary.blocked_code()?;
                additional.iter().all(|member| member.blocked_code().is_some()).then_some(code)
            },
            Self::Eligible { .. } => None,
            Self::Blocked { code, .. } => Some(*code),
        }
    }

    /// 为执行或任务意图复制处理人 ID；或签返回首位参与人，兼容旧快照中的单人字段。
    pub fn participant(&self) -> ParticipantId {
        self.participant_ref().clone()
    }

    // 在资格归属校验中借用身份，避免复制；或签按首位身份与原单人绑定字段比较。
    pub(crate) fn participant_ref(&self) -> &ParticipantId {
        match self {
            Self::AnyOf { primary, .. } => primary.participant_ref(),
            Self::Eligible { participant, .. } | Self::Blocked { participant, .. } => participant,
        }
    }
}

impl DefinitionGraph {
    /// 启动前按定义冻结的入口键定位首个执行节点；找不到时返回图损坏，不以节点存储顺序兜底。
    pub fn entry_node(&self) -> EngineResult<&ApprovalNodeDefinition> {
        self.node(&self.definition.entry_node_key).ok_or(EngineError::GraphCorrupted)
    }
}

#[cfg(test)]
mod tests {
    use super::enter_node::{EnterNodeInput, plan_enter_node};
    use super::event::BpmEventKind;
    use super::test_fixtures::{at, blocked, eligible, node, participant};
    use super::{
        CancelCommand, CommitRequired, DecideCommand, DefinitionGraph, Eligibility, EngineError,
        ResumeCommand, StartAssigneeBinding, StartCommand, TaskCloseReason, TaskIntent, block_current,
        cancel, decide, reject_terminal, resume, start,
    };
    use crate::ids::{
        ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId,
        ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
    };
    use crate::model::types::{
        ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
        ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus, ApprovalTransitionEvent, ModelError,
    };
    use crate::model::{
        ApprovalProcessDefinition, ApprovalProcessInstance, ApprovalTransitionDefinition, ProcessKind,
        SubjectRef,
    };

    // 对同一份定义、绑定、ID 和时间重复启动，验证计划不受外部状态影响。
    #[test]
    fn engine_start_is_deterministic() {
        let first = start_two_node(eligible("u1", "张三"));
        let second = start_two_node(eligible("u1", "张三"));
        assert_eq!(first, second);
        assert_eq!(first.instance.status, ApprovalProcessInstanceStatus::Running);
        assert_eq!(first.created_assignees.len(), 2);
        assert!(matches!(first.task_intents.first(), Some(TaskIntent::HumanTaskRequested { .. })));
        assert_eq!(
            first.events.iter().map(|event| event.kind.as_str()).collect::<Vec<_>>(),
            ["INSTANCE_STARTED", "NODE_ENTERED"]
        );
    }

    #[test]
    fn frozen_assignees_require_unique_binding_per_node_and_preserve_node_order() {
        let started = start_two_node(eligible("u1", "张三"));
        let graph = two_node_graph();
        let mut bindings = started.created_assignees;
        bindings.reverse();
        assert_eq!(graph.clone().with_frozen_assignees(&started.instance, &bindings).unwrap(), graph);

        let duplicate = vec![bindings[1].clone(), bindings[1].clone()];
        assert_eq!(
            graph.clone().with_frozen_assignees(&started.instance, &duplicate),
            Err(ModelError::InvalidField("实例节点指派缺失或重复"))
        );

        bindings[0].node_key = "missing-node".into();
        assert_eq!(
            graph.with_frozen_assignees(&started.instance, &bindings),
            Err(ModelError::InvalidField("实例节点指派缺失或重复"))
        );
    }

    // 让入口合格而后继人员失效，验证启动会检查整条链，而不是只检查首个待办。
    #[test]
    fn engine_start_rejects_ineligible_non_entry_assignee() {
        let error = start(
            StartCommand {
                instance_id: ApprovalProcessInstanceId::new("inst"),
                process_kind: ProcessKind::StockAdjustment,
                subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
                subject_version: 1,
                started_by: participant("starter"),
                entry_execution_id: ApprovalNodeExecutionId::new("e1"),
                now: at(10),
            },
            &two_node_graph(),
            &[
                StartAssigneeBinding {
                    id: ApprovalInstanceAssigneeId::new("a1"),
                    node_key: "n1".into(),
                    participant: participant("u1"),
                    eligibility: eligible("u1", "张三"),
                },
                StartAssigneeBinding {
                    id: ApprovalInstanceAssigneeId::new("a2"),
                    node_key: "n2".into(),
                    participant: participant("u2"),
                    eligibility: blocked("u2", "李四", ApprovalBlockerCode::ApproverAccountInactive),
                },
            ],
        )
        .unwrap_err();
        assert!(matches!(error, EngineError::InvalidCommand(_)));
    }

    // 入口人员失效时拒绝启动，不留下起步即受阻的新实例。
    #[test]
    fn engine_start_rejects_ineligible_entry_assignee() {
        let error = try_start_two_node(
            blocked("u1", "张三", ApprovalBlockerCode::ApproverAccountInactive),
            eligible("u2", "李四"),
        )
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("启动时全部审批人必须有效，不得创建受阻实例"));
    }

    // 用其他人的资格替代节点冻结人员，验证启动不会误用该资格。
    #[test]
    fn engine_start_rejects_eligibility_for_another_participant() {
        let error = try_start_two_node(eligible("u9", "钱七"), eligible("u2", "李四")).unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("资格结果必须属于定义审批人"));
    }

    // 验证下一节点人员失效时，当前通过和后继受阻仍在同一 `Proceed` 计划中提交。
    #[test]
    fn engine_approve_keeps_decision_when_next_blocked() {
        let started = start_two_node(eligible("u1", "张三"));
        let current = started.created_executions[0].clone();
        let plan = decide(
            started.instance,
            current,
            &two_node_graph(),
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: participant("u1"),
                current_eligibility: eligible("u1", "张三"),
                next_eligibility: blocked("u2", "李四", ApprovalBlockerCode::ApproverNotEligible),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(20),
            },
        )
        .unwrap();

        assert_eq!(plan.commit, CommitRequired::Proceed);
        assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Approved);
        assert_eq!(plan.created_executions[0].status, ApprovalNodeExecutionStatus::Blocked);
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Blocked);
        assert!(matches!(
            plan.task_intents.as_slice(),
            [TaskIntent::CompleteTask { execution_id }] if execution_id.as_ref() == "e1"
        ));

        assert_eq!(
            plan.events
                .iter()
                .map(|event| (
                    event.kind.as_str(),
                    event.execution_id.as_ref().map(AsRef::as_ref),
                    event.node_key.as_deref(),
                    event.blocker_code,
                ))
                .collect::<Vec<_>>(),
            [
                ("NODE_APPROVED", Some("e1"), Some("n1"), None),
                ("INSTANCE_BLOCKED", Some("e2"), Some("n2"), Some(ApprovalBlockerCode::ApproverNotEligible),),
            ]
        );
    }

    // 下一节点收到其他人的资格时应拒绝推进，不能借通过命令暗中换人。
    #[test]
    fn engine_decide_rejects_next_participant_outside_definition() {
        let started = start_two_node(eligible("u1", "张三"));
        let error = decide(
            started.instance,
            started.created_executions[0].clone(),
            &two_node_graph(),
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: participant("u1"),
                current_eligibility: eligible("u1", "张三"),
                next_eligibility: eligible("u9", "钱七"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(20),
            },
        )
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("节点责任人必须匹配定义审批人"));
    }

    // 验证末节点通过会清除当前执行引用，并以 `TerminalApproved` 通知调用方收结实例。
    #[test]
    fn engine_final_approve_marks_terminal() {
        let started = start_single_node(eligible("u1", "张三"));
        let current = started.created_executions[0].clone();
        let plan = decide(
            started.instance,
            current,
            &single_node_graph(),
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: participant("u1"),
                current_eligibility: eligible("u1", "张三"),
                next_eligibility: eligible("u1", "张三"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(21),
            },
        )
        .unwrap();
        assert_eq!(plan.commit, CommitRequired::TerminalApproved);
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Approved);
        assert!(plan.instance.current_node_execution_id.is_none());
    }

    #[test]
    fn terminal_rejection_finishes_without_restart_or_new_tasks() {
        let started = start_two_node(eligible("u1", "张三"));
        let current = started.created_executions[0].clone();
        let command = DecideCommand {
            decision: ApprovalDecision::Reject,
            reason: Some("依据不符".into()),
            actor: participant("u1"),
            current_eligibility: eligible("u1", "张三"),
            next_eligibility: blocked("u1", "张三", ApprovalBlockerCode::ApproverAccountInactive),
            next_execution_id: ApprovalNodeExecutionId::new("unused"),
            next_execution_no: 2,
            now: at(20),
        };
        let plan =
            reject_terminal(started.instance.clone(), current.clone(), &two_node_graph(), command.clone())
                .unwrap();
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Rejected);
        assert_eq!(plan.instance.current_round_no, 1);
        assert!(plan.instance.current_node_execution_id.is_none());
        assert!(plan.instance.ended_at.is_some());
        assert!(plan.created_executions.is_empty());
        assert!(plan.created_assignees.is_empty());
        assert!(matches!(plan.task_intents.as_slice(), [TaskIntent::CompleteTask { .. }]));
        assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Rejected);

        let mut empty_reason = command.clone();
        empty_reason.reason = Some("   ".into());
        assert!(
            reject_terminal(started.instance.clone(), current.clone(), &two_node_graph(), empty_reason)
                .is_err()
        );

        let mut inactive = command;
        inactive.current_eligibility = blocked("u1", "张三", ApprovalBlockerCode::ApproverAccountInactive);
        let blocked_plan = reject_terminal(started.instance, current, &two_node_graph(), inactive).unwrap();
        assert_eq!(blocked_plan.commit, CommitRequired::Blocked);
        assert_eq!(blocked_plan.instance.status, ApprovalProcessInstanceStatus::Blocked);
    }

    // 验证驳回终结本实例、不重启或新建执行，也不会改写已冻结的提交版本和轮次。
    #[test]
    fn engine_reject_terminates_without_restart() {
        let started = start_two_node(eligible("u1", "张三"));
        let subject_version = started.instance.subject_version;
        let current = started.created_executions[0].clone();
        let plan = decide(
            started.instance,
            current,
            &two_node_graph(),
            DecideCommand {
                decision: ApprovalDecision::Reject,
                reason: Some("资料不全".into()),
                actor: participant("u1"),
                current_eligibility: eligible("u1", "张三"),
                next_eligibility: eligible("u1", "张三"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(22),
            },
        )
        .unwrap();
        assert_eq!(plan.instance.current_round_no, 1);
        assert_eq!(plan.instance.subject_version, subject_version);
        assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Rejected);
        assert!(plan.created_executions.is_empty());
        assert!(plan.created_assignees.is_empty());
        assert!(plan.instance.current_node_execution_id.is_none());
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Rejected);
        assert!(!plan.events.iter().any(|event| event.kind == BpmEventKind::RoundRestarted));
    }

    // 验证当前单人审批资格失效时，计划关闭任务并提交阻塞事实，但不记下决定。
    #[test]
    fn engine_decide_blocks_current_actor_without_accepting() {
        let started = start_two_node(eligible("u1", "张三"));
        let current = started.created_executions[0].clone();
        let plan = decide(
            started.instance,
            current,
            &two_node_graph(),
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: participant("u1"),
                current_eligibility: blocked("u1", "张三", ApprovalBlockerCode::ApproverOutOfAuthorizedScope),
                next_eligibility: eligible("u2", "李四"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(23),
            },
        )
        .unwrap();
        assert_eq!(plan.commit, CommitRequired::Blocked);
        assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Blocked);
        assert!(matches!(
            plan.task_intents.first(),
            Some(TaskIntent::CloseTask { reason: TaskCloseReason::ApprovalRuntimeBlocked, .. })
        ));
    }

    // 验证标准阻塞计划同时包含阻塞码、执行变更、任务关闭和可关联节点的事件。
    #[test]
    fn engine_block_current_emits_complete_standard_plan() {
        let started = start_two_node(eligible("u1", "张三"));
        let current = started.created_executions[0].clone();
        let plan =
            block_current(started.instance, current, ApprovalBlockerCode::OpenTaskConflict, at(24)).unwrap();

        assert_eq!(plan.commit, CommitRequired::Blocked);
        assert_eq!(plan.updated_executions[0].blocker_code, Some(ApprovalBlockerCode::OpenTaskConflict));
        assert!(matches!(
            plan.task_intents.as_slice(),
            [TaskIntent::CloseTask { reason: TaskCloseReason::ApprovalRuntimeBlocked, .. }]
        ));
        let event = plan
            .events
            .iter()
            .find(|event| event.kind == super::BpmEventKind::InstanceBlocked)
            .expect("阻塞计划必须包含标准事件");
        assert_eq!(event.execution_id.as_ref().map(AsRef::as_ref), Some("e1"));
        assert_eq!(event.node_key.as_deref(), Some("n1"));
        assert_eq!(event.blocker_code, Some(ApprovalBlockerCode::OpenTaskConflict));
    }

    // 传入已不是当前令牌的执行，验证阻塞入口不允许旧执行影响实例。
    #[test]
    fn engine_block_current_rejects_non_current_execution() {
        let started = start_two_node(eligible("u1", "张三"));
        let mut current = started.created_executions[0].clone();
        current.base.id = "other-execution".to_string();

        let error = block_current(started.instance, current, ApprovalBlockerCode::OpenTaskConflict, at(24))
            .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("执行不是实例当前令牌"));
    }

    // 验证当前令牌已受阻时不会通过同一入口再写一次阻塞迁移。
    #[test]
    fn engine_block_current_rejects_non_active_execution() {
        let started = start_two_node(eligible("u1", "张三"));
        let mut current = started.created_executions[0].clone();
        current.block(ApprovalBlockerCode::DefinitionGraphCorrupted, at(23)).unwrap();

        let error = block_current(started.instance, current, ApprovalBlockerCode::OpenTaskConflict, at(24))
            .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("只有活动执行可以接受决定"));
    }

    // 破坏当前决定连线，验证可形成合法执行快照时以结构阻塞代替接受决定。
    #[test]
    fn engine_decide_blocks_on_corrupted_graph() {
        let started = start_two_node(eligible("u1", "张三"));
        let current = started.created_executions[0].clone();
        let mut graph = two_node_graph();
        graph.transitions.clear();

        let plan = decide(
            started.instance,
            current,
            &graph,
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: participant("u1"),
                current_eligibility: eligible("u1", "张三"),
                next_eligibility: eligible("u2", "李四"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(24),
            },
        )
        .unwrap();
        assert_eq!(plan.commit, CommitRequired::Blocked);
        assert_eq!(
            plan.updated_executions[0].blocker_code,
            Some(ApprovalBlockerCode::DefinitionGraphCorrupted)
        );
    }

    // 验证取消同时结束实例和节点执行，不留下可继续办理的当前令牌。
    #[test]
    fn engine_cancel_clears_current_execution() {
        let started = start_two_node(eligible("u1", "张三"));
        let current = started.created_executions[0].clone();
        let plan = cancel(
            started.instance,
            current,
            CancelCommand {
                actor: participant("starter"),
                reason: "撤回重填".into(),
                close_open_task: true,
                now: at(25),
            },
        )
        .unwrap();
        assert_eq!(plan.commit, CommitRequired::Cancelled);
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Cancelled);
        assert!(plan.instance.current_node_execution_id.is_none());
        assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Cancelled);
    }

    // 验证原人恢复会替换旧执行、为新执行派任务，且不重开旧任务或改写原绑定。
    #[test]
    fn engine_resume_creates_new_execution() {
        let blocked = block_after_start(ApprovalBlockerCode::ApproverEmploymentInvalid);
        let current = blocked.updated_executions[0].clone();
        let assignee = blocked.created_assignees[0].clone();
        let original_assignee = assignee.clone();
        let plan = resume(
            blocked.instance,
            current,
            &assignee,
            &two_node_graph(),
            ResumeCommand {
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                eligibility: eligible("u1", "张三"),
                now: at(26),
            },
        )
        .unwrap();

        assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Superseded);
        assert_eq!(
            plan.updated_executions[0].ended_reason,
            Some(ApprovalExecutionEndReason::AssigneeRecovered)
        );
        assert_eq!(plan.created_executions[0].status, ApprovalNodeExecutionStatus::Active);
        assert_eq!(
            plan.created_executions[0].assignment_source,
            ApprovalExecutionAssignmentSource::AssigneeRecovery
        );
        assert_eq!(plan.created_executions.len(), 1);
        assert_eq!(plan.updated_executions.len(), 1);
        assert_eq!(
            plan.created_executions[0].replaces_execution_id.as_ref(),
            Some(&ApprovalNodeExecutionId::new("e1"))
        );
        assert_eq!(
            plan.created_executions[0].assignee_participant_id,
            assignee.definition_assignee_participant_id
        );

        assert!(plan.created_assignees.is_empty());
        assert_eq!(assignee, original_assignee);
        assert!(assignee.ensure_unchanged_from_definition().is_ok());

        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Running);
        assert!(matches!(plan.task_intents.first(), Some(TaskIntent::HumanTaskRequested { .. })));
        assert_eq!(
            plan.events.iter().map(|event| event.kind.as_str()).collect::<Vec<_>>(),
            ["EXECUTION_SUPERSEDED", "NODE_ENTERED", "ASSIGNEE_RECOVERED"]
        );
    }

    // 验证图或一致性问题不能通过刷新人员资格解除。
    #[test]
    fn engine_resume_rejects_structural_blocker() {
        let started = start_two_node(eligible("u1", "张三"));
        let assignee = started.created_assignees[0].clone();
        let mut instance = started.instance;
        instance.enter_blocked(ApprovalBlockerCode::DefinitionGraphCorrupted, at(27)).unwrap();
        let mut current = started.created_executions[0].clone();
        current.block(ApprovalBlockerCode::DefinitionGraphCorrupted, at(27)).unwrap();

        let error = resume(
            instance,
            current,
            &assignee,
            &two_node_graph(),
            ResumeCommand {
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                eligibility: eligible("u1", "张三"),
                now: at(28),
            },
        )
        .unwrap_err();
        assert_eq!(
            error,
            EngineError::Model(ModelError::InvalidStatus("结构性或一致性阻塞不得恢复原审批人"))
        );
    }

    // 验证资格未恢复时不能生成计划，原实例、执行和绑定保持不变。
    #[test]
    fn engine_resume_rejects_still_ineligible_assignee_without_state_change() {
        let blocked_plan = block_after_start(ApprovalBlockerCode::ApproverAccountInactive);
        let instance = blocked_plan.instance;
        let current = blocked_plan.updated_executions[0].clone();
        let assignee = blocked_plan.created_assignees[0].clone();
        let original_instance = instance.clone();
        let original_current = current.clone();
        let original_assignee = assignee.clone();

        let error = resume(
            instance.clone(),
            current.clone(),
            &assignee,
            &two_node_graph(),
            ResumeCommand {
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                eligibility: blocked("u1", "张三", ApprovalBlockerCode::ApproverAccountInactive),
                now: at(29),
            },
        )
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("原审批人资格尚未恢复"));

        assert_eq!(instance, original_instance);
        assert_eq!(current, original_current);
        assert_eq!(assignee, original_assignee);
    }

    // 篡改冻结责任人或历史变更审计字段，验证恢复不会接纳已漂移的绑定。
    #[test]
    fn engine_resume_rejects_corrupted_definition_binding() {
        let blocked = block_after_start(ApprovalBlockerCode::ApproverAccountInactive);
        let instance = blocked.instance;
        let current = blocked.updated_executions[0].clone();
        let binding = blocked.created_assignees[0].clone();
        let mut corrupted = Vec::new();

        let mut changed_current = binding.clone();
        changed_current.current_assignee_participant_id = participant("u9");
        corrupted.push(changed_current);

        let mut changed_by = binding.clone();
        changed_by.changed_by = Some(participant("operator"));
        corrupted.push(changed_by);

        let mut changed_at = binding.clone();
        changed_at.changed_at = Some(at(28));
        corrupted.push(changed_at);

        let mut changed_reason = binding;
        changed_reason.change_reason = Some("unexpected change".into());
        corrupted.push(changed_reason);

        for (index, assignee) in corrupted.iter().enumerate() {
            let error = resume(
                instance.clone(),
                current.clone(),
                assignee,
                &two_node_graph(),
                ResumeCommand {
                    next_execution_id: ApprovalNodeExecutionId::new(format!("e-corrupted-{index}")),
                    next_execution_no: 2,
                    eligibility: eligible("u1", "张三"),
                    now: at(29),
                },
            )
            .unwrap_err();
            assert_eq!(
                error,
                EngineError::Model(ModelError::InvalidStatus("实例审批人绑定必须保持定义快照不变"))
            );
        }
    }

    // 分别让旧执行责任人、资格身份或轮次不一致，验证恢复不能拼接不同来源的事实。
    #[test]
    fn engine_resume_rejects_mismatched_recovery_facts() {
        let blocked = block_after_start(ApprovalBlockerCode::ApproverAccountInactive);
        let instance = blocked.instance;
        let current = blocked.updated_executions[0].clone();
        let assignee = blocked.created_assignees[0].clone();

        let mut wrong_execution_assignee = current.clone();
        wrong_execution_assignee.assignee_participant_id = participant("u9");
        let error = resume(
            instance.clone(),
            wrong_execution_assignee,
            &assignee,
            &two_node_graph(),
            ResumeCommand {
                next_execution_id: ApprovalNodeExecutionId::new("e-wrong-current"),
                next_execution_no: 2,
                eligibility: eligible("u1", "张三"),
                now: at(29),
            },
        )
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("原审批人与旧受阻执行不一致"));

        let error = resume(
            instance.clone(),
            current.clone(),
            &assignee,
            &two_node_graph(),
            ResumeCommand {
                next_execution_id: ApprovalNodeExecutionId::new("e-wrong-eligibility"),
                next_execution_no: 2,
                eligibility: eligible("u9", "钱七"),
                now: at(29),
            },
        )
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("资格结果必须属于原审批人"));

        let mut wrong_round = current;
        wrong_round.round_no = 2;
        let error = resume(
            instance,
            wrong_round,
            &assignee,
            &two_node_graph(),
            ResumeCommand {
                next_execution_id: ApprovalNodeExecutionId::new("e-wrong-round"),
                next_execution_no: 2,
                eligibility: eligible("u1", "张三"),
                now: at(29),
            },
        )
        .unwrap_err();
        assert_eq!(error, EngineError::InvalidCommand("受阻执行不属于实例当前轮次"));
    }

    // 验证目标节点缺失时不会编造节点信息来写入阻塞执行。
    #[test]
    fn engine_enter_missing_node_is_uncommittable() {
        let graph = two_node_graph();
        for (node_key, message) in [
            ("missing", "定义图缺少目标节点，无法形成合法阻塞快照"),
            (" \t", "缺失节点键，无法形成合法阻塞快照"),
        ] {
            let error = plan_enter_node(EnterNodeInput {
                instance: running_instance(),
                graph: &graph,
                node_key,
                round_no: 1,
                participant: participant("u1"),
                eligibility: eligible("u1", "张三"),
                execution_id: ApprovalNodeExecutionId::new("e9"),
                execution_no: 1,
                assignment_source: ApprovalExecutionAssignmentSource::Definition,
                replaces_execution_id: None,
                now: at(30),
            })
            .unwrap_err();
            assert_eq!(error, EngineError::Uncommittable(message));
        }
    }

    #[test]
    fn engine_enter_checks_resolved_assignee_then_identity_then_group_before_execution() {
        let graph = two_node_graph();
        let mut unresolved = graph.clone();
        unresolved.nodes[0].assignee_participant_id = None;
        let duplicate_group = Eligibility::AnyOf {
            primary: Box::new(eligible("u1", "张三")),
            additional: vec![eligible("u1", "张三")],
        };

        for (graph, user, eligibility, expected) in [
            (
                unresolved,
                "u9",
                eligible("u9", ""),
                EngineError::Model(ModelError::InvalidField("审批人来源尚未解析")),
            ),
            (
                graph.clone(),
                "u9",
                eligible("u1", ""),
                EngineError::InvalidCommand("节点责任人必须匹配定义审批人"),
            ),
            (
                graph.clone(),
                "u1",
                eligible("u9", ""),
                EngineError::InvalidCommand("节点责任人必须匹配定义审批人"),
            ),
            (graph, "u1", duplicate_group, EngineError::InvalidCommand("资格结果必须属于定义审批人")),
        ] {
            let error = plan_enter_node(EnterNodeInput {
                instance: running_instance(),
                graph: &graph,
                node_key: "n1",
                round_no: 1,
                participant: participant(user),
                eligibility,
                execution_id: ApprovalNodeExecutionId::new("e9"),
                execution_no: 0,
                assignment_source: ApprovalExecutionAssignmentSource::Definition,
                replaces_execution_id: None,
                now: at(30),
            })
            .unwrap_err();
            assert_eq!(error, expected);
        }
    }

    // 检查计划不混入路由、权限或通知细节，维持 BPM 到应用适配层的边界。
    #[test]
    fn engine_plan_has_no_erp_payload() {
        let plan = start_two_node(eligible("u1", "张三"));
        let debug = format!("{plan:?}");
        assert!(!debug.contains("http"));
        assert!(!debug.contains("permission"));
        assert!(!debug.contains("sales_order:detail"));
        assert!(!debug.contains("notification"));
    }

    fn block_after_start(code: ApprovalBlockerCode) -> super::TransitionPlan {
        let started = start_two_node(eligible("u1", "张三"));
        let assignees = started.created_assignees.clone();
        let current = started.created_executions[0].clone();
        let mut blocked = decide(
            started.instance,
            current,
            &two_node_graph(),
            DecideCommand {
                decision: ApprovalDecision::Approve,
                reason: None,
                actor: participant("u1"),
                current_eligibility: blocked("u1", "张三", code),
                next_eligibility: eligible("u2", "李四"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(15),
            },
        )
        .unwrap();
        blocked.created_assignees = assignees;
        blocked
    }

    fn start_two_node(entry: Eligibility) -> super::TransitionPlan {
        try_start_two_node(entry, eligible("u2", "李四")).unwrap()
    }

    fn try_start_two_node(
        entry: Eligibility,
        second: Eligibility,
    ) -> Result<super::TransitionPlan, EngineError> {
        start(
            StartCommand {
                instance_id: ApprovalProcessInstanceId::new("inst"),
                process_kind: ProcessKind::StockAdjustment,
                subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
                subject_version: 1,
                started_by: participant("starter"),
                entry_execution_id: ApprovalNodeExecutionId::new("e1"),
                now: at(10),
            },
            &two_node_graph(),
            &[
                StartAssigneeBinding {
                    id: ApprovalInstanceAssigneeId::new("a1"),
                    node_key: "n1".into(),
                    participant: participant("u1"),
                    eligibility: entry,
                },
                StartAssigneeBinding {
                    id: ApprovalInstanceAssigneeId::new("a2"),
                    node_key: "n2".into(),
                    participant: participant("u2"),
                    eligibility: second,
                },
            ],
        )
    }

    fn start_single_node(entry: Eligibility) -> super::TransitionPlan {
        start(
            StartCommand {
                instance_id: ApprovalProcessInstanceId::new("inst"),
                process_kind: ProcessKind::StockAdjustment,
                subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
                subject_version: 1,
                started_by: participant("starter"),
                entry_execution_id: ApprovalNodeExecutionId::new("e1"),
                now: at(10),
            },
            &single_node_graph(),
            &[StartAssigneeBinding {
                id: ApprovalInstanceAssigneeId::new("a1"),
                node_key: "n1".into(),
                participant: participant("u1"),
                eligibility: entry,
            }],
        )
        .unwrap()
    }

    fn two_node_graph() -> DefinitionGraph {
        let at = at(1);
        let definition = ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def"),
            ProcessKind::StockAdjustment,
            1,
            "库存调整",
            "n1",
            participant("admin"),
            at,
        )
        .unwrap();
        let n1 = node("nd1", "n1", "仓储复核", 1, "u1", "张三", at);
        let n2 = node("nd2", "n2", "财务复核", 2, "u2", "李四", at);
        let transitions = vec![
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new("t1"),
                ApprovalProcessDefinitionId::new("def"),
                "n1",
                ApprovalTransitionEvent::Approve,
                "n2",
                at,
            )
            .unwrap(),
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new("t2"),
                ApprovalProcessDefinitionId::new("def"),
                "n1",
                ApprovalTransitionEvent::Reject,
                "n1",
                at,
            )
            .unwrap(),
            ApprovalTransitionDefinition::to_approved(
                ApprovalTransitionDefinitionId::new("t3"),
                ApprovalProcessDefinitionId::new("def"),
                "n2",
                ApprovalTransitionEvent::Approve,
                at,
            )
            .unwrap(),
            ApprovalTransitionDefinition::to_node(
                ApprovalTransitionDefinitionId::new("t4"),
                ApprovalProcessDefinitionId::new("def"),
                "n2",
                ApprovalTransitionEvent::Reject,
                "n1",
                at,
            )
            .unwrap(),
        ];
        DefinitionGraph { definition, nodes: vec![n1, n2], transitions }
    }

    fn single_node_graph() -> DefinitionGraph {
        let at = at(1);
        let definition = ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def"),
            ProcessKind::StockAdjustment,
            1,
            "库存调整",
            "n1",
            participant("admin"),
            at,
        )
        .unwrap();
        DefinitionGraph {
            definition,
            nodes: vec![node("nd1", "n1", "仓储复核", 1, "u1", "张三", at)],
            transitions: vec![
                ApprovalTransitionDefinition::to_approved(
                    ApprovalTransitionDefinitionId::new("t1"),
                    ApprovalProcessDefinitionId::new("def"),
                    "n1",
                    ApprovalTransitionEvent::Approve,
                    at,
                )
                .unwrap(),
                ApprovalTransitionDefinition::to_node(
                    ApprovalTransitionDefinitionId::new("t2"),
                    ApprovalProcessDefinitionId::new("def"),
                    "n1",
                    ApprovalTransitionEvent::Reject,
                    "n1",
                    at,
                )
                .unwrap(),
            ],
        }
    }

    fn running_instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(crate::model::NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst"),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            subject_version: 1,
            started_by: participant("starter"),
            at: at(10),
        })
        .unwrap()
    }
}

#[cfg(test)]
mod or_sign_tests {
    //! 或签执行的完整状态与兼容性回归。
    use super::test_fixtures::{at, blocked, eligible, node, participant};
    use super::{
        CommitRequired, DecideCommand, DefinitionGraph, Eligibility, StartAssigneeBinding, StartCommand,
        TaskIntent, decide, reject_terminal, start,
    };
    use crate::graph::build_linear_transitions;
    use crate::ids::{
        ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId,
        ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
    };
    use crate::model::types::{
        ApprovalBlockerCode, ApprovalDecision, ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus,
    };
    use crate::model::{
        ApprovalNodeDefinition, ApprovalParticipant, ApprovalProcessDefinition, ProcessKind, SubjectRef,
    };

    fn graph() -> DefinitionGraph {
        let definition = ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def"),
            ProcessKind::StockAdjustment,
            1,
            "或签",
            "n1",
            participant("admin"),
            at(1),
        )
        .unwrap();
        let nodes = vec![
            node("nd1", "n1", "复核", 1, "u1", "张三", at(1))
                .with_additional_assignees(vec![ApprovalParticipant::new(participant("u2"), "李四").unwrap()])
                .unwrap(),
            node("nd2", "n2", "终审", 2, "u3", "王五", at(1)),
        ];
        let transitions = build_linear_transitions(
            &ApprovalProcessDefinitionId::new("def"),
            &nodes,
            (1..=4).map(|i| ApprovalTransitionDefinitionId::new(format!("t{i}"))).collect(),
            at(1),
        )
        .unwrap();
        DefinitionGraph { definition, nodes, transitions }
    }

    fn group() -> Eligibility {
        Eligibility::any_of(vec![eligible("u1", "张三"), eligible("u2", "李四")]).unwrap()
    }

    #[test]
    fn node_eligibility_keeps_exact_member_order_and_rejects_nested_groups() {
        let graph = graph();
        let node = &graph.nodes[0];
        group().validate_node(node).unwrap();
        Eligibility::any_of(vec![eligible("u1", "更新姓名"), eligible("u2", "另一姓名")])
            .unwrap()
            .validate_node(node)
            .unwrap();

        let invalid = [
            eligible("u1", "张三"),
            Eligibility::any_of(vec![
                eligible("u1", "张三"),
                eligible("u2", "李四"),
                eligible("u3", "额外人员"),
            ])
            .unwrap(),
            Eligibility::any_of(vec![eligible("u2", "李四"), eligible("u1", "张三")]).unwrap(),
            Eligibility::any_of(vec![eligible("u1", "张三"), eligible("u3", "错误人员")]).unwrap(),
            Eligibility::AnyOf { primary: Box::new(group()), additional: vec![eligible("u2", "李四")] },
        ];
        for eligibility in invalid {
            assert_eq!(
                eligibility.validate_node(node),
                Err(super::EngineError::InvalidCommand("资格结果必须属于定义审批人"))
            );
        }
    }

    fn started(graph: &DefinitionGraph) -> super::TransitionPlan {
        start(
            StartCommand {
                instance_id: ApprovalProcessInstanceId::new("i1"),
                process_kind: ProcessKind::StockAdjustment,
                subject: SubjectRef::new("stock_adjustment", "s1").unwrap(),
                subject_version: 1,
                started_by: participant("starter"),
                entry_execution_id: ApprovalNodeExecutionId::new("e1"),
                now: at(10),
            },
            graph,
            &[
                StartAssigneeBinding {
                    id: ApprovalInstanceAssigneeId::new("a1"),
                    node_key: "n1".into(),
                    participant: participant("u1"),
                    eligibility: group(),
                },
                StartAssigneeBinding {
                    id: ApprovalInstanceAssigneeId::new("a2"),
                    node_key: "n2".into(),
                    participant: participant("u3"),
                    eligibility: Eligibility::any_of(
                        graph.nodes[1]
                            .participants()
                            .iter()
                            .map(|person| eligible(person.participant_id.as_str(), &person.name))
                            .collect(),
                    )
                    .unwrap(),
                },
            ],
        )
        .unwrap()
    }
    fn command(actor: &str, decision: ApprovalDecision) -> DecideCommand {
        DecideCommand {
            decision,
            actor: participant(actor),
            reason: Some("已核对".into()),
            current_eligibility: eligible(actor, "决定人"),
            next_eligibility: if decision == ApprovalDecision::Reject {
                group()
            } else {
                eligible("u3", "王五")
            },
            next_execution_id: ApprovalNodeExecutionId::new("e2"),
            next_execution_no: 2,
            now: at(20),
        }
    }

    #[test]
    fn any_member_can_approve_once_and_next_node_has_one_execution() {
        for actor in ["u1", "u2"] {
            let graph = graph();
            let start = started(&graph);
            assert_eq!(start.created_executions.len(), 1);
            let owners = start
                .task_intents
                .iter()
                .filter_map(|task| {
                    if let TaskIntent::HumanTaskRequested { assignee, .. } = task {
                        Some(assignee.as_str())
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();
            assert_eq!(owners, ["u1", "u2"]);
            assert_eq!(start.created_assignees[0].additional_assignees, graph.nodes[0].additional_assignees);

            let plan = decide(
                start.instance,
                start.created_executions[0].clone(),
                &graph,
                command(actor, ApprovalDecision::Approve),
            )
            .unwrap();
            assert_eq!(plan.updated_executions[0].decided_by, Some(participant(actor)));
            assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Approved);
            assert_eq!(plan.created_executions.len(), 1);
            assert_eq!(plan.created_executions[0].node_key, "n2");
            assert_eq!(plan.task_intents.len(), 2);

            assert!(
                decide(
                    plan.instance,
                    plan.updated_executions[0].clone(),
                    &graph,
                    command("u2", ApprovalDecision::Approve)
                )
                .is_err()
            );
        }
    }

    #[test]
    fn reject_always_terminates_group_without_new_tasks() {
        let graph = graph();
        let start = started(&graph);
        let plan = decide(
            start.instance.clone(),
            start.created_executions[0].clone(),
            &graph,
            command("u2", ApprovalDecision::Reject),
        )
        .unwrap();
        assert_eq!(plan.instance.current_round_no, 1);
        assert!(plan.created_executions.is_empty());
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Rejected);
        assert!(matches!(plan.task_intents.as_slice(), [TaskIntent::CompleteTask { .. }]));

        let terminal = reject_terminal(
            start.instance,
            start.created_executions[0].clone(),
            &graph,
            command("u2", ApprovalDecision::Reject),
        )
        .unwrap();
        assert_eq!(terminal.instance.status, ApprovalProcessInstanceStatus::Rejected);
        assert!(terminal.created_executions.is_empty());
    }

    #[test]
    fn outsider_mismatched_eligibility_and_disabled_member_cannot_decide() {
        let graph = graph();
        let start = started(&graph);
        let outsider = command("u9", ApprovalDecision::Approve);
        let mut mismatched = command("u2", ApprovalDecision::Approve);
        mismatched.current_eligibility = eligible("u1", "张三");
        let mut inactive = command("u2", ApprovalDecision::Approve);
        inactive.current_eligibility = blocked("u2", "李四", ApprovalBlockerCode::ApproverAccountInactive);

        for cmd in [outsider, mismatched, inactive] {
            assert!(
                decide(start.instance.clone(), start.created_executions[0].clone(), &graph, cmd).is_err()
            );
        }
        assert_eq!(start.instance.status, ApprovalProcessInstanceStatus::Running);
    }

    #[test]
    fn next_group_can_run_with_one_eligible_member_and_blocks_when_all_fail() {
        let mut graph = graph();
        graph.nodes[1] = graph.nodes[1]
            .clone()
            .with_additional_assignees(vec![ApprovalParticipant::new(participant("u4"), "赵六").unwrap()])
            .unwrap();
        // 当前令牌的冻结定义仅增加尚未进入节点的测试候选人。
        let start = started(&self::graph());
        for all_blocked in [false, true] {
            let mut cmd = command("u2", ApprovalDecision::Approve);
            cmd.next_eligibility = Eligibility::any_of(vec![
                blocked("u3", "王五", ApprovalBlockerCode::ApproverAccountInactive),
                if all_blocked {
                    blocked("u4", "赵六", ApprovalBlockerCode::ApproverAccountInactive)
                } else {
                    eligible("u4", "赵六")
                },
            ])
            .unwrap();
            let plan =
                decide(start.instance.clone(), start.created_executions[0].clone(), &graph, cmd).unwrap();

            assert_eq!(plan.commit, CommitRequired::Proceed);
            assert_eq!(plan.updated_executions[0].status, ApprovalNodeExecutionStatus::Approved);
            assert_eq!(
                plan.created_executions[0].status,
                if all_blocked {
                    ApprovalNodeExecutionStatus::Blocked
                } else {
                    ApprovalNodeExecutionStatus::Active
                }
            );
            assert_eq!(plan.task_intents.len(), if all_blocked { 1 } else { 2 });
        }
    }

    #[test]
    fn group_blocker_requires_all_members_and_uses_primary_code() {
        let primary_code = ApprovalBlockerCode::ApproverAccountInactive;
        let secondary_code = ApprovalBlockerCode::ApproverNotEligible;
        let all_blocked = Eligibility::any_of(vec![
            blocked("u1", "张三", primary_code),
            blocked("u2", "李四", secondary_code),
        ])
        .unwrap();
        assert_eq!(all_blocked.blocked_code(), Some(primary_code));

        for members in [
            vec![eligible("u1", "张三"), blocked("u2", "李四", secondary_code)],
            vec![blocked("u1", "张三", primary_code), eligible("u2", "李四")],
            vec![eligible("u1", "张三"), eligible("u2", "李四")],
        ] {
            assert_eq!(Eligibility::any_of(members).unwrap().blocked_code(), None);
        }

        let single = Eligibility::any_of(vec![blocked("u1", "张三", primary_code)]).unwrap();
        assert_eq!(single.blocked_code(), Some(primary_code));
    }

    #[test]
    fn legacy_node_and_execution_deserialize_as_single_person_and_duplicates_fail() {
        let node = graph().nodes.remove(0);
        let mut legacy = serde_json::to_value(&node).unwrap();
        legacy.as_object_mut().unwrap().remove("additional_assignees");
        let parsed: ApprovalNodeDefinition = serde_json::from_value(legacy).unwrap();
        assert_eq!(parsed.participants().len(), 1);
        assert!(
            parsed
                .with_additional_assignees(vec![ApprovalParticipant::new(participant("u1"), "重复").unwrap()])
                .is_err()
        );

        assert!(Eligibility::any_of(vec![]).is_err());
        assert!(Eligibility::any_of(vec![eligible("u1", "张三"), eligible("u1", "重复")]).is_err());
        assert!(Eligibility::any_of(vec![group()]).is_err());
    }

    #[test]
    fn blocked_or_sign_resumes_only_effective_members_and_can_finish() {
        let mut graph = graph();
        graph.nodes[1] = graph.nodes[1]
            .clone()
            .with_additional_assignees(vec![ApprovalParticipant::new(participant("u4"), "赵六").unwrap()])
            .unwrap();

        let start = started(&graph);
        let binding = start.created_assignees[1].clone();
        let mut cmd = command("u2", ApprovalDecision::Approve);
        cmd.next_eligibility = Eligibility::any_of(vec![
            blocked("u3", "王五", ApprovalBlockerCode::ApproverAccountInactive),
            blocked("u4", "赵六", ApprovalBlockerCode::ApproverAccountInactive),
        ])
        .unwrap();
        let waiting = decide(start.instance, start.created_executions[0].clone(), &graph, cmd).unwrap();

        let resumed = super::resume(
            waiting.instance,
            waiting.created_executions[0].clone(),
            &binding,
            &graph,
            super::ResumeCommand {
                eligibility: Eligibility::any_of(vec![
                    blocked("u3", "王五", ApprovalBlockerCode::ApproverAccountInactive),
                    eligible("u4", "赵六"),
                ])
                .unwrap(),
                next_execution_id: ApprovalNodeExecutionId::new("e3"),
                next_execution_no: 3,
                now: at(30),
            },
        )
        .unwrap();
        assert_eq!(resumed.updated_executions[0].status, ApprovalNodeExecutionStatus::Superseded);
        assert_eq!(resumed.task_intents.len(), 1);
        assert!(
            matches!(&resumed.task_intents[0], TaskIntent::HumanTaskRequested { assignee, .. } if assignee.as_str() == "u4")
        );

        let mut final_command = command("u4", ApprovalDecision::Approve);
        final_command.now = at(40);
        let finished =
            decide(resumed.instance, resumed.created_executions[0].clone(), &graph, final_command).unwrap();
        assert_eq!(finished.instance.status, ApprovalProcessInstanceStatus::Approved);
        assert!(finished.created_executions.is_empty());
        assert_eq!(finished.updated_executions[0].assignee_display_name(), "赵六");
    }
}
