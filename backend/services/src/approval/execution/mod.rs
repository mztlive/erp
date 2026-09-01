//! 审批运行编排。唯一目标应用入口。
//!
//! 负责加载持久化事实、写时授权重验、调用 BPM、映射 WorkItem/审计/通知，
//! 并在一个 MongoDB 事务中应用全部写入。本模块单元测试覆盖纯编排合同。

pub mod apply_plan;
pub mod authorization;
pub mod cancel;
pub mod decision;
pub mod idempotency;
pub mod notification_outbox;
pub mod notification_worker;
pub mod observability;
pub mod resume;
pub mod runtime_history;
pub mod runtime_query;
pub mod runtime_service;
pub mod start;
pub mod store;
pub mod view;

use bpm::engine::{DefinitionGraph, Eligibility};
use bpm::model::{ApprovalCommandReceipt, IdempotencyKey, Timestamp};

use crate::errors::{Error, Result};

pub use apply_plan::{apply_plan, DomainActionKind, PlannedWrites};
pub use authorization::{converge_eligibility, AuthorizationFailure};
pub use cancel::{
    claim_and_persist_document_cancel_runtime, find_document_cancel_receipt,
    normalize_document_cancel_reason, prepare_cancel, prepare_document_cancel,
    replay_committed_document_cancel, CancelExecutionInput, DocumentCancelCommand, DocumentCancelReplayProof,
};
pub use decision::{prepare_decision, DecisionExecutionInput};
pub(crate) use idempotency::{
    command_may_have_committed, command_recovery_delay, map_receipt_first_write_error,
};
pub use idempotency::{
    legacy_standard_start_receipt_identity, legacy_start_receipt_identity, specialized_start_identity,
    upgrade_binding_identity, LegacyReceiptIdentity, PreparedCommandIdentity,
};
pub use notification_worker::ApprovalNotificationOutboxPort;
pub use resume::{prepare_resume, ResumeExecutionInput};
pub use runtime_history::{
    history_item_from_execution, history_page_from, latest_rejection_reason, RuntimeHistoryItem,
    RuntimeHistoryPage,
};
pub use runtime_query::{
    recovery_options_for, RuntimeInstanceListView, RuntimeInstanceStatusFilter, RuntimeRecoveryAction,
};
pub use runtime_service::{
    ApprovalRuntimeService, RuntimeAssigneeCandidate, RuntimeInstanceListCursor, RuntimeInstanceListItem,
    RuntimeInstanceListPage, RuntimeInstanceListQuery, RuntimeRecoveryOptionsView, UpgradeBindingCommand,
};
pub use start::{prepare_start, prepare_start_with_identity, StartExecutionInput};
pub use store::{commit_writes, replay_after_duplicate, MemoryRuntimeStore, TaskApplyContext};
pub use view::{map_command_view, ApprovalCommandOutcome, ApprovalCommandView, OpenTaskSummary};

/// 各命令共用的图、资格、收据与时间。
#[derive(Debug, Clone)]
pub struct ExecutionCommandInput {
    /// 定义图。
    pub graph: DefinitionGraph,
    /// 当前责任人资格。
    pub current_eligibility: Eligibility,
    /// 下一责任人资格。
    pub next_eligibility: Eligibility,
    /// 已存在收据。
    pub receipt: Option<ApprovalCommandReceipt>,
    /// 规范化幂等键。
    pub idempotency_key: IdempotencyKey,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 编排结果：新写入或同载荷回读。
#[derive(Debug, Clone)]
pub enum PreparedExecution {
    /// 需要在同一事务中应用的写入。
    Apply(Box<PlannedWrites>),
    /// 同载荷回读，不得重做写入。
    Replay {
        /// 已提交收据。
        receipt: ApprovalCommandReceipt,
    },
}

/// 拒绝尚未接线的运行编排调用。
///
/// # 错误
/// 始终返回业务逻辑错误。
pub fn refuse_unwired() -> Result<()> {
    Err(Error::BusinessLogicError(
        "审批运行编排尚未接入，已按安全策略拒绝".to_string(),
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        prepare_cancel, prepare_decision, prepare_document_cancel, prepare_resume, prepare_start,
        refuse_unwired, CancelExecutionInput, DecisionExecutionInput, ExecutionCommandInput,
        PreparedExecution, ResumeExecutionInput, StartExecutionInput,
    };
    use crate::approval::execution::decision::decision_commits_blocked;
    use crate::approval::execution::idempotency::normalize_idempotency_key;
    use crate::errors::Error;
    use bpm::engine::{
        BpmEventKind, CommitRequired, DefinitionGraph, Eligibility, StartAssigneeBinding, TaskCloseReason,
    };
    use bpm::ids::{
        ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeDefinitionId,
        ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId,
        ApprovalTransitionDefinitionId,
    };
    use bpm::model::types::{
        ApprovalBlockerCode, ApprovalCommandKind, ApprovalDecision, ApprovalNodeExecutionStatus,
        ApprovalProcessInstanceStatus, ApprovalTransitionEvent,
    };
    use bpm::model::{
        ApprovalCommandReceipt, ApprovalNodeDefinition, ApprovalProcessDefinition,
        ApprovalTransitionDefinition, NewNodeDefinition, ParticipantId, ProcessKind, SubjectRef, Timestamp,
    };
    use entities::approval_integration::ApprovalNotificationEventKind;

    /// 运行编排占位必须失败关闭，并钉死稳定文案。
    #[test]
    fn execution_placeholder_fails_closed() {
        let Err(Error::BusinessLogicError(message)) = refuse_unwired() else {
            panic!("运行编排占位必须返回 BusinessLogicError");
        };
        assert_eq!(message, "审批运行编排尚未接入，已按安全策略拒绝");
    }

    /// 启动、通过、驳回、取消和恢复均为单次编排计划。
    #[test]
    fn execution_commands_are_single_transaction_plans() {
        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(start_writes) = started else {
            panic!("启动必须产生写入");
        };
        assert_eq!(start_writes.created_executions.len(), 1);
        assert_eq!(start_writes.domain_action, Some(super::DomainActionKind::Start));

        let instance = start_writes.instance.clone();
        let current = start_writes.created_executions[0].clone();
        let approved = prepare_decision(decision_input(
            instance.clone(),
            current.clone(),
            ApprovalDecision::Approve,
            None,
            eligible("u1", "张三"),
            eligible("u2", "李四"),
            None,
        ))
        .unwrap();
        let PreparedExecution::Apply(approve_writes) = approved else {
            panic!("通过必须产生写入");
        };
        assert_eq!(
            approve_writes.updated_executions[0].status,
            ApprovalNodeExecutionStatus::Approved
        );

        let rejected = prepare_decision(decision_input(
            instance,
            current,
            ApprovalDecision::Reject,
            Some("资料不全".into()),
            eligible("u1", "张三"),
            eligible("u1", "张三"),
            None,
        ))
        .unwrap();
        let PreparedExecution::Apply(reject_writes) = rejected else {
            panic!("驳回必须产生写入");
        };
        assert_eq!(reject_writes.instance.current_round_no, 2);
        assert_eq!(reject_writes.instance.subject_version, 1);
    }

    /// 重复命令同载荷回读，异载荷冲突。
    #[test]
    fn execution_idempotent_replay_and_payload_conflict() {
        let first = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(writes) = first else {
            panic!("首次启动必须写入");
        };
        let replay =
            prepare_start(start_input(eligible("u1", "张三"), Some(writes.receipt.clone()))).unwrap();
        assert!(matches!(replay, PreparedExecution::Replay { .. }));

        let mut other = writes.receipt.clone();
        other.payload_digest = "other-digest".into();
        let conflict = prepare_start(start_input(eligible("u1", "张三"), Some(other))).unwrap_err();
        assert!(conflict
            .to_string()
            .contains("APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT"));
    }

    /// 当前审批人失效提交 BLOCKED，而不是回滚为空。
    #[test]
    fn execution_blocks_current_actor_instead_of_rollback() {
        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(writes) = started else {
            panic!("启动必须写入");
        };
        let prepared = prepare_decision(decision_input(
            writes.instance,
            writes.created_executions[0].clone(),
            ApprovalDecision::Approve,
            None,
            blocked("u1", "张三", ApprovalBlockerCode::ApproverAccountInactive),
            eligible("u2", "李四"),
            None,
        ))
        .unwrap();
        assert!(decision_commits_blocked(&prepared));
        let PreparedExecution::Apply(blocked_writes) = prepared else {
            panic!("阻塞必须提交");
        };
        assert_eq!(blocked_writes.commit, CommitRequired::Blocked);
        assert_eq!(
            blocked_writes.instance.status,
            ApprovalProcessInstanceStatus::Blocked
        );
    }

    /// 取消与受阻取消都产生取消领域动作。
    #[test]
    fn execution_cancel_and_blocked_cancel_use_same_action() {
        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(writes) = started else {
            panic!("启动必须写入");
        };
        let cancelled = prepare_cancel(cancel_input(
            writes.instance.clone(),
            writes.created_executions[0].clone(),
            false,
            None,
        ))
        .unwrap();
        let PreparedExecution::Apply(cancel_writes) = cancelled else {
            panic!("取消必须写入");
        };
        assert_eq!(cancel_writes.domain_action, Some(super::DomainActionKind::Cancel));
        assert_eq!(
            cancel_writes.instance.status,
            ApprovalProcessInstanceStatus::Cancelled
        );
    }

    /// 业务撤回收据同载荷可回放，业务单据版本漂移必须返回幂等冲突。
    #[test]
    fn document_cancel_receipt_binds_expected_document_version() {
        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(start_writes) = started else {
            panic!("启动必须写入");
        };
        let input = cancel_input(
            start_writes.instance,
            start_writes.created_executions[0].clone(),
            false,
            None,
        );
        let prepared = prepare_document_cancel(input.clone(), 7).unwrap();
        let PreparedExecution::Apply(writes) = prepared else {
            panic!("首次业务撤回必须写入");
        };

        let mut replay_input = input.clone();
        replay_input.command.receipt = Some(writes.receipt.clone());
        assert!(matches!(
            prepare_document_cancel(replay_input.clone(), 7).unwrap(),
            PreparedExecution::Replay { .. }
        ));

        let conflict = prepare_document_cancel(replay_input, 8).unwrap_err();
        assert!(conflict
            .to_string()
            .contains("APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT"));
    }

    /// 原审批人恢复创建新执行且不改变审批人绑定。
    #[test]
    fn execution_resume_replaces_execution_without_changing_assignee() {
        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(start_writes) = started else {
            panic!("启动必须写入");
        };
        let blocked_now = prepare_decision(decision_input(
            start_writes.instance,
            start_writes.created_executions[0].clone(),
            ApprovalDecision::Approve,
            None,
            blocked("u1", "张三", ApprovalBlockerCode::ApproverEmploymentInvalid),
            eligible("u2", "李四"),
            None,
        ))
        .unwrap();
        let PreparedExecution::Apply(writes) = blocked_now else {
            panic!("原审批人失效必须提交 BLOCKED");
        };
        let resumed = prepare_resume(resume_input(
            writes.instance.clone(),
            writes.updated_executions[0].clone(),
            start_writes.created_assignees[0].clone(),
            None,
        ))
        .unwrap();
        let PreparedExecution::Apply(resume_writes) = resumed else {
            panic!("恢复必须写入");
        };
        assert_eq!(
            resume_writes.created_executions[0].status,
            ApprovalNodeExecutionStatus::Active
        );
        assert!(resume_writes.created_assignees.is_empty());
        assert_eq!(
            resume_writes.created_executions[0]
                .assignee_participant_id
                .as_str(),
            "u1"
        );
    }

    /// 入口有效但非入口审批人失效时，由 BPM 拒绝创建实例。
    #[test]
    fn execution_start_rejects_ineligible_assignee() {
        let mut input = start_input(eligible("u1", "张三"), None);
        input.bindings[1].eligibility = blocked("u2", "李四", ApprovalBlockerCode::ApproverAccountInactive);
        let error = prepare_start(input).unwrap_err();
        assert!(error.to_string().contains("全部审批人必须有效"));
    }

    /// 原审批人可恢复的 blocker 不得走受阻取消；结构 blocker 只能走受阻取消。
    #[test]
    fn execution_cancel_blocked_rejects_assignee_recovery_and_accepts_structural() {
        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(start_writes) = started else {
            panic!("启动必须写入");
        };
        let recoverable = prepare_decision(decision_input(
            start_writes.instance.clone(),
            start_writes.created_executions[0].clone(),
            ApprovalDecision::Approve,
            None,
            blocked("u1", "张三", ApprovalBlockerCode::ApproverAccountInactive),
            eligible("u2", "李四"),
            None,
        ))
        .unwrap();
        let PreparedExecution::Apply(recoverable_writes) = recoverable else {
            panic!("原审批人失效必须提交");
        };
        let recoverable_blocked = prepare_cancel(cancel_input(
            recoverable_writes.instance,
            recoverable_writes.updated_executions[0].clone(),
            true,
            None,
        ))
        .unwrap_err();
        assert!(recoverable_blocked
            .to_string()
            .contains("原审批人可恢复时不得走受阻取消"));

        let mut graph = two_node_graph();
        graph.transitions.clear();
        let mut structural_input = decision_input(
            start_writes.instance,
            start_writes.created_executions[0].clone(),
            ApprovalDecision::Approve,
            None,
            eligible("u1", "张三"),
            eligible("u2", "李四"),
            None,
        );
        structural_input.command.graph = graph;
        let structural = prepare_decision(structural_input).unwrap();
        let PreparedExecution::Apply(structural_writes) = structural else {
            panic!("图损坏必须提交 BLOCKED");
        };
        assert_eq!(
            structural_writes.instance.blocker_code,
            Some(ApprovalBlockerCode::DefinitionGraphCorrupted)
        );
        let normal_cancel = prepare_cancel(cancel_input(
            structural_writes.instance.clone(),
            structural_writes.updated_executions[0].clone(),
            false,
            None,
        ))
        .unwrap_err();
        assert!(normal_cancel
            .to_string()
            .contains("不可恢复原审批人的阻塞只能走受阻取消"));
        let blocked_cancel = prepare_cancel(cancel_input(
            structural_writes.instance,
            structural_writes.updated_executions[0].clone(),
            true,
            None,
        ))
        .unwrap();
        let PreparedExecution::Apply(cancel_writes) = blocked_cancel else {
            panic!("受阻取消必须写入");
        };
        assert_eq!(
            cancel_writes.instance.status,
            ApprovalProcessInstanceStatus::Cancelled
        );
    }

    /// 同一执行多个 OPEN 任务提交 OPEN_TASK_CONFLICT。
    #[test]
    fn execution_blocks_open_task_conflict() {
        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(writes) = started else {
            panic!("启动必须写入");
        };
        let mut input = decision_input(
            writes.instance,
            writes.created_executions[0].clone(),
            ApprovalDecision::Approve,
            None,
            eligible("u1", "张三"),
            eligible("u2", "李四"),
            None,
        );
        input.open_task_count = 2;
        let prepared = prepare_decision(input).unwrap();
        let PreparedExecution::Apply(blocked_writes) = prepared else {
            panic!("任务冲突必须提交");
        };
        assert_eq!(
            blocked_writes.instance.blocker_code,
            Some(ApprovalBlockerCode::OpenTaskConflict)
        );
        assert_eq!(blocked_writes.commit, CommitRequired::Blocked);
        assert!(blocked_writes
            .events
            .iter()
            .any(|event| event.kind == BpmEventKind::InstanceBlocked));
        assert_eq!(
            blocked_writes.close_tasks,
            vec![(
                ApprovalNodeExecutionId::new("e1"),
                TaskCloseReason::ApprovalRuntimeBlocked
            )]
        );
        assert_eq!(blocked_writes.notifications.len(), 1);
        assert_eq!(
            blocked_writes.notifications[0].event_kind,
            ApprovalNotificationEventKind::Blocked
        );
        assert_eq!(blocked_writes.notifications[0].dedup_key, "blocked:e1");
    }

    /// 事务内应用计划；领域动作失败整单回滚；收据重复键按同/异载荷回读。
    #[test]
    fn execution_commit_writes_and_duplicate_key_replay() {
        use super::store::{
            commit_writes, replay_after_duplicate, MemoryRuntimeStore, RecordingDomainActions,
            TaskApplyContext,
        };
        use entities::common::time::Instant;

        let started = prepare_start(start_input(eligible("u1", "张三"), None)).unwrap();
        let PreparedExecution::Apply(writes) = started else {
            panic!("启动必须写入");
        };
        let ctx = TaskApplyContext {
            work_item_id: "wi-1".into(),
            business_object_type: "stock_adjustment".into(),
            business_object_id: "adj-1".into(),
            subject_version: "1".into(),
            owner_role: "stock_adjustment_approver".into(),
            owner_organization_id: "org-1".into(),
            actor_id: "u1".into(),
            now: Instant::from_unix_secs(10),
        };
        let mut failing = MemoryRuntimeStore::default();
        let fail_domain = RecordingDomainActions {
            fail: true,
            ..RecordingDomainActions::default()
        };
        assert!(commit_writes(&mut failing, &writes, &ctx, &fail_domain).is_err());
        assert!(failing.instance("inst").is_none());

        let mut store = MemoryRuntimeStore::default();
        let domain = RecordingDomainActions::default();
        commit_writes(&mut store, &writes, &ctx, &domain).unwrap();
        assert!(store.instance("inst").is_some());
        assert_eq!(store.open_task_count(&ApprovalNodeExecutionId::new("e1")), 1);
        assert!(store.outbox_items().next().is_some());
        assert!(!domain.executed.borrow().is_empty());

        let mut again = store.clone();
        let duplicate = commit_writes(&mut again, &writes, &ctx, &domain).unwrap_err();
        assert_eq!(duplicate, super::store::ApplyError::DuplicateReceipt);
        let same = replay_after_duplicate(
            &store,
            ApprovalCommandKind::StartApproval,
            &writes.receipt.scope_id,
            &writes.receipt.idempotency_key,
            &writes.receipt.payload_digest,
        )
        .unwrap();
        assert_eq!(same.payload_digest, writes.receipt.payload_digest);
        let conflict = replay_after_duplicate(
            &store,
            ApprovalCommandKind::StartApproval,
            &writes.receipt.scope_id,
            &writes.receipt.idempotency_key,
            "other-digest",
        )
        .unwrap_err();
        assert!(conflict
            .to_string()
            .contains("APPROVAL_IDEMPOTENCY_PAYLOAD_CONFLICT"));
    }

    /// 运行路径不接受旧恢复动作名称。
    #[test]
    fn execution_has_no_retry_current_step_symbol() {
        let key = normalize_idempotency_key(" key ").unwrap();
        assert_eq!(key.as_str(), "key");
        assert_ne!(
            ApprovalCommandKind::ResumeApprover.as_str(),
            &format!("{}{}", "RETRY_", "CURRENT_STEP")
        );
    }

    fn start_input(entry: Eligibility, receipt: Option<ApprovalCommandReceipt>) -> StartExecutionInput {
        StartExecutionInput {
            command: ExecutionCommandInput {
                graph: two_node_graph(),
                current_eligibility: entry.clone(),
                next_eligibility: eligible("u2", "李四"),
                receipt,
                idempotency_key: normalize_idempotency_key("start-1").unwrap(),
                now: at(10),
            },
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").unwrap(),
            subject_version: 1,
            binding_id: "def".into(),
            definition_version: 1,
            actor: participant("starter"),
            instance_id: ApprovalProcessInstanceId::new("inst"),
            entry_execution_id: ApprovalNodeExecutionId::new("e1"),
            receipt_id: ApprovalCommandReceiptId::new("r-start"),
            bindings: vec![
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
        }
    }

    fn decision_input(
        instance: bpm::model::ApprovalProcessInstance,
        current: bpm::model::ApprovalNodeExecution,
        decision: ApprovalDecision,
        reason: Option<String>,
        current_eligibility: Eligibility,
        next_eligibility: Eligibility,
        receipt: Option<ApprovalCommandReceipt>,
    ) -> DecisionExecutionInput {
        DecisionExecutionInput {
            command: ExecutionCommandInput {
                graph: two_node_graph(),
                current_eligibility,
                next_eligibility,
                receipt,
                idempotency_key: normalize_idempotency_key("dec-1").unwrap(),
                now: at(20),
            },
            instance,
            current,
            work_item_id: "wi-1".into(),
            task_owner_id: "u1".into(),
            instance_assignee_id: "u1".into(),
            decision,
            reason,
            expected_task_version: 1,
            actor: participant("u1"),
            next_execution_id: ApprovalNodeExecutionId::new("e2"),
            next_execution_no: 2,
            receipt_id: ApprovalCommandReceiptId::new("r-dec"),
            open_task_count: 1,
        }
    }

    fn cancel_input(
        instance: bpm::model::ApprovalProcessInstance,
        current: bpm::model::ApprovalNodeExecution,
        blocked_port: bool,
        receipt: Option<ApprovalCommandReceipt>,
    ) -> CancelExecutionInput {
        CancelExecutionInput {
            command: ExecutionCommandInput {
                graph: two_node_graph(),
                current_eligibility: eligible("u1", "张三"),
                next_eligibility: eligible("u1", "张三"),
                receipt,
                idempotency_key: normalize_idempotency_key("cancel-1").unwrap(),
                now: at(30),
            },
            instance,
            current,
            subject_version: 1,
            expected_instance_version: 1,
            expected_execution_version: 1,
            expected_task_version: Some(1),
            reason: "撤回".into(),
            actor: participant("starter"),
            close_open_task: true,
            blocked_port,
            receipt_id: ApprovalCommandReceiptId::new("r-cancel"),
        }
    }

    fn resume_input(
        instance: bpm::model::ApprovalProcessInstance,
        current: bpm::model::ApprovalNodeExecution,
        assignee: bpm::model::ApprovalInstanceAssignee,
        receipt: Option<ApprovalCommandReceipt>,
    ) -> ResumeExecutionInput {
        ResumeExecutionInput {
            command: ExecutionCommandInput {
                graph: two_node_graph(),
                current_eligibility: eligible("u1", "张三"),
                next_eligibility: eligible("u1", "张三"),
                receipt,
                idempotency_key: normalize_idempotency_key("resume-1").unwrap(),
                now: at(40),
            },
            instance,
            current,
            assignee,
            expected_instance_version: 2,
            expected_execution_version: 2,
            expected_assignment_version: 1,
            expected_closed_task_version: Some(2),
            next_execution_id: ApprovalNodeExecutionId::new("e2"),
            next_execution_no: 2,
            receipt_id: ApprovalCommandReceiptId::new("r-resume"),
            actor_id: "admin".into(),
        }
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
        DefinitionGraph {
            definition,
            nodes: vec![
                node("nd1", "n1", "仓储复核", 1, "u1", "张三", at),
                node("nd2", "n2", "财务复核", 2, "u2", "李四", at),
            ],
            transitions: vec![
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
        ApprovalNodeDefinition::new(NewNodeDefinition {
            id: ApprovalNodeDefinitionId::new(id),
            process_definition_id: ApprovalProcessDefinitionId::new("def"),
            node_key: key.into(),
            node_name: name.into(),
            node_purpose: None,
            display_order: order,
            assignee_participant_id: participant(user),
            assignee_label_snapshot: label.into(),
            at,
        })
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
