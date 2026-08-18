//! BPM 纯状态引擎：节点、事件、连线上的单令牌计算。
//!
//! 只接收 BPM 模型、命令值对象、已收敛资格结果和调用方提供的时间/ID。
//! 不得读取时钟、生成 ID、打开事务或访问 Repository。

mod cancel;
mod decision;
mod enter_node;
mod event;
mod reassign;
mod resume;
mod start;
mod transition_plan;

pub use cancel::{cancel, CancelCommand};
pub use decision::{decide, DecideCommand};
pub use enter_node::plan_enter_node;
pub use event::{BpmEvent, BpmEventKind};
pub use reassign::{reassign, ReassignCommand};
pub use resume::{resume, ResumeCommand};
pub use start::{start, StartAssigneeBinding, StartCommand};
pub use transition_plan::{CommitRequired, TaskCloseReason, TaskIntent, TransitionPlan};

use crate::error::{Error, Result};
use crate::model::types::{ApprovalBlockerCode, ModelError};
use crate::model::{
    ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalTransitionDefinition, ParticipantId,
};

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
    /// 将模型错误提升为引擎错误。
    fn from(error: ModelError) -> Self {
        Self::Model(error)
    }
}

/// 引擎计算结果。
pub type EngineResult<T> = std::result::Result<T, EngineError>;

/// 调用方已收敛的人员资格。BPM 不再访问账号或权限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Eligibility {
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
    /// 返回资格对应的显示名。
    ///
    /// # 返回
    /// 返回构造时写入的快照。
    pub fn name_snapshot(&self) -> &str {
        match self {
            Self::Eligible {
                assignee_name_snapshot,
                ..
            }
            | Self::Blocked {
                assignee_name_snapshot,
                ..
            } => assignee_name_snapshot,
        }
    }

    /// 返回人员失效阻塞码。
    ///
    /// # 返回
    /// 有效时返回 `None`。
    pub fn blocked_code(&self) -> Option<ApprovalBlockerCode> {
        match self {
            Self::Eligible { .. } => None,
            Self::Blocked { code, .. } => Some(*code),
        }
    }

    /// 返回资格对应的责任人。
    ///
    /// # 返回
    /// 返回构造时写入的处理人。
    pub fn participant(&self) -> ParticipantId {
        match self {
            Self::Eligible { participant, .. } | Self::Blocked { participant, .. } => participant.clone(),
        }
    }
}

/// 引擎使用的定义图。不引用 database 类型。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionGraph {
    /// 流程定义。
    pub definition: ApprovalProcessDefinition,
    /// 节点。
    pub nodes: Vec<ApprovalNodeDefinition>,
    /// 连线。
    pub transitions: Vec<ApprovalTransitionDefinition>,
}

impl DefinitionGraph {
    /// 按节点键查找节点。
    ///
    /// # 参数
    /// * `node_key` - 节点键
    ///
    /// # 返回
    /// 命中时返回节点。
    pub fn node(&self, node_key: &str) -> Option<&ApprovalNodeDefinition> {
        self.nodes.iter().find(|item| item.node_key == node_key)
    }

    /// 返回入口节点。
    ///
    /// # 错误
    /// 入口不存在时返回图损坏。
    pub fn entry_node(&self) -> EngineResult<&ApprovalNodeDefinition> {
        self.node(&self.definition.entry_node_key)
            .ok_or(EngineError::GraphCorrupted)
    }
}

/// 拒绝调用尚未接线的引擎入口。
///
/// # 错误
/// 始终返回 [`Error::NotWired`]，不得产生迁移计划或领域事件。
pub fn refuse_unwired() -> Result<TransitionPlan> {
    Err(Error::NotWired)
}

#[cfg(test)]
mod tests {
    use super::{
        cancel, decide, plan_enter_node, reassign, refuse_unwired, resume, start, CancelCommand,
        CommitRequired, DecideCommand, DefinitionGraph, Eligibility, EngineError, ReassignCommand,
        ResumeCommand, StartAssigneeBinding, StartCommand, TaskCloseReason, TaskIntent,
    };
    use crate::error::Error;
    use crate::ids::{
        ApprovalInstanceAssigneeId, ApprovalNodeDefinitionId, ApprovalNodeExecutionId,
        ApprovalProcessDefinitionId, ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
    };
    use crate::model::types::{
        ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource, ApprovalExecutionEndReason,
        ApprovalNodeExecutionStatus, ApprovalProcessInstanceStatus, ApprovalTransitionEvent,
    };
    use crate::model::{
        ApprovalNodeDefinition, ApprovalProcessDefinition, ApprovalProcessInstance,
        ApprovalTransitionDefinition, ParticipantId, ProcessKind, SubjectRef, Timestamp,
    };

    /// 引擎占位必须失败关闭。
    #[test]
    fn engine_placeholder_fails_closed() {
        assert_eq!(refuse_unwired(), Err(Error::NotWired));
    }

    /// 相同启动输入产生相同计划语义。
    #[test]
    fn engine_start_is_deterministic() {
        let first = start_two_node(eligible("u1", "张三"));
        let second = start_two_node(eligible("u1", "张三"));
        assert_eq!(first, second);
        assert_eq!(first.instance.status, ApprovalProcessInstanceStatus::Running);
        assert_eq!(first.created_assignees.len(), 2);
        assert!(matches!(
            first.task_intents.first(),
            Some(TaskIntent::HumanTaskRequested { .. })
        ));
        assert!(first
            .events
            .iter()
            .any(|event| event.kind.as_str() == "INSTANCE_STARTED"));
    }

    /// 入口人员失效时启动仍提交 BLOCKED 执行且不产生任务。
    #[test]
    fn engine_start_blocks_when_entry_ineligible() {
        let plan = start_two_node(blocked(
            "u1",
            "张三",
            ApprovalBlockerCode::ApproverAccountInactive,
        ));
        assert_eq!(plan.commit, CommitRequired::Blocked);
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Blocked);
        assert!(plan.task_intents.is_empty());
        assert_eq!(
            plan.created_executions[0].status,
            ApprovalNodeExecutionStatus::Blocked
        );
    }

    /// 通过进入下一节点；下一审批人失效时保留本次通过。
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
        assert_eq!(
            plan.updated_executions[0].status,
            ApprovalNodeExecutionStatus::Approved
        );
        assert_eq!(
            plan.created_executions[0].status,
            ApprovalNodeExecutionStatus::Blocked
        );
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Blocked);
        assert!(!plan
            .task_intents
            .iter()
            .any(|intent| matches!(intent, TaskIntent::HumanTaskRequested { .. })));
    }

    /// 末节点通过生成待终结计划。
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

    /// 任一节点驳回都进入下一轮入口，subject_version 不变。
    #[test]
    fn engine_reject_restarts_entry_round() {
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
        assert_eq!(plan.instance.current_round_no, 2);
        assert_eq!(plan.instance.subject_version, subject_version);
        assert_eq!(
            plan.updated_executions[0].status,
            ApprovalNodeExecutionStatus::Rejected
        );
        assert_eq!(plan.created_executions[0].node_key, "n1");
        assert_eq!(plan.created_executions[0].round_no, 2);
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Running);
    }

    /// 当前审批人失效提交 BLOCKED 事实并关闭任务，而不是返回不可提交错误。
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
        assert_eq!(
            plan.updated_executions[0].status,
            ApprovalNodeExecutionStatus::Blocked
        );
        assert!(matches!(
            plan.task_intents.first(),
            Some(TaskIntent::CloseTask {
                reason: TaskCloseReason::ApprovalRuntimeBlocked,
                ..
            })
        ));
    }

    /// 图损坏且能形成合法快照时提交结构阻塞。
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

    /// 取消清空当前执行引用。
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
        assert_eq!(
            plan.updated_executions[0].status,
            ApprovalNodeExecutionStatus::Cancelled
        );
    }

    /// 恢复创建新执行和新任务，旧任务保持关闭。
    #[test]
    fn engine_resume_creates_new_execution() {
        let blocked = start_two_node(blocked(
            "u1",
            "张三",
            ApprovalBlockerCode::ApproverEmploymentInvalid,
        ));
        let current = blocked.created_executions[0].clone();
        let assignee = blocked.created_assignees[0].clone();
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
        assert_eq!(
            plan.updated_executions[0].status,
            ApprovalNodeExecutionStatus::Superseded
        );
        assert_eq!(
            plan.updated_executions[0].ended_reason,
            Some(ApprovalExecutionEndReason::AssigneeRecovered)
        );
        assert_eq!(
            plan.created_executions[0].status,
            ApprovalNodeExecutionStatus::Active
        );
        assert_eq!(
            plan.created_executions[0].assignment_source,
            ApprovalExecutionAssignmentSource::AssigneeRecovery
        );
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Running);
        assert!(matches!(
            plan.task_intents.first(),
            Some(TaskIntent::HumanTaskRequested { .. })
        ));
    }

    /// 结构性阻塞不得改派。
    #[test]
    fn engine_reassign_rejects_structural_blocker() {
        let started = start_two_node(eligible("u1", "张三"));
        let mut instance = started.instance;
        instance
            .enter_blocked(ApprovalBlockerCode::DefinitionGraphCorrupted, at(27))
            .unwrap();
        let mut current = started.created_executions[0].clone();
        current
            .block(ApprovalBlockerCode::DefinitionGraphCorrupted, at(27))
            .unwrap();
        let error = reassign(
            instance,
            current,
            started.created_assignees[0].clone(),
            &two_node_graph(),
            ReassignCommand {
                target: participant("u9"),
                actor: participant("admin"),
                reason: "换人".into(),
                target_eligibility: eligible("u9", "钱七"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(28),
            },
        )
        .unwrap_err();
        assert!(matches!(
            error,
            EngineError::Model(_) | EngineError::InvalidCommand(_)
        ));
    }

    /// 管理员改派结束旧执行并创建新执行。
    #[test]
    fn engine_reassign_replaces_blocked_execution() {
        let blocked = start_two_node(blocked(
            "u1",
            "张三",
            ApprovalBlockerCode::ApproverAccountInactive,
        ));
        let plan = reassign(
            blocked.instance,
            blocked.created_executions[0].clone(),
            blocked.created_assignees[0].clone(),
            &two_node_graph(),
            ReassignCommand {
                target: participant("u9"),
                actor: participant("admin"),
                reason: "原审批人离职".into(),
                target_eligibility: eligible("u9", "钱七"),
                next_execution_id: ApprovalNodeExecutionId::new("e2"),
                next_execution_no: 2,
                now: at(29),
            },
        )
        .unwrap();
        assert_eq!(
            plan.updated_executions[0].ended_reason,
            Some(ApprovalExecutionEndReason::AdminReassigned)
        );
        assert_eq!(
            plan.created_executions[0].assignment_source,
            ApprovalExecutionAssignmentSource::AdminReassign
        );
        assert_eq!(
            plan.updated_assignees[0].current_assignee_participant_id.as_str(),
            "u9"
        );
        assert_eq!(
            plan.updated_assignees[0]
                .definition_assignee_participant_id
                .as_str(),
            "u1"
        );
        assert_eq!(plan.instance.status, ApprovalProcessInstanceStatus::Running);
    }

    /// 缺失节点不得构造缺字段执行。
    #[test]
    fn engine_enter_missing_node_is_uncommittable() {
        let instance = running_instance();
        let error = plan_enter_node(
            instance,
            &two_node_graph(),
            "missing",
            1,
            participant("u1"),
            eligible("u1", "张三"),
            ApprovalNodeExecutionId::new("e9"),
            1,
            ApprovalExecutionAssignmentSource::Definition,
            None,
            at(30),
        )
        .unwrap_err();
        assert!(matches!(error, EngineError::Uncommittable(_)));
    }

    /// 计划不得包含 ERP 语义字段名。
    #[test]
    fn engine_plan_has_no_erp_payload() {
        let plan = start_two_node(eligible("u1", "张三"));
        let debug = format!("{plan:?}");
        assert!(!debug.contains("http"));
        assert!(!debug.contains("permission"));
        assert!(!debug.contains("sales_order:detail"));
        assert!(!debug.contains("notification"));
    }

    fn start_two_node(entry: Eligibility) -> super::TransitionPlan {
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
                    eligibility: eligible("u2", "李四"),
                },
            ],
        )
        .unwrap()
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
        DefinitionGraph {
            definition,
            nodes: vec![n1, n2],
            transitions,
        }
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

    fn node(
        id: &str,
        key: &str,
        name: &str,
        order: u32,
        user: &str,
        label: &str,
        at: Timestamp,
    ) -> ApprovalNodeDefinition {
        ApprovalNodeDefinition::new(
            ApprovalNodeDefinitionId::new(id),
            ApprovalProcessDefinitionId::new("def"),
            key,
            name,
            None,
            order,
            participant(user),
            label,
            at,
        )
        .unwrap()
    }

    fn running_instance() -> ApprovalProcessInstance {
        ApprovalProcessInstance::start_running(
            ApprovalProcessInstanceId::new("inst"),
            ApprovalProcessDefinitionId::new("def"),
            1,
            ProcessKind::StockAdjustment,
            SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            1,
            participant("starter"),
            at(10),
        )
        .unwrap()
    }

    fn eligible(user: &str, name: &str) -> Eligibility {
        Eligibility::Eligible {
            participant: participant(user),
            assignee_name_snapshot: name.into(),
        }
    }

    fn blocked(user: &str, name: &str, code: ApprovalBlockerCode) -> Eligibility {
        Eligibility::Blocked {
            participant: participant(user),
            code,
            assignee_name_snapshot: name.into(),
        }
    }

    fn participant(id: &str) -> ParticipantId {
        ParticipantId::new(id).unwrap()
    }

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix_secs(secs).unwrap()
    }
}
