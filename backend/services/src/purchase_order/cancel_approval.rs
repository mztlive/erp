//! 采购单撤回：调用统一 `prepare_cancel`，再执行业务 `cancel_action`。

use bpm::engine::{plan_cancel, CancelPlan, CancelPlanInput, DefinitionGraph};
use bpm::ids::{ApprovalCommandReceiptId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, IdempotencyKey, ParticipantId, Timestamp};
use database::{AccessControlExt, BpmExt, NoTransaction, PurchaseOrderExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::purchase_order::PurchaseOrder;
use entities::work_item::WorkItem;
use id_generator::next_id;
use mongodb::Database;

use super::adapter::{
    execute_purchase_order_domain_action, purchase_order_adapter, purchase_order_subject_ref,
    require_frozen_binding,
};
use super::dto::CancelPurchaseOrderApprovalRequest;
use super::start_approval::load_bound_definition_graph;
use super::PurchaseOrderService;
use crate::approval::execution::authorization::converge_eligibility;
use crate::approval::execution::start::map_engine_error;
use crate::approval::execution::{
    claim_and_persist_document_cancel_runtime, command_may_have_committed, command_recovery_delay,
    normalize_document_cancel_reason, prepare_document_cancel, replay_committed_document_cancel,
    CancelExecutionInput, DocumentCancelCommand, DocumentCancelReplayProof, ExecutionCommandInput,
    PreparedExecution,
};
use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
use crate::document_registry::find_approval_binding;
use crate::errors::{Error, Result};
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use validator::Validate;

impl PurchaseOrderService {
    /// 撤回审批中的采购单，回到可修正草稿且 `subject_version` 不回退。
    ///
    /// 作为合同 `cancel_action`，供业务撤回与管理员受阻取消共用。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    /// * `req` - 撤回请求（原因必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 撤回成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 非审批中、已最终通过、原因缺失或并发冲突时返回错误。
    pub async fn cancel_approval(
        &self,
        id: &str,
        req: CancelPurchaseOrderApprovalRequest,
        actor: &AuditActor,
    ) -> Result<()> {
        req.validate()?;
        let mut order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        let subject = purchase_order_subject_ref(id)?;
        let command = DocumentCancelCommand::new(
            subject.clone(),
            order.approval_subject_version,
            req.expected_lock_version,
            &req.reason,
            actor.id(),
            &req.idempotency_key,
        )?;
        if replay_purchase_order_cancel(&self.db, &command).await?.is_some() {
            return Ok(());
        }
        self.ensure_version(&order, req.expected_lock_version)?;
        let adapter = purchase_order_adapter()?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let runtime =
            load_cancel_runtime(&self.db, &binding, &subject, order.approval_subject_version).await?;
        let now = Instant::now();
        let input = build_purchase_order_cancel_input(
            &runtime,
            command.reason(),
            actor.id(),
            command.idempotency_key(),
            None,
            now,
        )?;
        let prepared = prepare_document_cancel(input, command.expected_document_version())?;
        let submission_id = order.current_submission_id.clone().unwrap_or_default();
        execute_purchase_order_domain_action(
            &mut order,
            adapter.cancel_action,
            submission_id.as_str(),
            actor.id(),
        )?;
        let _ = ApprovalDomainAction::PurchaseOrderCancelApproval;
        let audit =
            actor
                .clone()
                .resource_log("purchase_order.cancel_approval", "purchase_order", id.to_string())?;
        let result = persist_purchase_order_cancel(
            &self.db,
            PurchaseOrderCancelPersistInput {
                order,
                prepared,
                open_tasks: runtime.open_tasks,
                actor_id: actor.id().to_string(),
                reason: command.reason().to_string(),
                now,
                audit,
            },
        )
        .await;
        match result {
            Ok(()) => Ok(()),
            Err(error) if command_may_have_committed(&error) => {
                recover_purchase_order_cancel(&self.db, &command, error).await
            }
            Err(error) => Err(error),
        }
    }
}

const CANCEL_RECOVERY_ATTEMPTS: usize = 8;

/// 使用独立 snapshot 会话回读采购单撤回 winner；本函数只读。
async fn replay_purchase_order_cancel(
    db: &Database,
    command: &DocumentCancelCommand,
) -> Result<Option<DocumentCancelReplayProof>> {
    let db = db.clone();
    let command = command.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move { replay_committed_document_cancel(&db, &command, session).await })
        })
        .await
}

/// 失败事务退出后只在新会话有限回读，不盲重跑 Fresh。
async fn recover_purchase_order_cancel(
    db: &Database,
    command: &DocumentCancelCommand,
    original_error: Error,
) -> Result<()> {
    for attempt in 0..CANCEL_RECOVERY_ATTEMPTS {
        match replay_purchase_order_cancel(db, command).await {
            Ok(Some(_)) => return Ok(()),
            Ok(None) => {}
            Err(error) if command_may_have_committed(&error) => {}
            Err(error) => return Err(error),
        }
        if attempt + 1 < CANCEL_RECOVERY_ATTEMPTS {
            tokio::time::sleep(command_recovery_delay(attempt)).await;
        }
    }
    Err(original_error)
}

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
/// `RUNNING` 必须恰有一个开放任务，`BLOCKED` 必须没有开放任务。
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
/// 人员失效走业务取消；非人员一致性 blocker 必须走受阻取消。
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
/// 返回可交给统一 `prepare_document_cancel` 的取消编排输入。
///
/// # 错误
/// 原因/幂等键非法、审批人引用无效或端口与 blocker 不匹配时返回错误。
///
/// # 关键业务约束
/// 状态、开放任务数量与 blocker 端口分类由 BPM `plan_cancel` 计算；本端口为
/// 业务单据普通撤回，受阻取消由统一取消端口承担。
pub fn build_purchase_order_cancel_input(
    runtime: &LoadedCancelRuntime,
    reason: &str,
    actor_id: &str,
    idempotency_key: &IdempotencyKey,
    receipt: Option<bpm::model::ApprovalCommandReceipt>,
    now: Instant,
) -> Result<CancelExecutionInput> {
    let reason = normalize_document_cancel_reason(reason)?;
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
            idempotency_key: idempotency_key.clone(),
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

/// 采购单撤回事务写入集合。
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
pub(super) struct PurchaseOrderCancelPersistInput {
    /// 已执行 `cancel_action` 的采购单。
    pub order: PurchaseOrder,
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

/// 在同一事务内应用取消计划、关闭任务并写回采购单。
///
/// # 用途
/// 撤回审批后原子写回运行事实与采购单。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 采购单、取消计划与开放任务
///
/// # 返回
/// 成功时无返回值。
///
/// # 错误
/// CAS 冲突或仓储失败时返回错误，事务回滚。
///
/// # 关键业务约束
/// Replay 不得重复关闭任务；Apply 必须关闭开放任务并写回草稿。
pub(super) async fn persist_purchase_order_cancel(
    db: &Database,
    input: PurchaseOrderCancelPersistInput,
) -> Result<()> {
    let PurchaseOrderCancelPersistInput {
        mut order,
        prepared,
        open_tasks,
        actor_id,
        reason,
        now,
        audit,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(());
    };
    let closed_tasks = WorkItem::close_all_for_approval_cancellation(open_tasks, &actor_id, &reason, now)?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                claim_and_persist_document_cancel_runtime(&db, &writes, &closed_tasks, session).await?;
                db.purchase_orders().update(&mut order, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

#[cfg(test)]
mod tests {
    use super::{build_purchase_order_cancel_input, map_cancel_plan_error, LoadedCancelRuntime};
    use bpm::engine::{plan_cancel, CancelPlanInput, EngineError};
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::types::{
        ApprovalBlockerCode, ApprovalExecutionAssignmentSource, ApprovalProcessInstanceStatus, ModelError,
    };
    use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance, NewNodeExecution, ParticipantId};
    use entities::common::time::Instant;

    use crate::approval::execution::{prepare_document_cancel, PreparedExecution};
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
            process_kind: bpm::model::ProcessKind::PurchaseOrder,
            subject: bpm::model::SubjectRef::new("purchase_order", "po-1").unwrap(),
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

    fn blocked_instance() -> ApprovalProcessInstance {
        let mut inst = running_instance();
        inst.enter_blocked(
            ApprovalBlockerCode::OpenTaskConflict,
            bpm::model::Timestamp::from_unix_secs(12).unwrap(),
        )
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

    /// 运行中撤回按 BPM 计划关闭唯一开放任务，并携带实例、执行与任务版本 CAS。
    #[test]
    fn purchase_order_cancel_uses_bpm_plan_for_open_task_close() {
        let instance = running_instance();
        let task = open_task(3);
        let input = build_purchase_order_cancel_input(
            &runtime(instance, vec![task]),
            " 撤销重提  ",
            "u1",
            &bpm::model::IdempotencyKey::parse("key-1").unwrap(),
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

    /// 非人员 blocker 不得走业务撤回端口；统一取消端口由 `prepare_cancel` 承担。
    #[test]
    fn purchase_order_cancel_keeps_document_withdraw_port() {
        let instance = blocked_instance();
        let built = build_purchase_order_cancel_input(
            &runtime(instance, vec![]),
            "撤销",
            "u1",
            &bpm::model::IdempotencyKey::parse("key-1").unwrap(),
            None,
            Instant::from_unix_secs(20),
        )
        .unwrap();
        assert!(!built.close_open_task);
        assert!(!built.blocked_port);
        let error = prepare_document_cancel(built, 1).unwrap_err();
        assert!(
            matches!(error, Error::ValidationError(message) if message.contains("不可恢复原审批人的阻塞只能走受阻取消"))
        );
    }

    /// 空白撤回原因失败关闭。
    #[test]
    fn purchase_order_cancel_rejects_empty_reason() {
        let instance = running_instance();
        let error = build_purchase_order_cancel_input(
            &runtime(instance, vec![open_task(1)]),
            "   ",
            "u1",
            &bpm::model::IdempotencyKey::parse("key-1").unwrap(),
            None,
            Instant::from_unix_secs(20),
        )
        .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
    }

    /// 取消计划状态错误映射保持冲突语义。
    #[test]
    fn cancel_plan_error_maps_to_conflict() {
        let error = map_cancel_plan_error(EngineError::Model(ModelError::InvalidStatus(
            "运行中审批实例必须恰有一个开放任务",
        )));
        assert!(matches!(error, Error::ConflictError(_)));
    }

    /// 运行实例状态不变式由 BPM 计划保证。
    #[test]
    fn purchase_order_cancel_plan_status_is_running() {
        let instance = running_instance();
        assert_eq!(instance.status, ApprovalProcessInstanceStatus::Running);
        assert!(matches!(
            prepare_document_cancel(
                build_purchase_order_cancel_input(
                    &runtime(instance, vec![open_task(1)]),
                    "撤销",
                    "u1",
                    &bpm::model::IdempotencyKey::parse("key-1").unwrap(),
                    None,
                    Instant::from_unix_secs(20),
                )
                .unwrap(),
                1,
            )
            .unwrap(),
            PreparedExecution::Apply(_)
        ));
    }
}
