use application_core::AuditActor;
use bpm::engine::DefinitionGraph;
use bpm::model::{ApprovalCommandReceipt, SubjectRef};
use erp_approval_runtime::approver_directory::resolve_start_approvers;
use erp_workflow::entity::document_registry::DocumentType;
use erp_workflow::entity::document_registry::business_document::ApprovalDefinitionBinding;
use erp_workflow::service::approval::read;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use crate::{Error, Result, ReverseFlowPorts};

/// 在读取具体退款/冲正资源前先重验认证主体仍有效。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 共享 RBAC。
/// * `actor` - 待重验的操作人。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 账号仍有效时无返回值。
///
/// # 错误
/// 账号失效时返回 `Forbidden`。授权查询失败时返回对应错误。
pub async fn ensure_return_start_actor_active(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    super::authorization::ensure_actor_active(db, rbac, actor, executor).await
}

/// 重放前在同一事务内重验账号、提交动作和精确资金来源。
///
/// # 参数
/// * `db` - 数据库。
/// * `rbac` - 共享 RBAC。
/// * `actor` - 待重验的操作人。
/// * `document_type` - 退款或冲正单据类型。
/// * `submit_permission` - 提交动作权限码。
/// * `document_id` - 单据主键。
/// * `executor` - 调用方事务执行器。
///
/// # 返回
/// 授权通过时无返回值。
///
/// # 错误
/// 账号失效、缺少提交权限、无权读取资金来源、经办人不符或仓储失败时返回对应错误。
pub async fn ensure_return_start_replay_authorized(
    db: &Database,
    rbac: &ReverseFlowPorts,
    actor: &AuditActor,
    document_type: DocumentType,
    submit_permission: &str,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    super::authorization::ensure_replay_authorized(
        db,
        rbac,
        actor,
        document_type,
        submit_permission,
        document_id,
        executor,
    )
    .await
}

/// 加载绑定定义并解析新申请负责人；收据回放恢复原实例指派。
///
/// # 参数
/// * `db` - 数据库
/// * `binding` - 创建时冻结的定义绑定
/// * `actor` - 本次申请人
/// * `receipt` - 已提交启动收据，可空
///
/// # 返回
/// 返回已解析新人员或恢复原实例人员的启动图。
///
/// # 错误
/// 定义不存在或仓储失败时返回冲突或仓储错误。
pub async fn load_start_graph(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    actor: &str,
    receipt: Option<&ApprovalCommandReceipt>,
) -> Result<DefinitionGraph> {
    let graph = load_bound_definition_graph_with_executor(db, binding, &mut NoTransaction).await?;
    resolve_start_approvers(db, graph, actor, receipt, &mut NoTransaction).await.map_err(Into::into)
}

/// 使用调用方执行器加载冻结绑定的定义图。
///
/// # 参数
/// * `db` - 数据库。
/// * `binding` - 创建时冻结的定义绑定。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 返回引擎可消费的定义图。
///
/// # 错误
/// 定义不存在时返回冲突。仓储读取失败时返回对应错误。
pub async fn load_bound_definition_graph_with_executor(
    db: &Database,
    binding: &ApprovalDefinitionBinding,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    read::bound_definition_graph(db, binding, "客户退款单绑定的审批定义不存在", executor)
        .await
        .map_err(Error::from)
}

/// 按精确单据类型依次读取当前 V3 与已知历史 StartApproval 作用域。
///
/// # 参数
/// * `db` - 数据库。
/// * `document_type` - 退款或冲正单据类型。
/// * `subject` - 审批主题引用。
/// * `subject_version` - 主题版本。
/// * `idempotency_key` - 启动幂等键。
///
/// # 返回
/// 命中任一作用域时返回该收据；都未命中时返回 `None`。
///
/// # 错误
/// 幂等键不合法、作用域构造失败或收据读取失败时返回对应错误。
pub async fn load_start_receipt_for_document_type(
    db: &Database,
    document_type: DocumentType,
    subject: &SubjectRef,
    subject_version: u32,
    idempotency_key: &str,
) -> Result<Option<bpm::model::ApprovalCommandReceipt>> {
    read::start_receipt(db, document_type, subject, subject_version, idempotency_key, &mut NoTransaction)
        .await
        .map_err(Error::from)
}
