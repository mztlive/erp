//! HTTP 面审批运行 Service：查询、决定、恢复、受阻取消与绑定升级。
//!
//! Handler 只转换协议；本文件编排仓储、prepare_* 与事务写入。

mod cancel_blocked;
mod decision_apply;
mod notifications;
mod query;
mod read_auth;
mod resume_apply;
mod tasks;
mod upgrade;

use std::sync::Arc;

use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::document_registry::DocumentType;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use bpm::engine::CommitRequired;
use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalCommandReceipt, ApprovalProcessInstance};
use mongodb::Database;
use persistence_core::Executor;
use serde::{Deserialize, Serialize};

use super::idempotency::PreparedCommandIdentity;
use super::view::{map_command_view, ApprovalCommandView, OpenTaskSummary};
use crate::error::{Error, Result};
use crate::ports::{
    ApprovalObjectReadPort, FailClosedObjectReadPort, ObjectFactPort, UpgradeSubjectPort, WorkflowAuditPort,
};
use crate::service::approval::process_kind::process_kind_of;
use crate::service::approval::{ApprovalDomainActionPort, FailClosedApprovalActionPort};
use application_core::AuditActor;

pub use query::{
    RuntimeInstanceListCursor, RuntimeInstanceListItem, RuntimeInstanceListPage, RuntimeInstanceListQuery,
    RuntimeRecoveryOptionsView,
};
pub use upgrade::UpgradeBindingCommand;

/// HTTP 面审批运行服务。
pub struct ApprovalRuntimeService<A> {
    db: Database,
    auth: A,
    action_port: Arc<dyn ApprovalDomainActionPort>,
    pub(crate) object_read: Arc<dyn ApprovalObjectReadPort>,
    pub(crate) upgrade: Arc<dyn UpgradeSubjectPort>,
    pub(crate) audit: Arc<dyn WorkflowAuditPort>,
    #[allow(dead_code)]
    pub(crate) facts: Arc<dyn ObjectFactPort>,
}

/// 候选人。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAssigneeCandidate {
    /// 账号 ID。
    pub user_id: String,
    /// 显示名。
    pub name: String,
}

impl<A: crate::ports::WorkflowAuthorizationPort> ApprovalRuntimeService<A> {
    /// 创建运行服务。
    ///
    /// # 参数
    /// * `db` - MongoDB
    /// * `auth` - 授权 Port
    ///
    /// # 返回
    /// 返回尚未由 P0-B 注入 AppState 的应用端口。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: Database, auth: A) -> Self {
        Self::with_ports(
            db,
            auth,
            Arc::new(FailClosedApprovalActionPort),
            Arc::new(FailClosedObjectReadPort),
            Arc::new(crate::ports::FailClosedUpgradeSubjectPort),
            Arc::new(crate::ports::FailClosedAuditPort),
            Arc::new(crate::ports::FailClosedObjectFactPort),
        )
    }

    /// 创建已由组合根注入领域动作端口的运行服务。
    pub fn with_action_port(db: Database, auth: A, action_port: Arc<dyn ApprovalDomainActionPort>) -> Self {
        Self::with_ports(
            db,
            auth,
            action_port,
            Arc::new(FailClosedObjectReadPort),
            Arc::new(crate::ports::FailClosedUpgradeSubjectPort),
            Arc::new(crate::ports::FailClosedAuditPort),
            Arc::new(crate::ports::FailClosedObjectFactPort),
        )
    }

    /// Create a runtime service with every composition-root port injected.
    pub fn with_ports(
        db: Database,
        auth: A,
        action_port: Arc<dyn ApprovalDomainActionPort>,
        object_read: Arc<dyn ApprovalObjectReadPort>,
        upgrade: Arc<dyn UpgradeSubjectPort>,
        audit: Arc<dyn WorkflowAuditPort>,
        facts: Arc<dyn ObjectFactPort>,
    ) -> Self {
        Self {
            db,
            auth,
            action_port,
            object_read,
            upgrade,
            audit,
            facts,
        }
    }
}

/// 加载并校验实例、process_kind 与冻结快照的不可变主体三元组。
async fn load_exact_runtime_snapshot(
    db: &Database,
    instance: &ApprovalProcessInstance,
    executor: &mut dyn Executor,
    hide_mismatch: bool,
) -> Result<(DocumentType, ApprovalSubjectSnapshot)> {
    let mismatch = || {
        if hide_mismatch {
            hidden_not_found()
        } else {
            Error::ConflictError("审批实例与冻结业务快照不一致".to_string())
        }
    };
    let document_type =
        crate::entity::approval_integration::document_type_from_subject_kind(instance.subject.subject_kind())
            .map_err(|_| mismatch())?;
    if instance.process_kind != process_kind_of(document_type) {
        return Err(mismatch());
    }
    let snapshot = db
        .approval_subject_snapshots()
        .find_by_process_instance_id(&instance.base.id, executor)
        .await?
        .ok_or_else(mismatch)?;
    snapshot
        .ensure_matches_runtime_subject(
            document_type,
            instance.subject.subject_id(),
            instance.subject_version,
        )
        .map_err(|_| mismatch())?;
    Ok((document_type, snapshot))
}

/// 以调用方事务执行器读取最新运行视图；回放不依赖原任务仍为 OPEN。
async fn persisted_command_view_with_executor(
    db: &Database,
    instance_id: &str,
    commit: CommitRequired,
    replay: bool,
    executor: &mut dyn Executor,
) -> Result<ApprovalCommandView> {
    let instance_id = ApprovalProcessInstanceId::new(instance_id);
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&instance_id, executor)
        .await?
        .ok_or_else(hidden_not_found)?;
    let current = db
        .bpm_workflow()
        .find_current_execution(&instance_id, executor)
        .await?;
    let next_open_task = match current.as_ref() {
        Some(execution) => {
            let tasks = db
                .work_items()
                .open_approval_tasks_for_execution(
                    &ApprovalNodeExecutionId::new(execution.base.id.clone()),
                    executor,
                )
                .await?;
            if tasks.len() > 1 {
                return Err(Error::ConflictError("当前执行关联多个开放审批任务".to_string()));
            }
            tasks.into_iter().next().map(|task| OpenTaskSummary {
                work_item_id: task.base.id,
                task_version: task.base.version.to_string(),
                owner_user_id: task.owner_user_id.unwrap_or_default(),
            })
        }
        None => None,
    };
    Ok(map_command_view(
        &instance,
        current.as_ref(),
        None,
        None,
        next_open_task,
        commit,
        replay,
    ))
}

/// 按当前 V3 scope 优先、已知历史 scope 次之读取唯一命令收据。
///
/// scope 与 digest 的完整成对判定由 [`PreparedCommandIdentity::classify`] 完成；
/// 当前 scope 一旦存在收据，调用方不得继续向历史 scope 降级。
async fn find_receipt_for_identity(
    db: &Database,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    for scope in identity.scope_candidates() {
        if let Some(receipt) = db
            .bpm_workflow()
            .find_command_receipt(
                identity.current().command_kind(),
                scope,
                identity.idempotency_key(),
                executor,
            )
            .await?
        {
            return Ok(Some(receipt));
        }
    }
    Ok(None)
}

/// 隐藏实例存在性。
fn hidden_not_found() -> Error {
    Error::NotFound("审批实例不存在".to_string())
}

/// 校验协议命令只能由当前认证主体执行。
fn ensure_command_actor(actor: &AuditActor, command_actor_id: &str) -> Result<()> {
    if actor.id() == command_actor_id {
        return Ok(());
    }
    Err(Error::Forbidden("审批命令操作人与认证主体不一致".to_string()))
}

/// 校验调用方持有的乐观锁版本。
fn ensure_expected_version(label: &str, expected: u64, actual: u64) -> Result<()> {
    if expected == actual {
        return Ok(());
    }
    Err(Error::ConflictError(format!("{label}版本已变化，请刷新后重试")))
}

/// CAS 未应用时失败关闭。
///
/// # 错误
/// 未找到、版本冲突或状态已变时返回冲突。
fn require_cas_applied<T>(outcome: crate::repository::bpm::CasWriteOutcome<T>, label: &str) -> Result<()> {
    match outcome {
        crate::repository::bpm::CasWriteOutcome::Applied(_) => Ok(()),
        _ => Err(Error::ConflictError(format!(
            "{label}已被其他请求修改，请刷新后重试"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use mongodb::error::{Error as MongoError, ErrorKind, WriteError, WriteFailure};
    use serde_json::json;

    use crate::entity::approval_integration::{ApprovalSubjectSnapshot, ApprovalSubjectSnapshotPayload};
    use crate::entity::document_registry::DocumentType;
    use crate::repository::approval_integration::{ApprovalRuntimeReadRow, ApprovalRuntimeReadTypeScope};
    use crate::repository::bpm::{
        ApprovalInstanceListView, ApprovalInstanceSummary, APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX,
    };
    use bpm::engine::TaskCloseReason;
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessDefinitionId, ApprovalProcessInstanceId};
    use bpm::model::types::{
        ApprovalBlockerCode, ApprovalDecision, ApprovalExecutionAssignmentSource,
        ApprovalProcessInstanceStatus,
    };
    use bpm::model::{
        ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, NewProcessInstance, ParticipantId,
        Timestamp,
    };
    use bpm::{ProcessKind, SubjectRef};
    use erp_core::common::time::Instant;
    use erp_core::ids::{ApprovalSubjectSnapshotId, WorkItemId};
    use erp_core::money::Quantity;
    use erp_core::AccountKind;

    use crate::entity::work_item::{
        ApprovalRuntimeTaskEnding, AssignmentSource, DocumentApprovalWorkItemData, WorkItem,
        WorkItemPriority, WorkItemStatus,
    };

    use crate::error::{Error, ErrorCode};
    use crate::service::approval::business_adapter::{adapter_spec_of, BindingRevalidationContext};
    use crate::service::approval::execution::idempotency::{
        command_may_have_committed, command_recovery_delay, map_receipt_first_write_error,
    };
    use crate::service::approval::ApprovalCancelBlockedCommand;
    use application_core::AuditActor;

    use super::super::runtime_query::RuntimeInstanceListView;
    use super::cancel_blocked::{
        cancel_blocked_terminal_facts_match, ensure_cancel_blocked_instance_preconditions,
        CancelBlockedTerminalFacts,
    };
    use super::decision_apply::{
        decision_receipt_lookup_gate, decision_terminal_actor, decision_terminal_fresh_error,
        legacy_decision_terminal_facts_match, map_approval_task_error, DecisionReceiptLookup,
        RuntimeDecisionCommand,
    };
    use super::notifications::{blocked_cancel_notification_recipients, notification_recipients};
    use super::query::{cursor_from_summary, item_from_runtime_read_row, item_from_summary};
    use super::read_auth::{
        ensure_mine_page_integrity, management_runtime_read_allowed, mine_execution_ids, mine_instance_ids,
        mine_runtime_chain_matches, ordinary_runtime_read_allowed, runtime_object_readable,
        started_runtime_read_allowed, task_proves_current_responsibility, unique_by_id,
        RuntimeReadAuthorizationFacts, RuntimeReadSubject,
    };
    use super::tasks::{approval_task_ending, CompleteOrCloseTasksInput};

    fn summary() -> ApprovalInstanceSummary {
        ApprovalInstanceSummary {
            id: "inst-1".to_string(),
            process_kind: ProcessKind::StockAdjustment,
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            definition_version: 2,
            subject: SubjectRef::new("stock_adjustment", "adj-1").expect("主体"),
            subject_version: 1,
            status: ApprovalProcessInstanceStatus::Running,
            current_round_no: 1,
            current_node_execution_id: Some(ApprovalNodeExecutionId::new("exec-1")),
            current_node_key: Some("review".to_string()),
            current_node_name: Some("仓储复核".to_string()),
            current_assignee_participant_id: Some("warehouse-1".to_string()),
            current_assignee_name: Some("仓库1".to_string()),
            latest_rejected_execution_id: None,
            latest_rejection_summary: None,
            last_status_changed_at: Some(20),
            started_by: "starter".to_string(),
            started_at: 10,
            blocked_at: None,
            version: 1,
            updated_at: 20,
        }
    }

    fn snapshot() -> ApprovalSubjectSnapshot {
        ApprovalSubjectSnapshot::new(
            ApprovalSubjectSnapshotId::new("snapshot-1"),
            ApprovalProcessInstanceId::new("inst-1"),
            DocumentType::StockAdjustment,
            "adj-1",
            1,
            ApprovalSubjectSnapshotPayload {
                document_no: "ADJ-0001".to_string(),
                responsible_org_id: "org-1".to_string(),
                submitted_by: "starter".to_string(),
                submitted_at: Instant::from_unix_secs(10),
                counterparty: None,
                total_amount: None,
                total_quantity: Some(Quantity::from_str("1").expect("数量")),
                line_count: 1,
            },
        )
        .expect("快照")
    }

    fn active_execution(execution_id: &str, instance_id: &str) -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new(execution_id),
            process_instance_id: ApprovalProcessInstanceId::new(instance_id),
            node_key: "review".to_string(),
            node_name: "仓储复核".to_string(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("warehouse-1").expect("审批人"),
            assignee_name_snapshot: "仓库1".to_string(),
            at: Timestamp::from_unix_secs(10).expect("时间"),
        })
        .expect("执行")
    }

    fn approval_task(task_id: &str, execution_id: &str) -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new(task_id),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new(execution_id),
                business_object_type: "stock_adjustment".to_string(),
                business_object_id: "adj-1".to_string(),
                subject_version: "1".to_string(),
                owner_role: "stock_adjustment_approver".to_string(),
                owner_organization_id: "org-1".to_string(),
                owner_user_id: "warehouse-1".to_string(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .expect("任务")
    }

    fn assert_hidden_not_found(error: Error) {
        assert!(matches!(
            error,
            Error::NotFound(message) if message == "审批实例不存在"
        ));
    }

    fn assert_same_error_semantics(actual: Error, expected: Error) {
        assert_eq!(std::mem::discriminant(&actual), std::mem::discriminant(&expected));
        assert_eq!(actual.code(), expected.code());
        assert_eq!(actual.to_string(), expected.to_string());
    }

    fn runtime_responsibility_fixture() -> (RuntimeReadSubject, WorkItem) {
        let at = Timestamp::from_unix_secs(10).expect("时间");
        let instance_id = ApprovalProcessInstanceId::new("inst-1");
        let execution_id = ApprovalNodeExecutionId::new("exec-1");
        let mut instance = ApprovalProcessInstance::start_running(NewProcessInstance {
            id: instance_id.clone(),
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            definition_version: 2,
            process_kind: ProcessKind::StockAdjustment,
            subject: SubjectRef::new("stock_adjustment", "adj-1").expect("主体"),
            subject_version: 1,
            started_by: ParticipantId::new("starter").expect("启动人"),
            at,
        })
        .expect("实例");
        let execution = active_execution("exec-1", "inst-1");
        instance
            .set_current_execution(execution_id.clone(), at)
            .expect("当前执行");
        let task = approval_task("wi-1", execution_id.as_ref());
        (
            RuntimeReadSubject {
                instance,
                current_execution: Some(execution),
                snapshot: snapshot(),
                document_type: DocumentType::StockAdjustment,
            },
            task,
        )
    }

    #[test]
    fn started_item_uses_snapshot_document_number_and_runtime_projection() {
        let snapshot = snapshot();
        let item = item_from_summary(summary(), Some(&snapshot)).expect("列表行");

        assert_eq!(item.document_type.as_deref(), Some("stock_adjustment"));
        assert_eq!(item.document_id.as_deref(), Some("adj-1"));
        assert_eq!(item.document_label.as_deref(), Some("ADJ-0001"));
        assert_eq!(item.current_assignee_name.as_deref(), Some("仓库1"));
        assert_eq!(item.process_version, Some(2));
        assert_eq!(item.started_at, Some(10));
    }

    #[test]
    fn started_amount_uses_matching_snapshot_and_preserves_zero() {
        for raw in ["12800.50", "0"] {
            let mut frozen = snapshot();
            frozen.payload.total_amount = Some(raw.parse().unwrap());
            let item = item_from_summary(summary(), Some(&frozen)).unwrap();
            assert_eq!(item.total_amount, frozen.payload.total_amount);
            assert!(serde_json::to_value(&item).unwrap()["total_amount"].is_string());
            frozen.subject_version += 1;
            assert_eq!(
                item_from_summary(summary(), Some(&frozen)).unwrap().total_amount,
                None
            );
            frozen.subject_version -= 1;
            frozen.approval_process_instance_id = ApprovalProcessInstanceId::new("other-instance");
            assert_eq!(
                item_from_summary(summary(), Some(&frozen)).unwrap().total_amount,
                None
            );
            frozen.approval_process_instance_id = ApprovalProcessInstanceId::new("inst-1");
            frozen.business_object_id = "other-object".into();
            assert_eq!(
                item_from_summary(summary(), Some(&frozen)).unwrap().total_amount,
                None
            );
        }
        assert_eq!(item_from_summary(summary(), None).unwrap().total_amount, None);
        assert_eq!(
            item_from_summary(summary(), Some(&snapshot()))
                .unwrap()
                .total_amount,
            None
        );
    }

    #[test]
    fn started_cursor_uses_started_time_and_stable_instance_id() {
        let cursor = cursor_from_summary(ApprovalInstanceListView::Started, &summary());
        assert_eq!(cursor.sort_time, 10);
        assert_eq!(cursor.id, "inst-1");
    }

    #[test]
    fn ordinary_read_matrix_supports_only_signed_sources() {
        let denied = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: false,
            current_responsibility: false,
            object_readable: false,
            scope_covers: false,
            runtime_admin: false,
        };
        assert!(!ordinary_runtime_read_allowed(denied));
        assert!(ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            initiator: true,
            ..denied
        }));
        assert!(ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            current_responsibility: true,
            ..denied
        }));
        assert!(ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            object_readable: true,
            scope_covers: true,
            ..denied
        }));
        assert!(!ordinary_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            actor_active: false,
            initiator: true,
            current_responsibility: true,
            object_readable: true,
            scope_covers: true,
            ..denied
        }));
    }

    #[test]
    fn current_responsibility_requires_exact_open_runtime_task_chain() {
        let (subject, task) = runtime_responsibility_fixture();
        let execution = subject.current_execution.as_ref().expect("当前执行");
        assert!(task_proves_current_responsibility(
            &task,
            execution,
            &subject,
            "warehouse-1",
            "stock_adjustment_approver",
        ));

        let mut wrong_source = task.clone();
        wrong_source.assignment_source = AssignmentSource::SystemRule;
        let mut wrong_role = task.clone();
        wrong_role.owner_role = "other-role".to_string();
        let mut wrong_org = task.clone();
        wrong_org.owner_organization_id = "org-2".to_string();
        let mut wrong_version = task.clone();
        wrong_version.subject_version = "01".to_string();
        let mut closed = task.clone();
        closed.status = WorkItemStatus::Closed;
        for candidate in [wrong_source, wrong_role, wrong_org, wrong_version, closed] {
            assert!(!task_proves_current_responsibility(
                &candidate,
                execution,
                &subject,
                "warehouse-1",
                "stock_adjustment_approver",
            ));
        }

        let mut ended_subject = subject;
        ended_subject.instance.status = ApprovalProcessInstanceStatus::Approved;
        assert!(!task_proves_current_responsibility(
            &task,
            ended_subject.current_execution.as_ref().expect("当前执行"),
            &ended_subject,
            "warehouse-1",
            "stock_adjustment_approver",
        ));
    }

    #[test]
    fn mine_chain_uses_runtime_identity_and_treats_snapshot_as_optional_label() {
        let (subject, task) = runtime_responsibility_fixture();
        let execution = subject.current_execution.as_ref().expect("当前执行");
        let row = summary();
        assert!(
            mine_runtime_chain_matches(&task, execution, &row, Some(&subject.snapshot), "warehouse-1",)
                .expect("责任链")
        );

        let mut drifted = subject.snapshot.clone();
        drifted.subject_version = 2;
        assert!(
            mine_runtime_chain_matches(&task, execution, &row, Some(&drifted), "warehouse-1",)
                .expect("漂移快照不撤销 WorkItem 责任")
        );

        let mut wrong_projection = row;
        wrong_projection.current_node_name = Some("错误节点".to_string());
        assert!(
            !mine_runtime_chain_matches(&task, execution, &wrong_projection, None, "warehouse-1",)
                .expect("实例投影漂移")
        );
    }

    #[test]
    fn mine_page_rejects_two_tasks_for_the_same_execution() {
        let tasks = [approval_task("wi-1", "exec-1"), approval_task("wi-2", "exec-1")];

        let error = mine_execution_ids(&tasks).expect_err("重复 execution 必须整页失败关闭");

        assert_hidden_not_found(error);
    }

    #[test]
    fn mine_page_hides_repository_wide_integrity_conflicts() {
        ensure_mine_page_integrity(0).expect("无完整性冲突");
        let error = ensure_mine_page_integrity(1).expect_err("跨页冲突必须隐藏式失败关闭");
        assert_hidden_not_found(error);
    }

    #[test]
    fn mine_page_rejects_two_executions_for_the_same_instance() {
        let execution_ids = vec![
            ApprovalNodeExecutionId::new("exec-1"),
            ApprovalNodeExecutionId::new("exec-2"),
        ];
        let execution_by_id = unique_by_id(
            vec![
                active_execution("exec-1", "inst-1"),
                active_execution("exec-2", "inst-1"),
            ],
            |execution| execution.base.id.clone(),
        )
        .expect("执行主键唯一");

        let error = mine_instance_ids(&execution_ids, &execution_by_id)
            .expect_err("不同 execution 指向同一 instance 必须整页失败关闭");

        assert_hidden_not_found(error);
    }

    #[test]
    fn mine_page_preserves_distinct_task_execution_instance_chains() {
        let tasks = [approval_task("wi-1", "exec-1"), approval_task("wi-2", "exec-2")];
        let execution_ids = mine_execution_ids(&tasks).expect("不同执行");
        let execution_by_id = unique_by_id(
            vec![
                active_execution("exec-1", "inst-1"),
                active_execution("exec-2", "inst-2"),
            ],
            |execution| execution.base.id.clone(),
        )
        .expect("执行主键唯一");

        let instance_ids = mine_instance_ids(&execution_ids, &execution_by_id).expect("不同实例");

        assert_eq!(
            instance_ids.iter().map(AsRef::as_ref).collect::<Vec<_>>(),
            vec!["inst-1", "inst-2"]
        );
    }

    #[test]
    fn management_requires_every_gate_while_started_uses_initiator_fact() {
        let allowed = RuntimeReadAuthorizationFacts {
            actor_active: true,
            initiator: true,
            current_responsibility: false,
            object_readable: true,
            scope_covers: true,
            runtime_admin: true,
        };
        assert!(management_runtime_read_allowed(allowed));
        assert!(started_runtime_read_allowed(allowed));
        for denied in [
            RuntimeReadAuthorizationFacts {
                actor_active: false,
                ..allowed
            },
            RuntimeReadAuthorizationFacts {
                object_readable: false,
                ..allowed
            },
            RuntimeReadAuthorizationFacts {
                scope_covers: false,
                ..allowed
            },
        ] {
            assert!(!management_runtime_read_allowed(denied));
        }
        assert!(!started_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            actor_active: false,
            ..allowed
        }));
        assert!(started_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            object_readable: false,
            scope_covers: false,
            runtime_admin: false,
            ..allowed
        }));
        assert!(!management_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            runtime_admin: false,
            ..allowed
        }));
        assert!(!started_runtime_read_allowed(RuntimeReadAuthorizationFacts {
            initiator: false,
            ..allowed
        }));
    }

    #[test]
    fn scoped_row_revalidates_snapshot_and_view_authorization() {
        let actor = AuditActor::new("starter".to_string(), "starter".to_string(), AccountKind::Admin);
        let type_scopes = [ApprovalRuntimeReadTypeScope {
            process_kind: ProcessKind::StockAdjustment,
            organization_ids: None,
        }];
        let item = item_from_runtime_read_row(
            ApprovalRuntimeReadRow {
                instance: summary(),
                snapshot: Some(snapshot()),
            },
            &actor,
            RuntimeInstanceListView::Started,
            &type_scopes,
        )
        .expect("Started 发起人事实成立");
        assert_eq!(item.instance_id, "inst-1");

        assert!(item_from_runtime_read_row(
            ApprovalRuntimeReadRow {
                instance: summary(),
                snapshot: Some(snapshot()),
            },
            &actor,
            RuntimeInstanceListView::Managed,
            &[],
        )
        .is_err());

        let mut drifted = snapshot();
        drifted.subject_version = 2;
        assert_eq!(
            item_from_summary(summary(), Some(&drifted))
                .expect("漂移快照仅清空标签")
                .document_label,
            None
        );
        assert_eq!(
            item_from_summary(summary(), None)
                .expect("缺失快照保留运行实例")
                .document_label,
            None
        );
    }

    #[test]
    fn approval_task_ending_rejects_conflicting_plan() {
        let execution_id = ApprovalNodeExecutionId::new("exec-1");
        let complete_tasks = vec![execution_id.clone()];
        let close_tasks = Vec::new();
        let input = CompleteOrCloseTasksInput {
            complete_tasks: &complete_tasks,
            close_tasks: &close_tasks,
            work_item_id: "wi-1",
            expected_task_version: 1,
            ended_execution_id: "exec-1",
            actor_id: "u1",
            now: Instant::from_unix_secs(20),
        };
        assert_eq!(
            approval_task_ending(&input, &execution_id).unwrap(),
            Some(ApprovalRuntimeTaskEnding::Complete)
        );

        let close_tasks = vec![(execution_id.clone(), TaskCloseReason::ApprovalRuntimeBlocked)];
        let conflicting = CompleteOrCloseTasksInput {
            close_tasks: &close_tasks,
            ..input
        };
        assert!(approval_task_ending(&conflicting, &execution_id).is_err());
    }

    #[test]
    fn decision_and_blocked_cancel_notification_recipients_are_contract_exact() {
        assert_eq!(
            notification_recipients("submitter", ["runtime-admin", "submitter", "runtime-admin"]),
            vec!["submitter".to_string(), "runtime-admin".to_string()]
        );
        assert_eq!(
            blocked_cancel_notification_recipients("submitter", "executing-admin"),
            vec!["submitter".to_string(), "executing-admin".to_string()]
        );
        assert_eq!(
            blocked_cancel_notification_recipients("same-user", "same-user"),
            vec!["same-user".to_string()]
        );
    }

    fn duplicate_key_error(index_name: Option<&str>) -> persistence_core::Error {
        let message = index_name.map_or_else(
            || "E11000 duplicate key error".to_string(),
            |index| format!("E11000 duplicate key error collection: erp.receipts index: {index} dup key"),
        );
        let write_error: WriteError = serde_json::from_value(json!({
            "code": 11000,
            "codeName": "DuplicateKey",
            "errmsg": message,
            "errInfo": null,
        }))
        .expect("duplicate key fixture");
        let mongo_error: MongoError = ErrorKind::Write(WriteFailure::WriteError(write_error)).into();
        persistence_core::Error::from(mongo_error)
    }

    fn runtime_source_fn(start: &str, end: &str) -> &'static str {
        const SOURCES: &[&str] = &[
            include_str!("runtime_service/resume_apply.rs"),
            include_str!("runtime_service/decision_apply.rs"),
            include_str!("runtime_service/cancel_blocked.rs"),
            include_str!("runtime_service/query.rs"),
            include_str!("runtime_service/read_auth.rs"),
            include_str!("runtime_service/notifications.rs"),
            include_str!("runtime_service/tasks.rs"),
            include_str!("runtime_service/upgrade.rs"),
        ];
        for source in SOURCES {
            let Some(start_idx) = source.find(start) else {
                continue;
            };
            let end_idx = source[start_idx..]
                .find(end)
                .map(|offset| start_idx + offset)
                .expect("运行时函数终点必须存在");
            return &source[start_idx..end_idx];
        }
        panic!("运行时函数起点必须存在");
    }

    #[test]
    fn decision_recovery_only_polls_receipt_competition_and_uncertain_commits() {
        let duplicate = map_receipt_first_write_error(duplicate_key_error(Some(
            APPROVAL_COMMAND_RECEIPT_IDEMPOTENCY_INDEX,
        )));
        assert!(command_may_have_committed(&duplicate));
        assert!(matches!(duplicate, Error::ReceiptDuplicate(_)));

        for unrelated in [Some("_id_"), Some("uk_approval_command_receipts_id"), None] {
            let error = map_receipt_first_write_error(duplicate_key_error(unrelated));
            assert!(!command_may_have_committed(&error));
            assert!(matches!(error, Error::ConflictError(_)));
        }
        let transient = Error::from(persistence_core::Error::TransientTransactionConflict(
            mongodb::error::Error::custom("write conflict"),
        ));
        assert!(command_may_have_committed(&transient));
        assert!(matches!(transient, Error::TransientTransaction(_)));
        assert!(command_may_have_committed(&Error::OutcomeUnknown(
            persistence_core::Error::CommitOutcomeUnknown(mongodb::error::Error::custom("unknown commit")),
        )));
        assert!(!command_may_have_committed(&Error::ConflictError(
            "数据已存在，请勿重复提交".to_string()
        )));
        assert!(!command_may_have_committed(&Error::ConflictError(
            "并发事务冲突，请重试".to_string()
        )));
        assert!(!command_may_have_committed(&Error::ConflictError(
            "审批任务已结束".to_string()
        )));
        assert!(!command_may_have_committed(&Error::ValidationError(
            "请求无效".to_string()
        )));
        assert_eq!(command_recovery_delay(0).as_millis(), 5);
        assert_eq!(command_recovery_delay(5).as_millis(), 160);
        assert_eq!(command_recovery_delay(99).as_millis(), 160);
    }

    #[test]
    fn resume_persistence_keeps_receipt_as_first_physical_write() {
        let source = runtime_source_fn(
            "async fn persist_resume_writes(",
            "fn ensure_resume_approver_recovered(",
        );
        let receipt = source.find("insert_command_receipt").expect("恢复必须写命令收据");
        assert!(source[..receipt].contains("find_document_approval_by_id"));
        assert!(!source[..receipt].contains("advance_instance"));
        assert!(!source[..receipt].contains("end_blocked_execution"));
        assert!(!source[..receipt].contains("insert_execution"));
        assert!(!source[..receipt].contains("create_open_tasks"));
        assert!(!source[..receipt].contains("persist_resume_notifications"));
        assert!(!source[..receipt].contains("audit_port.persist"));
        assert!(source[receipt..].contains("map_err(map_receipt_first_write_error)"));
        for later_write in [
            "advance_instance",
            "end_blocked_execution",
            "insert_execution",
            "create_open_tasks",
            "persist_resume_notifications",
            "audit_port.persist",
        ] {
            assert!(
                receipt < source.find(later_write).expect("恢复后续写入必须存在"),
                "receipt 必须先于 {later_write}",
            );
        }
    }

    #[test]
    fn resume_uncertain_result_recovery_always_opens_a_fresh_transaction() {
        let endpoint = runtime_source_fn("pub async fn resume_current_approver(", "async fn replay_resume(");
        assert!(endpoint.contains("command_may_have_committed"));
        assert!(endpoint.contains("recover_resume_after_competing_commit"));

        let replay = runtime_source_fn(
            "async fn replay_resume(",
            "async fn recover_resume_after_competing_commit(",
        );
        assert!(replay.contains("with_transaction"));
        assert!(replay.contains("replay_resume_in_transaction"));

        let recovery = runtime_source_fn(
            "async fn recover_resume_after_competing_commit(",
            "async fn load_resume_task_guard(",
        );
        assert!(recovery.contains("const RECOVERY_ATTEMPTS"));
        assert!(recovery.contains("self.replay_resume"));
        assert!(recovery.contains("command_recovery_delay"));
        assert!(!recovery.contains("ClientSession"));
    }

    fn decided_fixture(
        reason: Option<&str>,
        expected_task_version: u64,
    ) -> (WorkItem, ApprovalNodeExecution) {
        let mut execution = active_execution("exec-legacy", "inst-legacy");
        execution
            .record_approve(
                ParticipantId::new("warehouse-1").expect("决定人"),
                reason.map(ToOwned::to_owned),
                Timestamp::from_unix_secs(20).expect("决定时间"),
            )
            .expect("记录终态决定");
        let mut item = approval_task("wi-legacy", "exec-legacy");
        item.complete_by_approval_runtime("warehouse-1", Instant::from_unix_secs(20))
            .expect("完成审批任务");
        item.base.version = expected_task_version + 1;
        (item, execution)
    }

    fn runtime_decision_command(
        reason: Option<&str>,
        expected_task_version: u64,
        _actor_id: &str,
    ) -> RuntimeDecisionCommand {
        RuntimeDecisionCommand {
            work_item_id: "wi-legacy".to_string(),
            decision: ApprovalDecision::Approve,
            reason: reason.map(ToOwned::to_owned),
            expected_task_version,
            idempotency_key: crate::service::approval::execution::idempotency::normalize_idempotency_key(
                "legacy-key",
            )
            .expect("幂等键"),
        }
    }

    #[test]
    fn decision_existing_and_missing_keys_hide_from_outsider_and_revoked_actor() {
        let (item, execution) = decided_fixture(None, 3);
        assert_eq!(decision_terminal_actor(&item, &execution), Some("warehouse-1"));

        let missing_outsider = map_approval_task_error(
            item.approval_execution_for_decision("outsider", 3)
                .expect_err("非原责任人 Fresh 路径必须拒绝"),
        );
        let existing_outsider =
            decision_receipt_lookup_gate(&item, "outsider", 3).expect_err("非原责任人不得进入收据查询");
        assert_same_error_semantics(existing_outsider, missing_outsider);

        let missing_original = map_approval_task_error(
            item.approval_execution_for_decision("warehouse-1", 3)
                .expect_err("已完成任务 Fresh 路径必须稳定返回 NotOpen"),
        );
        assert!(matches!(
            decision_receipt_lookup_gate(&item, "warehouse-1", 3).expect("原责任人可继续证明回放"),
            DecisionReceiptLookup::Terminal(execution_id) if execution_id.as_ref() == "exec-legacy"
        ));
        let existing_revoked = decision_terminal_fresh_error();
        assert_same_error_semantics(existing_revoked, missing_original);
        assert_eq!(
            decision_terminal_fresh_error().code(),
            Some(ErrorCode::ApprovalTaskNotOpen)
        );
    }

    #[test]
    fn cancel_existing_and_missing_keys_share_fresh_terminal_errors_before_digest() {
        let (mut subject, _) = runtime_responsibility_fixture();
        let expected_instance_version = subject.instance.base.version;
        subject
            .instance
            .cancel(Timestamp::from_unix_secs(20).expect("取消时间"))
            .expect("构造已取消终态");
        let command = ApprovalCancelBlockedCommand {
            approval_process_instance_id: subject.instance.base.id.clone(),
            expected_instance_version,
            expected_execution_version: 7,
            expected_task_version: None,
            reason: "结构受损退出".to_string(),
            idempotency_key: "cancel-key".to_string(),
            actor_id: "runtime-admin".to_string(),
        };
        let missing_stale = ensure_cancel_blocked_instance_preconditions(&subject.instance, &command)
            .expect_err("Fresh 路径先返回实例版本冲突");
        let existing_non_actor = ensure_cancel_blocked_instance_preconditions(&subject.instance, &command)
            .expect_err("existing key 非原 actor 必须复用同一 Fresh 冲突");
        assert_same_error_semantics(existing_non_actor, missing_stale);

        let mut current_version = command.clone();
        current_version.expected_instance_version = subject.instance.base.version;
        let missing_status =
            ensure_cancel_blocked_instance_preconditions(&subject.instance, &current_version)
                .expect_err("伪造当前版本仍必须按 Fresh 状态失败");
        let existing_status =
            ensure_cancel_blocked_instance_preconditions(&subject.instance, &current_version)
                .expect_err("existing key 不得改为摘要冲突");
        assert_same_error_semantics(existing_status, missing_status);

        let facts = CancelBlockedTerminalFacts {
            blocker: ApprovalBlockerCode::DefinitionGraphCorrupted,
            actor_id: "runtime-admin".to_string(),
            reason: command.reason.clone(),
            execution_version: command.expected_execution_version + 1,
            task_versions: Vec::new(),
        };
        assert!(cancel_blocked_terminal_facts_match(
            &subject.instance,
            &facts,
            &command,
            "runtime-admin"
        ));
        assert!(!cancel_blocked_terminal_facts_match(
            &subject.instance,
            &facts,
            &command,
            "other-admin"
        ));
    }

    #[test]
    fn legacy_decision_requires_exact_terminal_facts() {
        let (none_item, none_execution) = decided_fixture(None, 3);
        let exact_none = runtime_decision_command(None, 3, "warehouse-1");
        let literal_null = runtime_decision_command(Some("NULL"), 3, "warehouse-1");
        assert!(legacy_decision_terminal_facts_match(
            &none_item,
            &none_execution,
            &exact_none,
            "warehouse-1"
        ));
        assert!(!legacy_decision_terminal_facts_match(
            &none_item,
            &none_execution,
            &literal_null,
            "warehouse-1"
        ));

        let (separator_item, separator_execution) = decided_fixture(Some("x\u{1f}3"), 4);
        let separator_exact = runtime_decision_command(Some("x\u{1f}3"), 4, "warehouse-1");
        let separator_relocated = runtime_decision_command(Some("x"), 3, "4\u{1f}warehouse-1");
        assert!(legacy_decision_terminal_facts_match(
            &separator_item,
            &separator_execution,
            &separator_exact,
            "warehouse-1"
        ));
        assert!(!legacy_decision_terminal_facts_match(
            &separator_item,
            &separator_execution,
            &separator_relocated,
            "4\u{1f}warehouse-1"
        ));
    }

    #[test]
    fn stock_adjustment_runtime_object_read_uses_registered_permission_scope() {
        let spec = adapter_spec_of(DocumentType::StockAdjustment).expect("库存调整适配器");
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "submitter".to_string(),
        };
        let port = crate::ports::FailClosedObjectReadPort;
        assert!(
            runtime_object_readable(&spec, &context, "approver", true, &port).expect("已登记读权且范围覆盖")
        );
        assert!(
            !runtime_object_readable(&spec, &context, "approver", false, &port).expect("范围不覆盖必须拒绝")
        );
    }
}
