use crate::entity::document_registry::business_document::ApprovalDefinitionBinding;
use crate::entity::document_registry::{BusinessDocument, DocumentType};
use crate::repository::bpm::DefinitionGraph;
use crate::repository::{BpmExt, DocumentRegistryExt};
use bpm::ids::ApprovalProcessDefinitionId;

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::error::{Error, Result};
use crate::ports::{ApprovalObjectReadPort, PreparedWorkflowAudit};
use application_core::AuditActor;

use super::super::policy::{policy_of, require_process_required};
use super::super::process_kind::process_kind_of;
use super::revalidate::{revalidate_binding_graph, revalidate_published_graph};
use super::types::BindPublishedDefinitionCommand;
use super::{
    binding_decision, published_definition_or_not_configured, BindingDecision, DEFINITION_BOUND_AUDIT_ACTION,
    DEFINITION_POLICY_AUDIT_ACTION,
};

/// 创建单据时绑定当前发布定义。
///
/// `NO_APPROVAL` 返回空绑定并记录政策事实，不查询定义。
/// `PROCESS_REQUIRED` 查询唯一 `PUBLISHED` 定义并重验图与人员。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC
/// * `object_read` - 注入的对象读取端口
/// * `audit_port` - 审计写入端口
/// * `command` - 绑定命令
/// * `actor` - 已认证操作人
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 无审批返回 `None`；必须审批返回完整绑定。
///
/// # 错误
/// 缺失发布定义、图损坏、人员或范围失败时返回错误，调用方必须回滚。
pub async fn bind_published_definition_on_document_create(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalDefinitionBinding>> {
    let _ = command.business_object_version;
    let policy = policy_of(command.document_type)?;
    match binding_decision(policy.requirement()) {
        BindingDecision::SkipNoApproval => {
            record_no_approval_policy(db, audit_port, command, actor, executor).await?;
            Ok(None)
        }
        BindingDecision::RequirePublished => {
            let binding =
                bind_required_definition(db, rbac, object_read, audit_port, command, actor, executor).await?;
            Ok(Some(binding))
        }
    }
}

/// 将已计算绑定写入单据实体。
///
/// # 错误
/// 单据已有绑定时返回错误。
pub fn attach_published_binding(
    document: &mut BusinessDocument,
    binding: ApprovalDefinitionBinding,
) -> Result<ApprovalDefinitionBinding> {
    document.bind_approval_definition(binding.clone())?;
    Ok(binding)
}

/// 由发布定义构造初次绑定。
///
/// # 错误
/// 定义版本为零时返回错误。
pub fn binding_from_published(
    definition_id: ApprovalProcessDefinitionId,
    definition_version: u32,
    bound_at: Instant,
) -> Result<ApprovalDefinitionBinding> {
    ApprovalDefinitionBinding::new(definition_id, definition_version, bound_at).map_err(Into::into)
}

/// 必须审批路径：查询发布定义、重验并写绑定审计。
async fn bind_required_definition(
    db: &Database,
    rbac: &impl crate::ports::WorkflowAuthorizationPort,
    object_read: &dyn ApprovalObjectReadPort,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    let policy = require_process_required(command.document_type)?;
    let graph = load_published_graph(db, command.document_type, executor).await?;
    revalidate_binding_graph(db, rbac, object_read, &policy, &command.context, &graph, executor).await?;
    let binding = binding_from_published(
        ApprovalProcessDefinitionId::new(graph.definition.base.id.clone()),
        graph.definition.definition_version,
        Instant::now(),
    )?;
    write_bound_audit(db, audit_port, actor, command, &binding, None, executor).await?;
    Ok(binding)
}

/// 加载当前唯一已发布定义图。
pub(super) async fn load_published_graph(
    db: &Database,
    document_type: DocumentType,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = published_definition_or_not_configured(
        db.bpm_workflow()
            .load_published_definition_graph(process_kind_of(document_type), executor)
            .await?,
    )?;
    revalidate_published_graph(&graph)?;
    Ok(graph)
}

/// 记录无审批政策事实，不查询定义。
async fn record_no_approval_policy(
    db: &Database,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    tracing::info!(
        document_type = command.document_type.as_str(),
        business_object_id = command.business_object_id.as_str(),
        requirement = "NO_APPROVAL",
        "审批绑定政策：无需绑定"
    );
    let audit = PreparedWorkflowAudit::resource_with_message(
        actor.clone(),
        DEFINITION_POLICY_AUDIT_ACTION,
        "business_document",
        command.business_object_id.clone(),
        Some(format!(
            "requirement=NO_APPROVAL document_type={}",
            command.document_type.as_str()
        )),
    )?;
    let _ = db;
    audit_port.persist(&audit, executor).await?;
    Ok(())
}

/// 写入 `approval.definition.bound` 审计。
async fn write_bound_audit(
    db: &Database,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    actor: &AuditActor,
    command: &BindPublishedDefinitionCommand,
    binding: &ApprovalDefinitionBinding,
    previous: Option<&ApprovalDefinitionBinding>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let message = format!(
        "document_type={} definition_id={} version={} previous={:?} object_version={}",
        command.document_type.as_str(),
        binding.approval_process_definition_id.as_ref(),
        binding.approval_definition_version,
        previous.map(|item| item.approval_process_definition_id.as_ref().to_string()),
        command.business_object_version
    );
    let audit = PreparedWorkflowAudit::resource_with_message(
        actor.clone(),
        DEFINITION_BOUND_AUDIT_ACTION,
        "business_document",
        command.business_object_id.clone(),
        Some(message),
    )?;
    let _ = db;
    audit_port.persist(&audit, executor).await?;
    Ok(())
}

/// 读取注册行。
pub(super) async fn load_registered_document(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<BusinessDocument> {
    db.business_documents()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))
}
