use bpm::engine::DefinitionGraph;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::SubjectRef;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::BpmExt;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use application_core::AuditActor;
use erp_identity::SharedRbacService;
use erp_workflow::service::approval::execution::idempotency::{
    normalize_idempotency_key, payload_conflict_error, start_identity, start_scope_candidates, ReceiptBranch,
    StartIdentityParams,
};
use erp_workflow::service::approval::process_kind::process_kind_of;
use services::{Error, Result};

/// 在读取具体退款/冲正资源前先重验认证主体仍有效。
pub async fn ensure_return_start_actor_active(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    super::authorization::ensure_actor_active(db, rbac, actor, executor).await
}

/// 重放前在同一 fresh session 内重验账号、提交动作和对象读取 DataScope。
pub async fn ensure_return_start_replay_authorized(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    document_type: DocumentType,
    submit_permission: &str,
    organization_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    super::authorization::ensure_replay_authorized(
        db,
        rbac,
        actor,
        document_type,
        submit_permission,
        organization_id,
        executor,
    )
    .await
}

/// 顺序重试先查当前已冻结版本；草稿首次/重提再查严格下一版本。
pub fn replay_subject_versions(current: u32) -> Result<Vec<u32>> {
    let mut versions = Vec::with_capacity(2);
    if current > 0 {
        versions.push(current);
    }
    let next = current
        .checked_add(1)
        .ok_or_else(|| Error::ConflictError("审批主题版本已达上限".to_string()))?;
    if !versions.contains(&next) {
        versions.push(next);
    }
    Ok(versions)
}

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
pub async fn load_bound_definition_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
) -> Result<DefinitionGraph> {
    load_bound_definition_graph_with_executor(db, binding, &mut NoTransaction).await
}

/// 使用调用方执行器加载冻结绑定的定义图。
pub async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("客户退款单绑定的审批定义不存在".to_string()))?;
    Ok(engine_graph(graph))
}

/// 将仓储定义图转为引擎定义图。字段一一对应，不得在此补默认节点。
///
/// # 参数
/// * `graph` - 仓储一次批量读取结果
///
/// # 返回
/// 返回引擎可消费的定义图。
fn engine_graph(graph: erp_workflow::repository::bpm::DefinitionGraph) -> DefinitionGraph {
    DefinitionGraph {
        definition: graph.definition,
        nodes: graph.nodes,
        transitions: graph.transitions,
    }
}

/// 按精确单据类型依次读取当前 V3 与已知历史 StartApproval 作用域。
pub async fn load_start_receipt_for_document_type(
    db: &Database,
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(document_type);
    let scopes = start_scope_candidates(
        process_kind.as_str(),
        subject.subject_kind(),
        subject.subject_id(),
        subject_version,
    )?;
    for scope in scopes {
        let receipt = db
            .bpm_workflow()
            .find_command_receipt(
                bpm::model::types::ApprovalCommandKind::StartApproval,
                &scope,
                &key,
                &mut NoTransaction,
            )
            .await?;
        if receipt.is_some() {
            return Ok(receipt);
        }
    }
    Ok(None)
}

/// 退款/冲正启动收据回放入参。
pub struct ReplayReturnStartInput<'a> {
    /// 退款或冲正单据类型。
    pub document_type: DocumentType,
    /// 审批主题引用。
    pub subject: &'a SubjectRef,
    /// 主题版本。
    pub subject_version: u32,
    /// 启动幂等键。
    pub idempotency_key: &'a str,
    /// 冻结的审批定义绑定。
    pub binding: &'a ApprovalDefinitionBinding,
    /// 启动人账号 ID。
    pub actor_id: &'a str,
}

/// 在 fresh 事务快照内按完整 V3/legacy 身份回读已提交的退款/冲正启动结果。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `input` - 单据类型、主题、绑定与启动人
/// * `executor` - 当前事务执行器
///
/// # 返回
/// 命中同载荷收据时返回实例 ID；无收据时返回 `None`。
///
/// # 错误
/// 载荷冲突或收据与冻结运行事实不一致时返回冲突错误。
///
/// # 关键业务约束
/// 必须按完整 V3/legacy 身份候选范围回读，禁止只认单一 scope。
pub async fn replay_return_start_with_executor(
    db: &Database,
    input: ReplayReturnStartInput<'_>,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let key = normalize_idempotency_key(input.idempotency_key)?;
    let process_kind = process_kind_of(input.document_type);
    let identity = start_identity(StartIdentityParams {
        idempotency_key: key,
        process_kind: process_kind.as_str(),
        subject_kind: input.subject.subject_kind(),
        subject_id: input.subject.subject_id(),
        subject_version: input.subject_version,
        binding_id: input.binding.approval_process_definition_id.as_ref(),
        definition_version: input.binding.approval_definition_version,
        actor_participant_id: input.actor_id,
    })?;
    let mut receipt = None;
    for scope in identity.scope_candidates() {
        receipt = db
            .bpm_workflow()
            .find_command_receipt(
                bpm::model::types::ApprovalCommandKind::StartApproval,
                scope,
                identity.idempotency_key(),
                executor,
            )
            .await?;
        if receipt.is_some() {
            break;
        }
    }
    let receipt = match identity.classify(receipt.as_ref()) {
        ReceiptBranch::Fresh => return Ok(None),
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error().into()),
        ReceiptBranch::SamePayload(receipt) => receipt,
    };
    let instance = db
        .bpm_workflow()
        .find_instance_by_id(&ApprovalProcessInstanceId::new(&receipt.result_ref), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("退款/冲正启动收据引用的审批实例不存在".to_string()))?;
    if instance.base.id != receipt.result_ref
        || instance.process_kind != process_kind
        || instance.subject.subject_kind() != input.subject.subject_kind()
        || instance.subject.subject_id() != input.subject.subject_id()
        || instance.subject_version != input.subject_version
        || instance.started_by.as_str() != input.actor_id
        || instance.process_definition_id != input.binding.approval_process_definition_id
        || instance.definition_version != input.binding.approval_definition_version
    {
        return Err(Error::ConflictError(
            "退款/冲正启动收据与冻结运行事实不一致".to_string(),
        ));
    }
    Ok(Some(instance.base.id))
}
