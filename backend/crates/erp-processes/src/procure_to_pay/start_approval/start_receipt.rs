//! 采购启动收据与定义图加载：回放复用带 executor 版本。
//!
//! 本模块收拢定义图加载（无事务/带 executor 双版本）与启动收据读取/回放；
//! 创建并提交路径与单纯提交路径共享同一启动装配，不得各自复制收据读取。

use bpm::engine::DefinitionGraph;
use bpm::model::SubjectRef;
use erp_workflow::BpmExt;
use erp_workflow::entity::approval_integration::ApprovalStartIdentity;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::execution::idempotency::{
    normalize_idempotency_key, start_scope_candidates,
};
use erp_workflow::service::approval::process_kind::process_kind_of;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::approval_start::{StartReplayErrors, replay_start};
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
    Ok(graph)
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
///
/// # 参数
/// * `db` - 审批运行时数据库。
/// * `subject` - 采购单主体引用。
/// * `subject_version` - 冻结提交版本。
/// * `idempotency_key` - 尚未规范化的幂等键。
/// * `binding` - 创建时冻结的定义绑定。
/// * `actor_id` - 启动人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 没有匹配收据时返回 `None`；同键同载荷且运行事实一致时返回审批实例 ID。
///
/// # 错误
/// 幂等键非法、同键异载荷、收据引用的实例不存在、收据与冻结运行事实不一致，或仓储读取失败时返回错误。
pub(crate) async fn replay_purchase_order_start_with_executor(
    db: &Database,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
    binding: &ApprovalDefinitionBinding,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    replay_start(
        db,
        ApprovalStartIdentity {
            process_kind: process_kind_of(DocumentType::PurchaseOrder),
            subject,
            subject_version,
            binding,
            actor_id,
        },
        idempotency_key,
        StartReplayErrors {
            missing_instance: "采购启动收据引用的审批实例不存在",
            identity_mismatch: "采购启动收据与冻结运行事实不一致",
        },
        executor,
    )
    .await
}
