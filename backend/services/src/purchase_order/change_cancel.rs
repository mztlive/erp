//! 采购变更撤回：调用统一 `prepare_cancel`，再执行业务 `cancel_action`。

use bpm::engine::{plan_cancel, CancelPlan, CancelPlanInput, DefinitionGraph};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, ParticipantId, Timestamp};
use database::repository::bpm::ApprovalInstanceListProjection;
use database::{AccessControlExt, BpmExt, NoTransaction, PurchaseOrderExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::purchase_order::PurchaseChangeOrder;
use entities::work_item::{WorkItem, WorkItemCloseData};
use id_generator::next_id;
use mongodb::Database;

use super::change_start::load_bound_definition_graph;
use crate::approval::execution::authorization::converge_eligibility;
use crate::approval::execution::idempotency::normalize_idempotency_key;
use crate::approval::execution::start::map_engine_error;
use crate::approval::execution::{
    normalize_document_cancel_reason, CancelExecutionInput, ExecutionCommandInput, PreparedExecution,
};
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
    /// BPM 统一取消计划。
    pub plan: CancelPlan,
    /// 当前执行上的开放任务。
    pub open_tasks: Vec<WorkItem>,
}

/// 按主体加载 RUNNING/BLOCKED 实例、当前执行与开放任务，并由 BPM 形成取消计划。
///
/// 已 `APPROVED` 的实例必须拒绝。`RUNNING` 必须恰有一个开放任务，`BLOCKED`
/// 时不得存在开放任务。开放任务计数与采购单一致：仅统计当前执行上的
/// 单据审批任务，独立任务不得影响取消计划。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
///
/// # 返回
/// 返回定义图、实例、当前执行、取消计划与开放任务快照。
///
/// # 错误
/// 实例缺失、已终态、受阻仍有开放任务或仓储失败时返回错误。
pub(super) async fn load_cancel_runtime(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    subject: &bpm::SubjectRef,
    subject_version: u32,
) -> Result<LoadedCancelRuntime> {
    let instance = db
        .bpm_workflow()
        .find_non_terminal_by_subject(subject, subject_version, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::ConflictError("没有可撤回的审批实例".to_string()))?;
    let current = db
        .bpm_workflow()
        .find_current_execution(
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
    let plan = plan_cancel(CancelPlanInput {
        instance: &instance,
        current: &current,
        open_task_count: open_tasks.len(),
    })
    .map_err(map_cancel_plan_error)?;
    Ok(LoadedCancelRuntime {
        graph: load_bound_definition_graph(db, binding).await?,
        instance,
        current,
        plan,
        open_tasks,
    })
}

/// 构造统一 `cancel_approval` 输入。
///
/// 本端口为业务单据普通撤回；人员失效 blocker 走业务取消，非人员一致性
/// blocker 由统一受阻取消端口承担。
///
/// # 参数
/// * `runtime` - 已加载运行事实与 BPM 取消计划
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
///
/// # 关键业务约束
/// 状态、开放任务数量与 blocker 端口分类由 BPM `plan_cancel` 计算；本端口为
/// 业务单据普通撤回，受阻取消由统一取消端口承担。采购单与采购变更单必须
/// 复用同一规则。
pub fn build_purchase_change_cancel_input(
    runtime: &LoadedCancelRuntime,
    reason: &str,
    actor_id: &str,
    idempotency_key: &str,
    receipt: Option<bpm::model::ApprovalCommandReceipt>,
    now: Instant,
) -> Result<CancelExecutionInput> {
    let reason = normalize_document_cancel_reason(reason)?;
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("撤回人引用无效".to_string()))?;
    let eligibility = converge_eligibility(
        runtime.current.assignee_participant_id.as_str(),
        &runtime.current.assignee_name_snapshot,
        None,
    )?;
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
        reason,
        actor,
        close_open_task: runtime.plan.close_open_task,
        blocked_port: false,
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
    })
}

/// 将取消计划错误映射为服务错误；状态与任务数量错误保持冲突语义。
///
/// # 参数
/// * `error` - BPM 取消计划错误
///
/// # 返回
/// 模型状态错误返回冲突，其余错误按引擎错误映射。
fn map_cancel_plan_error(error: bpm::engine::EngineError) -> Error {
    match error {
        bpm::engine::EngineError::Model(error) => Error::ConflictError(error.to_string()),
        other => map_engine_error(other),
    }
}

/// 采购变更撤回事务写入集合。
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
pub(super) struct PurchaseChangeCancelPersistInput {
    /// 已执行 `cancel_action` 的变更单。
    pub change_order: PurchaseChangeOrder,
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

/// 在同一事务内应用取消计划、关闭任务并写回变更单。
///
/// # 用途
/// 撤回审批后原子写回运行事实与变更单。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 变更单、取消计划与开放任务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// CAS 冲突或仓储失败时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复关闭任务；Apply 必须关闭开放任务并写回草稿。
pub(super) async fn persist_purchase_change_cancel(
    db: &Database,
    input: PurchaseChangeCancelPersistInput,
) -> Result<()> {
    let PurchaseChangeCancelPersistInput {
        mut change_order,
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
                    persist_cancel_runtime(&db, &writes, &open_tasks, &actor_id, &reason, now, session)
                        .await?;
                }
                db.purchase_change_orders()
                    .update(&mut change_order, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 将取消计划写入实例、执行、收据与任务。
///
/// # 错误
/// CAS 未应用或写入失败时返回错误。
async fn persist_cancel_runtime(
    db: &Database,
    writes: &crate::approval::execution::apply_plan::PlannedWrites,
    open_tasks: &[WorkItem],
    actor_id: &str,
    reason: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    let expected_instance_version = writes
        .instance
        .base
        .version
        .checked_sub(1)
        .ok_or_else(|| Error::Internal("取消后实例版本非法".to_string()))?;
    let expected_execution_id = writes
        .updated_executions
        .first()
        .map(|item| bpm::ids::ApprovalNodeExecutionId::new(item.base.id.clone()))
        .ok_or_else(|| Error::Internal("取消计划缺少结束执行".to_string()))?;
    require_cas_applied(
        db.bpm_workflow()
            .advance_instance(
                &writes.instance,
                expected_instance_version,
                &expected_execution_id,
                &cancel_list_projection(now),
                session,
            )
            .await?,
        "审批实例",
    )?;
    for execution in &writes.updated_executions {
        let expected = execution
            .base
            .version
            .checked_sub(1)
            .ok_or_else(|| Error::Internal("取消后执行版本非法".to_string()))?;
        require_cas_applied(
            db.bpm_workflow()
                .end_active_execution(execution, expected, session)
                .await?,
            "审批执行",
        )?;
    }
    db.approval_command_receipts()
        .create(&writes.receipt, session)
        .await?;
    close_open_tasks(db, open_tasks, actor_id, reason, now, session).await
}

/// 关闭当前开放审批任务。
///
/// # 错误
/// 任务非开放或 CAS 失败时返回错误。
async fn close_open_tasks(
    db: &Database,
    open_tasks: &[WorkItem],
    actor_id: &str,
    reason: &str,
    now: Instant,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    for item in open_tasks {
        let mut item = item.clone();
        let expected = item.base.version;
        let execution_id = item
            .approval_node_execution_id
            .clone()
            .ok_or_else(|| Error::ConflictError("开放审批任务缺少节点执行引用".to_string()))?;
        item.close_by_approval_runtime(
            actor_id,
            WorkItemCloseData {
                close_reason: reason.to_string(),
            },
            now,
        )?;
        require_cas_applied(
            db.work_items()
                .close_approval_task(&item, expected, &execution_id, session)
                .await?,
            "审批任务",
        )?;
    }
    Ok(())
}

/// 取消后的有界列表投影：不得再展示当前审批人。
///
/// # 参数
/// * `now` - 状态变更时间
///
/// # 返回
/// 返回清空当前节点的列表投影。
fn cancel_list_projection(now: Instant) -> ApprovalInstanceListProjection {
    ApprovalInstanceListProjection {
        current_node_key: None,
        current_node_name: None,
        current_assignee_participant_id: None,
        current_assignee_name: None,
        latest_rejected_execution_id: None,
        latest_rejection_summary: None,
        last_status_changed_at: Some(now.unix_secs()),
    }
}

/// CAS 未应用时失败关闭。
///
/// # 错误
/// 未找到、版本冲突或状态已变时返回冲突。
fn require_cas_applied<T>(outcome: database::repository::bpm::CasWriteOutcome<T>, label: &str) -> Result<()> {
    match outcome {
        database::repository::bpm::CasWriteOutcome::Applied(_) => Ok(()),
        _ => Err(Error::ConflictError(format!(
            "{label}已被其他请求修改，请刷新后重试"
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::{build_purchase_change_cancel_input, LoadedCancelRuntime};
    use bpm::engine::{plan_cancel, CancelPlanInput};
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::types::{ApprovalBlockerCode, ApprovalExecutionAssignmentSource};
    use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, ParticipantId};
    use entities::common::time::Instant;

    use crate::approval::execution::prepare_cancel;
    use crate::errors::Error;
    use crate::purchase_order::start_approval::tests::{open_task, two_node_graph};

    fn current_execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "n1".into(),
            node_name: "采购确认".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: bpm::model::Timestamp::from_unix_secs(10).unwrap(),
        })
        .expect("当前执行夹具")
    }

    fn running_instance() -> ApprovalProcessInstance {
        let mut inst = ApprovalProcessInstance::start_running(bpm::model::NewProcessInstance {
            id: ApprovalProcessInstanceId::new("inst-1"),
            process_definition_id: bpm::ids::ApprovalProcessDefinitionId::new("def"),
            definition_version: 1,
            process_kind: bpm::model::ProcessKind::PurchaseChangeOrder,
            subject: bpm::model::SubjectRef::new("purchase_change_order", "co-1").unwrap(),
            subject_version: 1,
            started_by: ParticipantId::new("starter").unwrap(),
            at: bpm::model::Timestamp::from_unix_secs(10).unwrap(),
        })
        .unwrap();
        inst.set_current_execution(
            ApprovalNodeExecutionId::new("e1"),
            bpm::model::Timestamp::from_unix_secs(11).unwrap(),
        )
        .unwrap();
        inst
    }

    fn blocked_instance(code: ApprovalBlockerCode) -> ApprovalProcessInstance {
        let mut inst = running_instance();
        inst.enter_blocked(code, bpm::model::Timestamp::from_unix_secs(12).unwrap())
            .unwrap();
        inst
    }

    fn runtime(
        instance: ApprovalProcessInstance,
        open_tasks: Vec<entities::work_item::WorkItem>,
    ) -> LoadedCancelRuntime {
        let current = current_execution();
        let plan = plan_cancel(CancelPlanInput {
            instance: &instance,
            current: &current,
            open_task_count: open_tasks.len(),
        })
        .unwrap();
        LoadedCancelRuntime {
            graph: two_node_graph(),
            instance,
            current,
            plan,
            open_tasks,
        }
    }

    /// 运行中撤回按 BPM 计划关闭唯一开放任务，与采购单行为一致。
    #[test]
    fn purchase_change_cancel_uses_bpm_plan_for_open_task_close() {
        let input = build_purchase_change_cancel_input(
            &runtime(running_instance(), vec![open_task(3)]),
            " 撤销重提  ",
            "u1",
            "key-1",
            None,
            Instant::from_unix_secs(20),
        )
        .unwrap();
        assert!(input.close_open_task);
        assert!(!input.blocked_port);
        assert_eq!(input.expected_instance_version, 2);
        assert_eq!(input.expected_execution_version, 1);
        assert_eq!(input.expected_task_version, Some(3));
        assert_eq!(input.reason, "撤销重提");
    }

    /// 人员失效 blocker 走业务取消端口。
    #[test]
    fn purchase_change_cancel_personnel_blocker_uses_business_port() {
        let input = build_purchase_change_cancel_input(
            &runtime(
                blocked_instance(ApprovalBlockerCode::ApproverAccountInactive),
                vec![],
            ),
            "撤销",
            "u1",
            "key-1",
            None,
            Instant::from_unix_secs(20),
        )
        .unwrap();
        assert!(!input.close_open_task);
        assert!(!input.blocked_port);
    }

    /// 非人员 blocker 不得走业务撤回端口，与采购单业务端口失败关闭一致；
    /// 统一受阻取消端口由 `prepare_cancel` 的受阻分支承担。
    #[test]
    fn purchase_change_cancel_structural_blocker_fails_closed() {
        let built = build_purchase_change_cancel_input(
            &runtime(blocked_instance(ApprovalBlockerCode::OpenTaskConflict), vec![]),
            "撤销",
            "u1",
            "key-1",
            None,
            Instant::from_unix_secs(20),
        )
        .unwrap();
        assert!(!built.close_open_task);
        assert!(!built.blocked_port);
        let error = prepare_cancel(built).unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message.contains("不可恢复原审批人的阻塞只能走受阻取消"))
        );
    }

    /// 空白撤回原因失败关闭。
    #[test]
    fn purchase_change_cancel_rejects_empty_reason() {
        let error = build_purchase_change_cancel_input(
            &runtime(running_instance(), vec![open_task(1)]),
            "   ",
            "u1",
            "key-1",
            None,
            Instant::from_unix_secs(20),
        )
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
    }

    /// 运行中零开放任务必须失败关闭，与采购单开放任务约束一致。
    #[test]
    fn purchase_change_cancel_running_without_open_task_fails_closed() {
        let current = current_execution();
        let instance = running_instance();
        let error = plan_cancel(CancelPlanInput {
            instance: &instance,
            current: &current,
            open_task_count: 0,
        })
        .unwrap_err();
        assert!(matches!(
            error,
            bpm::engine::EngineError::Model(bpm::model::types::ModelError::InvalidStatus(message))
                if message.contains("运行中审批实例必须恰有一个开放任务")
        ));
    }
}
