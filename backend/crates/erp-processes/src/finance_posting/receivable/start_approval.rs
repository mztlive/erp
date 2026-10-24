//! 客户回款提交启动：加载定义图、构造 `prepare_start` 输入并持久化运行事实。

use application_core::AuditActor;
use bpm::engine::{DefinitionGraph, StartAssigneeBinding};
use bpm::ids::{
    ApprovalCommandReceiptId, ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId,
};
use bpm::model::{ApprovalCommandReceipt, ParticipantId, SubjectRef, Timestamp};
use erp_core::common::time::Instant;
use erp_finance::entity::receivable::CustomerReceipt;
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::receipt_capacity::reserve_receipt;
use erp_read_models::workbench::capture_approval_display;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshotPayload;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::repository::prelude::*;
use erp_workflow::service::approval::execution::apply_plan::PlannedWrites;
use erp_workflow::service::approval::execution::authorization::{
    AuthorizationFailure, converge_node_eligibility,
};
use erp_workflow::service::approval::execution::idempotency::normalize_idempotency_key;
use erp_workflow::service::approval::execution::{
    ExecutionCommandInput, PreparedExecution, StartExecutionInput, map_receipt_first_write_error,
};
use erp_workflow::service::approval::process_kind::process_kind_of;
use erp_workflow::service::approval::read;
use erp_workflow::{BpmExt, DocumentRegistryExt};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::adapter::customer_receipt_object_readable;
use crate::approval_start::{
    StartTaskContext, persist_start_runtime, persist_start_snapshot, persist_start_tasks,
};
use crate::{Error, Result};

/// 加载绑定定义图。缺失时失败关闭，不得用空图启动。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
///
/// # 返回
/// 返回已持久化的定义图。
///
/// # 错误
/// 定义不存在或仓储失败时返回冲突或仓储错误。
pub(crate) async fn load_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
) -> Result<DefinitionGraph> {
    load_bound_definition_graph_with_executor(db, binding, &mut NoTransaction).await
}

/// 使用调用方执行器加载绑定定义图。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回已持久化的定义图。
///
/// # 错误
/// 定义不存在或仓储失败时返回错误。
pub(crate) async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    read::bound_definition_graph(db, binding, "客户回款单绑定的审批定义不存在", executor)
        .await
        .map_err(Error::from)
}

/// 读取候选启动收据；不存在时返回 `None`。
///
/// 本步骤只按命令类型、作用域与幂等键查找；载荷一致性由后续启动校验承担。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
///
/// # 返回
/// 尚待启动校验确认载荷一致性的候选收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(crate) async fn load_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<ApprovalCommandReceipt>> {
    load_start_receipt_with_executor(db, subject, subject_version, idempotency_key, &mut NoTransaction).await
}

/// 使用调用方执行器读取候选启动收据。
///
/// 本步骤不比较载荷；调用方须通过后续启动校验确认候选收据与请求一致。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 尚待启动校验确认载荷一致性的候选收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(crate) async fn load_start_receipt_with_executor(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    read::start_receipt(
        db,
        DocumentType::CustomerReceipt,
        subject,
        subject_version,
        idempotency_key,
        executor,
    )
    .await
    .map_err(Error::from)
}

/// 客户回款启动输入。
///
/// # 用途
/// 收拢 `build_document_start_input` 的定义图、绑定与提交人参数。
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
/// 审批人取自已发布节点，不接受客户端选择。
pub(crate) struct DocumentStartInput<'a> {
    /// 必须审批的业务类型。
    pub document_type: DocumentType,
    /// 绑定定义图。
    pub graph: DefinitionGraph,
    /// 冻结绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 提交人。
    pub actor_id: &'a str,
    /// 单据责任组织。
    pub organization_id: &'a str,
    /// 规范化前的幂等键。
    pub idempotency_key: &'a str,
    /// 已存在收据。
    pub receipt: Option<bpm::model::ApprovalCommandReceipt>,
    /// 调用方时间。
    pub now: Instant,
}

/// 由定义图与单据组织构造启动输入。
///
/// 审批人取自已发布节点，不接受客户端选择。对象读取权失败时收敛为 BLOCKED。
///
/// # 用途
/// 把启动参数收敛为引擎 `prepare_start` 输入。
///
/// # 参数
/// * `input` - 定义图、绑定、主体与提交人
///
/// # 返回
/// 返回可交给 `prepare_start` 的输入。
///
/// # 错误
/// 入口缺失、审批人非法、幂等键非法或读取权校验失败时返回错误。
///
/// # 关键业务约束
/// 定义版本必须与冻结绑定一致；对象读取权失败时收敛为 BLOCKED。
pub(crate) fn build_document_start_input(input: DocumentStartInput<'_>) -> Result<StartExecutionInput> {
    let DocumentStartInput {
        document_type,
        graph,
        binding,
        subject,
        subject_version,
        actor_id,
        organization_id,
        idempotency_key,
        receipt,
        now,
    } = input;
    if graph.definition.definition_version != binding.approval_definition_version {
        return Err(Error::ConflictError("客户回款单绑定定义版本与已加载定义不一致".to_string()));
    }
    let idempotency_key = normalize_idempotency_key(idempotency_key)?;
    let actor =
        ParticipantId::new(actor_id).map_err(|_| Error::ValidationError("提交人引用无效".to_string()))?;
    let timestamp = Timestamp::from_utc(now.as_utc());
    let bindings = start_bindings_from_graph(&graph, organization_id)?;
    let entry = graph.entry_node().map_err(|_| Error::ConflictError("审批定义缺少入口节点".to_string()))?;
    let entry_eligibility = bindings
        .iter()
        .find(|item| item.node_key == entry.node_key)
        .map(|item| item.eligibility.clone())
        .ok_or_else(|| Error::ConflictError("入口节点缺少审批人绑定".to_string()))?;
    Ok(StartExecutionInput {
        command: ExecutionCommandInput {
            graph,
            current_eligibility: entry_eligibility.clone(),
            next_eligibility: entry_eligibility,
            receipt,
            idempotency_key,
            now: timestamp,
        },
        process_kind: process_kind_of(document_type),
        subject,
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref().to_string(),
        definition_version: binding.approval_definition_version,
        actor,
        instance_id: ApprovalProcessInstanceId::new(next_id()),
        entry_execution_id: ApprovalNodeExecutionId::new(next_id()),
        receipt_id: ApprovalCommandReceiptId::new(next_id()),
        bindings,
    })
}

/// 为定义全部节点冻结启动绑定，并按单据组织重验对象读取权。
///
/// # 参数
/// * `graph` - 定义图
/// * `organization_id` - 单据责任组织
///
/// # 返回
/// 返回与节点一一对应的绑定。
///
/// # 错误
/// 节点审批人引用非法或显示名为空时返回校验错误。
fn start_bindings_from_graph(
    graph: &DefinitionGraph,
    organization_id: &str,
) -> Result<Vec<StartAssigneeBinding>> {
    let mut bindings = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        bindings.push(StartAssigneeBinding {
            id: ApprovalInstanceAssigneeId::new(next_id()),
            node_key: node.node_key.clone(),
            participant: node.assignee().map_err(|e| Error::ValidationError(e.to_string()))?.clone(),
            eligibility: converge_node_eligibility(node, |assignee| match customer_receipt_object_readable(
                organization_id,
                assignee,
            ) {
                Ok(_) => None,
                Err(_) => Some(AuthorizationFailure::CannotReadSubject),
            })?,
        });
    }
    if bindings.is_empty() {
        return Err(Error::ConflictError("审批定义没有节点，无法启动客户回款审批".to_string()));
    }
    Ok(bindings)
}

/// 客户回款启动事务写入集合。
///
/// # 用途
/// 收拢提交后需一并写入的回款单、快照、启动计划与审计身份。
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
/// 运行事实、不可变快照与入口任务必须与单据迁移同事务。
pub(crate) struct CustomerReceiptStartPersistInput {
    /// 已进入 `IN_APPROVAL` 的回款单。
    pub receipt: CustomerReceipt,
    /// 审计操作人。
    pub actor: AuditActor,
    /// 回款单主键。
    pub id: String,
    /// 冻结快照载荷。
    pub snapshot_payload: ApprovalSubjectSnapshotPayload,
    /// `prepare_start` 结果。
    pub prepared: PreparedExecution,
    /// 合同签署的责任角色。
    pub owner_role: &'static str,
    /// 责任组织。
    pub organization_id: String,
    /// 调用方时间。
    pub now: Instant,
}

/// 在调用方事务内写入回款审批启动事实。
///
/// # 参数
/// * `db` - 数据库
/// * `input` - 已进入审批的回款、快照与启动计划
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回更新后的回款实体。
///
/// # 错误
/// 运行事实或回款写入失败时返回错误。
///
/// # 关键业务约束
/// 独立提交通过上层新建事务，本方法的启动收据必须是整笔事务第一写。创建并
/// 提交由外层创建命令先完成自身幂等仲裁；本方法作为不可独立调用的审批子步骤，
/// 其写段固定为“启动收据 -> 注册行启动守卫 -> 回款单 -> BPM”。
/// 成功财务回执与业务事件由外层主命令在同一执行器写入，本子步骤不重复记录。
pub(crate) async fn persist_customer_receipt_start_apply(
    db: &Database,
    input: CustomerReceiptStartPersistInput,
    executor: &mut dyn Executor,
) -> Result<CustomerReceipt> {
    let CustomerReceiptStartPersistInput {
        receipt,
        actor: _,
        id: _,
        snapshot_payload,
        prepared,
        owner_role,
        organization_id,
        now,
    } = input;
    let PreparedExecution::Apply(writes) = prepared else {
        return Ok(receipt);
    };
    db.bpm_workflow()
        .insert_command_receipt(&writes.receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    let guarded = db
        .business_documents()
        .mark_approval_started(
            writes.instance.subject.subject_id(),
            DocumentType::CustomerReceipt,
            &writes.instance.process_definition_id,
            writes.instance.definition_version,
            now,
            executor,
        )
        .await?;
    if guarded.is_none() {
        return Err(Error::ConflictError("客户回款单审批启动守卫冲突，请刷新后重试".to_string()));
    }
    reserve_receipt(db, &receipt, executor).await?;
    // 先在同一事务写入提交字段，快照读取才能包含本次核销安排。
    let mut receipt = receipt;
    db.customer_receipts().update(&mut receipt, executor).await?;
    persist_runtime_writes(
        db,
        &writes,
        RuntimeSubject { document_type: DocumentType::CustomerReceipt, snapshot_payload: &snapshot_payload },
        owner_role,
        &organization_id,
        now,
        executor,
    )
    .await?;
    Ok(receipt)
}

/// 启动时固定的业务类型及提交快照。
pub(crate) struct RuntimeSubject<'a> {
    pub document_type: DocumentType,
    pub snapshot_payload: &'a ApprovalSubjectSnapshotPayload,
}

/// 将启动计划写入 BPM 集合、不可变快照和入口 WorkItem。
///
/// # 参数
/// * `db` - 数据库。
/// * `writes` - 已规划的启动写入。
/// * `subject` - 单据类型与提交快照。
/// * `owner_role` - 入口任务责任角色。
/// * `organization_id` - 单据责任组织。
/// * `now` - 启动时刻。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 运行时、快照和入口任务写入成功时无返回值。
///
/// # 错误
/// 计划缺少入口执行、快照构造、展示捕获或任一步写入失败时返回错误。
pub(crate) async fn persist_runtime_writes(
    db: &Database,
    writes: &PlannedWrites,
    subject: RuntimeSubject<'_>,
    owner_role: &str,
    organization_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let RuntimeSubject { document_type, snapshot_payload } = subject;
    let mut snapshot = persist_start_runtime(
        db,
        writes,
        document_type,
        snapshot_payload,
        now,
        "启动计划缺少入口执行，不得提交客户回款",
        executor,
    )
    .await?;
    snapshot.display = Some(
        capture_approval_display(db, snapshot.document_type, &snapshot.business_object_id, executor).await?,
    );
    persist_start_snapshot(db, snapshot, executor).await?;
    persist_start_tasks(
        db,
        writes,
        StartTaskContext { document_type, owner_role, organization_id, now },
        executor,
    )
    .await
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use bpm::ids::{ApprovalNodeExecutionId, ApprovalProcessInstanceId};
    use bpm::model::{ApprovalNodeExecution, NewNodeExecution, ParticipantId, Timestamp};
    use erp_core::common::time::Instant;
    use erp_workflow::repository::bpm::ApprovalInstanceListProjection;

    fn execution() -> ApprovalNodeExecution {
        ApprovalNodeExecution::new_active(NewNodeExecution {
            id: ApprovalNodeExecutionId::new("e1"),
            process_instance_id: ApprovalProcessInstanceId::new("inst-1"),
            node_key: "n1".into(),
            node_name: "财务复核".into(),
            round_no: 1,
            execution_no: 1,
            assignment_source: bpm::model::types::ApprovalExecutionAssignmentSource::Definition,
            replaces_execution_id: None,
            assignee_participant_id: ParticipantId::new("u1").unwrap(),
            assignee_name_snapshot: "张三".into(),
            at: Timestamp::from_unix_secs(10).unwrap(),
        })
        .expect("入口执行夹具")
    }

    /// 列表投影必须来自入口执行，不得推断未知审批人。
    #[test]
    fn list_projection_copies_entry_assignee() {
        let projection =
            ApprovalInstanceListProjection::from_entry_execution(&execution(), Instant::from_unix_secs(10));
        assert_eq!(projection.current_node_key.as_deref(), Some("n1"));
        assert_eq!(projection.current_assignee_participant_id.as_deref(), Some("u1"));
        assert_eq!(projection.current_assignee_name.as_deref(), Some("张三"));
        assert_eq!(projection.last_status_changed_at, Some(10));
    }
}
