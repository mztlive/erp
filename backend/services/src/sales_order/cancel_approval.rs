//! 销售单撤回：调用统一 `prepare_cancel`，再执行业务 `cancel_action`。

use bpm::engine::DefinitionGraph;
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{
    ApprovalCancellationTaskPolicy, ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp,
};
use database::{AccessControlExt, BpmExt, NoTransaction, SalesOrderExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::sales_order::SalesOrder;
use entities::work_item::WorkItem;
use id_generator::next_id;
use mongodb::Database;

use super::start_approval::load_bound_definition_graph;
use crate::approval::execution::authorization::{converge_eligibility, requires_blocked_cancel};
use crate::approval::execution::idempotency::normalize_idempotency_key;
use crate::approval::execution::{CancelExecutionInput, ExecutionCommandInput, PreparedExecution};
use crate::errors::{Error, Result};
use entities::document_registry::business_document::ApprovalDefinitionBinding;

/// 已加载的可撤回运行事实。
pub(super) struct LoadedCancelRuntime {
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 非终态实例。
    pub instance: ApprovalProcessInstance,
    /// 当前执行。
    pub current: ApprovalNodeExecution,
    /// 当前实例决定的任务关闭策略。
    pub task_policy: ApprovalCancellationTaskPolicy,
    /// 当前执行上的开放任务。
    pub open_tasks: Vec<WorkItem>,
}

/// 按主体加载 RUNNING/BLOCKED 实例、当前执行与开放任务。
///
/// `RUNNING` 必须恰有一个开放任务，`BLOCKED` 必须没有开放任务。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
///
/// # 返回
/// 返回定义图、实例、当前执行、任务关闭策略与开放任务快照。
///
/// # 错误
/// 实例或当前执行缺失、状态与开放任务数量不一致或仓储失败时返回错误。
pub(super) async fn load_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    subject: &bpm::SubjectRef,
    subject_version: u32,
) -> Result<LoadedCancelRuntime> {
    let instance = db
        .bpm_workflow()
        .cancellation_instance_by_subject(subject, subject_version, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("没有可撤回的审批实例".to_string()))?;
    let task_policy = instance
        .cancellation_task_policy()
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    let current = db
        .bpm_workflow()
        .current_execution_for_cancellation(
            &ApprovalProcessInstanceId::new(instance.base.id.clone()),
            &mut NoTransaction,
        )
        .await?
        .ok_or_else(|| Error::ConflictError("审批实例缺少当前执行".to_string()))?;
    let open_tasks = db
        .work_items()
        .open_approval_tasks_for_execution(
            &ApprovalNodeExecutionId::new(current.base.id.clone()),
            &mut NoTransaction,
        )
        .await?;
    task_policy
        .ensure_open_task_count(open_tasks.len())
        .map_err(|error| Error::ConflictError(error.to_string()))?;
    Ok(LoadedCancelRuntime {
        graph: load_bound_definition_graph(db, binding).await?,
        instance,
        current,
        task_policy,
        open_tasks,
    })
}

/// 构造统一 `cancel_approval` 输入。
///
/// 人员失效走业务取消；非人员一致性 blocker 必须走受阻取消。
///
/// # 参数
/// * `runtime` - 已加载运行事实
/// * `reason` - 已校验的非空原因
/// * `actor_id` - 撤回人
/// * `idempotency_key` - 幂等键
/// * `receipt` - 已存在收据
/// * `now` - 调用方时间
///
/// # 返回
/// 返回可交给统一 `prepare_cancel` 的取消编排输入。
///
/// # 错误
/// 原因/幂等键非法、审批人引用无效或端口与 blocker 不匹配时返回错误。
pub fn build_sales_order_cancel_input(
    runtime: &LoadedCancelRuntime,
    reason: &str,
    actor_id: &str,
    idempotency_key: &str,
    receipt: Option<bpm::model::ApprovalCommandReceipt>,
    now: Instant,
) -> Result<CancelExecutionInput> {
    let reason = reason.trim();
    if reason.is_empty() {
        return Err(Error::ValidationError("撤回原因不能为空".to_string()));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("撤回人引用无效".to_string()))?;
    let eligibility = converge_eligibility(
        runtime.current.assignee_participant_id.as_str(),
        &runtime.current.assignee_name_snapshot,
        None,
    )?;
    let blocked_port = runtime.instance.blocker_code.is_some_and(requires_blocked_cancel);
    Ok(CancelExecutionInput {
        command: ExecutionCommandInput {
            graph: runtime.graph.clone(),
            current_eligibility: eligibility.clone(),
            next_eligibility: eligibility,
            receipt,
            idempotency_key,
            now: Timestamp::from_utc(now.as_utc()),
        },
        instance: runtime.instance.clone(),
        current: runtime.current.clone(),
        subject_version: runtime.instance.subject_version,
        expected_instance_version: runtime.instance.base.version,
        expected_execution_version: runtime.current.base.version,
        expected_task_version: runtime.open_tasks.first().map(|item| item.base.version),
        reason: reason.to_string(),
        actor,
        close_open_task: runtime.task_policy.closes_open_task(),
        blocked_port,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })
}

/// 销售单撤回事务写入集合。
///
/// # 用途
/// 收拢取消计划、开放任务、撤回人与审计，供同一事务写入。
///
/// # 参数
/// 无。
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 运行事实、任务关闭与单据回写必须同事务；CAS 失败时回滚。
pub(super) struct SalesOrderCancelPersistInput {
    /// 已执行 `cancel_action` 的销售单。
    pub order: SalesOrder,
    /// `prepare_cancel` 结果。
    pub prepared: PreparedExecution,
    /// 待关闭的开放任务。
    pub open_tasks: Vec<WorkItem>,
    /// 撤回人。
    pub actor_id: String,
    /// 撤回原因。
    pub reason: String,
    /// 调用方时间。
    pub now: Instant,
    /// 已构造审计。
    pub audit: entities::AuditLog,
}

/// 在同一事务内应用取消计划、关闭任务并写回销售单。
///
/// # 用途
/// 撤回审批后原子写回运行事实与销售单。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 销售单、取消计划与开放任务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// CAS 冲突或仓储失败时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复关闭任务；Apply 必须关闭开放任务并写回草稿。
pub(super) async fn persist_sales_order_cancel(
    db: &Database,
    input: SalesOrderCancelPersistInput,
) -> Result<()> {
    let SalesOrderCancelPersistInput {
        mut order,
        prepared,
        open_tasks,
        actor_id,
        reason,
        now,
        audit,
    } = input;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                if let PreparedExecution::Apply(writes) = prepared {
                    let closed_tasks =
                        WorkItem::close_all_for_approval_cancellation(open_tasks, &actor_id, &reason, now)?;
                    db.bpm_workflow()
                        .persist_cancelled_runtime(
                            &writes.instance,
                            &writes.updated_executions,
                            &writes.receipt,
                            session,
                        )
                        .await?;
                    db.work_items()
                        .persist_cancelled_approval_tasks(&closed_tasks, session)
                        .await?;
                }
                db.sales_orders().update(&mut order, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::{build_sales_order_cancel_input, LoadedCancelRuntime};
    use crate::approval::execution::{prepare_cancel, PreparedExecution};
    use bpm::engine::DefinitionGraph;
    use bpm::ids::{
        ApprovalNodeDefinitionId, ApprovalNodeExecutionId, ApprovalProcessDefinitionId,
        ApprovalProcessInstanceId, ApprovalTransitionDefinitionId,
    };
    use bpm::model::types::{
        ApprovalCommandKind, ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus,
        ApprovalTransitionEvent,
    };
    use bpm::model::{
        ApprovalCancellationTaskPolicy, ApprovalNodeDefinition, ApprovalNodeExecution,
        ApprovalProcessDefinition, ApprovalProcessInstance, ApprovalTransitionDefinition, NewNodeExecution,
        ParticipantId, ProcessKind, SubjectRef, Timestamp,
    };
    use entities::common::time::Instant;
    use entities::ids::WorkItemId;
    use entities::work_item::{DocumentApprovalWorkItemData, WorkItem, WorkItemPriority};

    fn at(secs: i64) -> Timestamp {
        Timestamp::from_unix_secs(secs).expect("时间合法")
    }

    fn participant(id: &str) -> ParticipantId {
        ParticipantId::new(id).expect("参与人合法")
    }

    fn graph() -> DefinitionGraph {
        let definition_id = ApprovalProcessDefinitionId::new("def-1");
        let started = at(1);
        DefinitionGraph {
            definition: ApprovalProcessDefinition::new_draft(
                definition_id.clone(),
                ProcessKind::SalesOrder,
                1,
                "销售审批",
                "n1",
                participant("admin"),
                started,
            )
            .expect("定义"),
            nodes: vec![ApprovalNodeDefinition::new(bpm::model::NewNodeDefinition {
                id: ApprovalNodeDefinitionId::new("nd-1"),
                process_definition_id: definition_id.clone(),
                node_key: "n1".into(),
                node_name: "入口".into(),
                node_purpose: None,
                display_order: 1,
                assignee_participant_id: participant("u1"),
                assignee_label_snapshot: "张三".into(),
                at: started,
            })
            .expect("节点")],
            transitions: vec![ApprovalTransitionDefinition::to_approved(
                ApprovalTransitionDefinitionId::new("tr-1"),
                definition_id,
                "n1",
                ApprovalTransitionEvent::Approve,
                started,
            )
            .expect("连线")],
        }
    }

    fn running_instance() -> ApprovalProcessInstance {
        let mut instance = ApprovalProcessInstance::start_running(bpm::model::NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst-1"),
            process_definition_id: ApprovalProcessDefinitionId::new("def-1"),
            definition_version: 1,
            process_kind: ProcessKind::SalesOrder,
            subject: SubjectRef::new("sales_order", "so-1").expect("主体"),
            subject_version: 1,
            started_by: participant("submitter"),
            at: at(10),
        })
        .expect("实例");
        instance.current_node_execution_id = Some(ApprovalNodeExecutionId::new("e1"));
        instance
    }

    fn active_execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "n1".into(),
            node_name: "入口".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: participant("u1"),
            assignee_name_snapshot: "张三".into(),
            at: at(10),
        })
        .expect("执行")
    }

    /// 构造当前执行关联的唯一开放审批任务。
    ///
    /// # 返回
    /// 返回责任人为 `u1` 且绑定执行 `e1` 的任务。
    fn open_task() -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new("wi-1"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("e1"),
                business_object_type: "sales_order".into(),
                business_object_id: "so-1".into(),
                subject_version: "1".into(),
                owner_role: "sales_order_approver".into(),
                owner_organization_id: "org-1".into(),
                owner_user_id: "u1".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .expect("开放审批任务")
    }

    /// 构造已通过模型取消规则校验的运行事实夹具。
    ///
    /// # 参数
    /// * `instance` - 带当前执行引用的运行实例
    ///
    /// # 返回
    /// 返回采用关闭唯一开放任务策略的取消输入上下文。
    fn runtime(instance: ApprovalProcessInstance) -> LoadedCancelRuntime {
        LoadedCancelRuntime {
            graph: graph(),
            instance,
            current: active_execution(),
            task_policy: ApprovalCancellationTaskPolicy::CloseOpenTask,
            open_tasks: vec![open_task()],
        }
    }

    /// 业务撤回必须构造统一 `CANCEL_APPROVAL` 并调用 `prepare_cancel`。
    ///
    /// 收据命令种类和实例终态都来自统一引擎计划。
    #[test]
    fn cancel_builds_unified_input_and_prepare_cancel() {
        let loaded = runtime(running_instance());
        let input = build_sales_order_cancel_input(
            &loaded,
            "撤回重改",
            "submitter",
            "cancel-key",
            None,
            Instant::from_unix_secs(30),
        )
        .expect("取消输入必须可构造");
        assert!(!input.blocked_port);
        assert!(input.close_open_task);
        assert_eq!(input.subject_version, 1);
        let prepared = prepare_cancel(input).expect("必须走统一 prepare_cancel");
        let PreparedExecution::Apply(writes) = prepared else {
            panic!("取消必须写入");
        };
        assert_eq!(writes.receipt.command_kind, ApprovalCommandKind::CancelApproval);
        assert_eq!(writes.instance.status, ApprovalProcessInstanceStatus::Cancelled);
    }
}
