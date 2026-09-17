//! 采购启动收据与定义图加载：回放复用带 executor 版本。
//!
//! 本模块收拢定义图加载（无事务/带 executor 双版本）与启动收据读取/回放；
//! 创建并提交路径与单纯提交路径共享同一启动装配，不得各自复制收据读取。

use bpm::engine::DefinitionGraph;
use bpm::ids::ApprovalProcessInstanceId;
use bpm::model::SubjectRef;
use erp_workflow::BpmExt;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::idempotency::{
    ReceiptBranch, StartIdentityParams, normalize_idempotency_key, payload_conflict_error, start_identity,
    start_scope_candidates,
};
use erp_workflow::service::approval::process_kind::process_kind_of;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

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

/// 使用调用方执行器加载绑定定义图，供创建并提交的同一事务复用。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回已持久化的定义图。
///
/// # 错误
/// 定义不存在或仓储失败时返回冲突或仓储错误。
///
/// # 关键业务约束
/// 新建采购单后立即提交时必须用同一事务会话读取绑定定义。
pub(crate) async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = db
        .bpm_workflow()
        .load_definition_graph(&binding.approval_process_definition_id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购单绑定的审批定义不存在".to_string()))?;
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
    DefinitionGraph { definition: graph.definition, nodes: graph.nodes, transitions: graph.transitions }
}

/// 读取同载荷启动收据；不存在时返回 `None`。
///
/// # 参数
/// * `db` - 数据库
/// * `subject` - 业务对象引用
/// * `subject_version` - 冻结提交版本
/// * `idempotency_key` - 调用方幂等键
///
/// # 返回
/// 已提交收据或空。
///
/// # 错误
/// 幂等键非法或仓储失败时返回错误。
pub(crate) async fn load_start_receipt(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(DocumentType::PurchaseOrder);
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

/// 在 fresh 事务快照内按完整 V3/legacy 身份回读已提交的采购启动结果。
pub(crate) async fn replay_purchase_order_start_with_executor(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    binding: &ApprovalDefinitionBinding,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let key = normalize_idempotency_key(idempotency_key)?;
    let process_kind = process_kind_of(DocumentType::PurchaseOrder);
    let identity = start_identity(StartIdentityParams {
        idempotency_key: key,
        process_kind: process_kind.as_str(),
        subject_kind: subject.subject_kind(),
        subject_id: subject.subject_id(),
        subject_version,
        binding_id: binding.approval_process_definition_id.as_ref(),
        definition_version: binding.approval_definition_version,
        actor_participant_id: actor_id,
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
        .ok_or_else(|| Error::ConflictError("采购启动收据引用的审批实例不存在".to_string()))?;
    if instance.base.id != receipt.result_ref
        || instance.process_kind != process_kind
        || instance.subject.subject_kind() != subject.subject_kind()
        || instance.subject.subject_id() != subject.subject_id()
        || instance.subject_version != subject_version
        || instance.started_by.as_str() != actor_id
        || instance.process_definition_id != binding.approval_process_definition_id
        || instance.definition_version != binding.approval_definition_version
    {
        return Err(Error::ConflictError("采购启动收据与冻结运行事实不一致".to_string()));
    }
    Ok(Some(instance.base.id))
}
