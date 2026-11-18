use application_core::AuditActor;
use bpm::ids::ApprovalProcessDefinitionId;
use erp_core::common::time::Instant;
use erp_identity::{AuthorizationService, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;

use super::super::policy::{policy_of, require_process_required};
use super::super::process_kind::process_kind_of;
use super::revalidate::{revalidate_binding_graph, revalidate_published_graph};
use super::types::BindPublishedDefinitionCommand;
use super::{
    BindingDecision, DEFINITION_BOUND_AUDIT_ACTION, DEFINITION_POLICY_AUDIT_ACTION, binding_decision,
    published_definition_or_not_configured,
};
use crate::entity::document_registry::business_document::ApprovalDefinitionBinding;
use crate::entity::document_registry::{BusinessDocument, DocumentType};
use crate::error::{Error, Result};
use crate::ports::PreparedWorkflowAudit;
use crate::repository::bpm::DefinitionGraph;
use crate::repository::{BpmExt, DocumentRegistryExt};

/// 创建单据时在调用方写事务固定审批政策。无审批类型只记录政策事实并返回 `None`；必须审批类型读取唯一发布定义，重验图与审批人并返回完整绑定。
///
/// 缺少发布定义、结构损坏或人员与数据资格检查失败时，调用方必须回滚创建事务。
pub async fn bind_published_definition_on_document_create(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
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
        },
        BindingDecision::RequirePublished => {
            let binding = bind_required_definition(
                authorization,
                identity_service,
                db,
                audit_port,
                command,
                actor,
                executor,
            )
            .await?;
            Ok(Some(binding))
        },
    }
}

/// 将已确认的发布定义绑定写入注册单据，供随后提交按冻结定义运行。已有绑定时拒绝覆盖，成功返回写入的同一绑定。
pub fn attach_published_binding(
    document: &mut BusinessDocument,
    binding: ApprovalDefinitionBinding,
) -> Result<ApprovalDefinitionBinding> {
    document.bind_approval_definition(binding.clone())?;
    Ok(binding)
}

/// 为初次绑定冻结已发布定义的 ID、正版本和绑定时刻，防止之后发布变更影响该单据；版本为零时拒绝。
pub fn binding_from_published(
    definition_id: ApprovalProcessDefinitionId,
    definition_version: u32,
    bound_at: Instant,
) -> Result<ApprovalDefinitionBinding> {
    ApprovalDefinitionBinding::new(definition_id, definition_version, bound_at).map_err(Into::into)
}

// 必须审批的单据创建时，在当前事务加载唯一发布定义，重验图及人员资格，再签发绑定并写审计。
async fn bind_required_definition(
    authorization: &AuthorizationService,
    identity_service: &SharedRbacService,
    db: &Database,
    audit_port: &dyn crate::ports::WorkflowAuditPort,
    command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    let policy = require_process_required(command.document_type)?;
    let graph = load_published_graph(db, command.document_type, executor).await?;
    revalidate_binding_graph(
        authorization,
        db,
        identity_service,
        &policy,
        &command.context,
        &graph,
        executor,
    )
    .await?;

    let binding = binding_from_published(
        ApprovalProcessDefinitionId::new(graph.definition.base.id.clone()),
        graph.definition.definition_version,
        Instant::now(),
    )?;
    write_bound_audit(db, audit_port, actor, command, &binding, None, executor).await?;
    Ok(binding)
}

// 加载当前唯一已发布定义图。
// 返回已通过发布图重验的定义图。
//
// 仓储读取失败、缺少发布定义或图重验失败时返回错误。
pub(super) async fn load_published_graph(
    db: &Database,
    document_type: DocumentType,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let graph = published_definition_or_not_configured(
        db.bpm_workflow().load_published_definition_graph(process_kind_of(document_type), executor).await?,
    )?;
    revalidate_published_graph(&graph)?;
    Ok(graph)
}

// 无审批单据创建时只在原事务记录政策事实，不选择或加载定义图。
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
        Some(format!("requirement=NO_APPROVAL document_type={}", command.document_type.as_str())),
    )?;
    let _ = db;
    audit_port.persist(&audit, executor).await?;
    Ok(())
}

// 确认发布定义绑定后沿原事务写入 `approval.definition.bound`，记录新旧绑定及当前单据版本供追溯。
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

// 读取注册行。
// 返回已注册的业务单据。
//
// 单据未注册时返回未找到；仓储读取失败时返回对应错误。
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
