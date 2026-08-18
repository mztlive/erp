//! 单据与当前已发布定义的统一绑定端口。
//!
//! 端口接收调用方 `Executor`，不得自行开嵌套事务，也不得把执行器传入 BPM。

use bpm::graph::{generate_linear_transitions, validate_entry_node, validate_transition};
use bpm::ids::ApprovalProcessDefinitionId;
use bpm::model::types::ModelError;
use bpm::model::{ApprovalNodeDefinition, ApprovalTransitionDefinition};
use database::repository::bpm::DefinitionGraph;
use database::{AccessControlExt, BpmExt, DocumentRegistryExt, Executor};
use entities::common::time::Instant;
use entities::document_registry::business_document::ApprovalDefinitionBinding;
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::AccountCore;
use mongodb::bson::doc;
use mongodb::Database;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::{subject, SharedRbacService};

use super::business_adapter::{
    adapter_spec_of, assignee_ids_of, ensure_binding_scope, ensure_published_status,
    ensure_separation_of_duties, subject_ref_for, BindingRevalidationContext,
};
use super::policy::{
    policy_of, require_process_required, ApprovalRequirement, ApproverEligibilityPolicy,
    ProcessRequiredApprovalPolicy, STATIC_APPROVE_PERMISSION,
};
use super::process_kind::process_kind_of;

/// 必须审批但无可绑定发布定义。
pub const APPROVAL_PROCESS_NOT_CONFIGURED: &str = "APPROVAL_PROCESS_NOT_CONFIGURED";
/// 绑定审计动作。
pub const DEFINITION_BOUND_AUDIT_ACTION: &str = "approval.definition.bound";
/// 无审批政策事实审计动作。
pub const DEFINITION_POLICY_AUDIT_ACTION: &str = "approval.definition.policy";
/// 未提交升级审计动作。
pub const DEFINITION_UPGRADED_AUDIT_ACTION: &str = "approval.definition.upgraded";

const MAX_DEFINITION_NODES: usize = 20;

/// 创建时绑定命令。客户端不得提交定义 ID。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindPublishedDefinitionCommand {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 业务对象主键。
    pub business_object_id: String,
    /// 业务对象乐观锁版本。
    pub business_object_version: u64,
    /// 当前单据组织与创建人。
    pub context: BindingRevalidationContext,
}

/// 未提交单据升级命令。目标固定为当前发布版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeUnsubmittedDefinitionCommand {
    /// 业务单据注册 ID。
    pub document_id: String,
    /// 期望的注册行版本。
    pub expected_document_version: u64,
    /// 期望的绑定 CAS 版本。
    pub expected_binding_version: u64,
    /// 升级原因。
    pub reason: String,
    /// 当前单据组织与创建人。
    pub context: BindingRevalidationContext,
}

/// 绑定政策决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingDecision {
    /// 无审批：不查询定义、不写绑定。
    SkipNoApproval,
    /// 必须审批：查询唯一 `PUBLISHED` 定义。
    RequirePublished,
}

/// 按政策决定是否查询发布定义。
///
/// # 返回
/// `NO_APPROVAL` 跳过；`PROCESS_REQUIRED` 必须绑定。
pub fn binding_decision(requirement: ApprovalRequirement) -> BindingDecision {
    match requirement {
        ApprovalRequirement::NoApproval => BindingDecision::SkipNoApproval,
        ApprovalRequirement::ProcessRequired => BindingDecision::RequirePublished,
    }
}

/// 必须审批且缺失发布定义时失败关闭。
///
/// # 错误
/// 返回 `APPROVAL_PROCESS_NOT_CONFIGURED`。
pub fn published_definition_or_not_configured<T>(published: Option<T>) -> Result<T> {
    published.ok_or_else(process_not_configured)
}

/// 构造未配置流程的稳定冲突。
///
/// # 返回
/// 返回 `APPROVAL_PROCESS_NOT_CONFIGURED`。
pub fn process_not_configured() -> Error {
    Error::ConflictError(APPROVAL_PROCESS_NOT_CONFIGURED.to_string())
}

/// 创建单据时绑定当前发布定义。
///
/// `NO_APPROVAL` 返回空绑定并记录政策事实，不查询定义。
/// `PROCESS_REQUIRED` 查询唯一 `PUBLISHED` 定义并重验图与人员。
///
/// # 参数
/// * `db` - 数据库
/// * `rbac` - 共享 RBAC
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
    rbac: &SharedRbacService,
    command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalDefinitionBinding>> {
    let _ = command.business_object_version;
    let policy = policy_of(command.document_type)?;
    match binding_decision(policy.requirement()) {
        BindingDecision::SkipNoApproval => {
            record_no_approval_policy(db, command, actor, executor).await?;
            Ok(None)
        }
        BindingDecision::RequirePublished => {
            let binding = bind_required_definition(db, rbac, command, actor, executor).await?;
            Ok(Some(binding))
        }
    }
}

/// 升级未提交且未启动单据的绑定到当前发布版本。
///
/// 仅运行管理员可调用；目标固定当前 `PUBLISHED`，禁止客户端提交定义 ID。
/// 使用单据版本与 `approval_binding_version` 双 CAS。
///
/// # 错误
/// 权限、状态、版本或人员重验失败时返回错误。
pub async fn upgrade_unsubmitted_document_definition(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    let mut document = load_registered_document(db, &command.document_id, executor).await?;
    let policy = require_process_required(document.document_type)?;
    ensure_runtime_admin(rbac, actor, &policy).await?;
    let current = document
        .approval_binding
        .clone()
        .ok_or_else(|| Error::ValidationError("尚未绑定审批定义".to_string()))?;
    ensure_upgrade_unsubmitted_allowed(
        document.formalized_at.is_some(),
        document_has_started_instance(db, document.document_type, &document.base.id, executor).await?,
        document.base.version,
        command.expected_document_version,
        current.approval_binding_version,
        command.expected_binding_version,
        true,
    )?;
    let published = load_published_graph(db, document.document_type, executor).await?;
    revalidate_binding_graph(db, rbac, &policy, &command.context, &published, executor).await?;
    apply_upgrade(&mut document, &published, &command.reason, actor)?;
    persist_upgraded_document(db, &mut document, &current, actor, executor).await
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

/// 未提交升级的纯闸门。
///
/// # 错误
/// 已提交、已启动、版本不匹配或非管理员时返回错误。
pub fn ensure_upgrade_unsubmitted_allowed(
    formalized: bool,
    started: bool,
    document_version: u64,
    expected_document_version: u64,
    binding_version: u64,
    expected_binding_version: u64,
    is_runtime_admin: bool,
) -> Result<()> {
    if !is_runtime_admin {
        return Err(Error::Forbidden("没有该单据类型的审批运行管理权限".to_string()));
    }
    if formalized {
        return Err(Error::ConflictError("已提交单据不能升级审批绑定".to_string()));
    }
    if started {
        return Err(Error::ConflictError("已启动单据不能升级审批绑定".to_string()));
    }
    if document_version != expected_document_version || binding_version != expected_binding_version {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 必须审批路径：查询发布定义、重验并写绑定审计。
async fn bind_required_definition(
    db: &Database,
    rbac: &SharedRbacService,
    command: &BindPublishedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    let policy = require_process_required(command.document_type)?;
    let graph = load_published_graph(db, command.document_type, executor).await?;
    revalidate_binding_graph(db, rbac, &policy, &command.context, &graph, executor).await?;
    let binding = binding_from_published(
        ApprovalProcessDefinitionId::new(graph.definition.base.id.clone()),
        graph.definition.definition_version,
        Instant::now(),
    )?;
    write_bound_audit(db, actor, command, &binding, None, executor).await?;
    Ok(binding)
}

/// 加载当前唯一已发布定义图。
async fn load_published_graph(
    db: &Database,
    document_type: DocumentType,
    executor: &mut dyn Executor,
) -> Result<DefinitionGraph> {
    let published = published_definition_or_not_configured(
        db.bpm_workflow()
            .find_published_by_process_kind(process_kind_of(document_type), executor)
            .await?,
    )?;
    ensure_published_status(published.status)?;
    let graph = db
        .bpm_workflow()
        .load_definition_graph(
            &ApprovalProcessDefinitionId::new(published.base.id.clone()),
            executor,
        )
        .await?
        .ok_or_else(process_not_configured)?;
    revalidate_published_graph(&graph)?;
    Ok(graph)
}

/// 复用图原语重验发布结构；不得把 Executor 传入 BPM。
fn revalidate_published_graph(graph: &DefinitionGraph) -> Result<()> {
    ensure_published_status(graph.definition.status)?;
    let nodes = sorted_nodes(&graph.nodes)?;
    ensure_node_count(nodes.len())?;
    let keys = nodes.iter().map(|node| node.node_key.clone()).collect::<Vec<_>>();
    validate_entry_node(&graph.definition.entry_node_key, &keys).map_err(map_model_error)?;
    if graph.definition.entry_node_key.trim() != keys[0] {
        return Err(Error::ValidationError("入口必须是顺序第一节点".to_string()));
    }
    let expected = generate_linear_transitions(&keys).map_err(map_model_error)?;
    ensure_transitions_match(&graph.transitions, &expected)?;
    for transition in &graph.transitions {
        validate_transition(transition).map_err(map_model_error)?;
    }
    Ok(())
}

/// Adapter 重验指定用户、权限、DataScope、读取权与岗位分离。
async fn revalidate_binding_graph(
    db: &Database,
    rbac: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    context: &BindingRevalidationContext,
    graph: &DefinitionGraph,
    executor: &mut dyn Executor,
) -> Result<()> {
    let spec = adapter_spec_of(policy.document_type)?;
    let assignee_ids = assignee_ids_of(graph);
    ensure_separation_of_duties(
        policy.separation_of_duties_policy,
        &context.creator_id,
        &assignee_ids,
    )?;
    for user_id in &assignee_ids {
        revalidate_one_assignee(db, rbac, policy, context, user_id, executor).await?;
        let scopes = db
            .data_scopes()
            .list_by_subject(
                entities::access_control::DataScopeSubjectType::User,
                user_id,
                executor,
            )
            .await?;
        ensure_binding_scope(&spec, &scopes, &context.organization_id, true)?;
    }
    Ok(())
}

/// 重验单个指定审批人的账号、静态权限与组织范围。
async fn revalidate_one_assignee(
    db: &Database,
    rbac: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    context: &BindingRevalidationContext,
    user_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let account = db
        .accounts()
        .find_by_id(user_id, executor)
        .await?
        .ok_or_else(|| Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string()))?;
    ensure_assignee_ready(&account)?;
    ensure_static_decide_permission(rbac, &account).await?;
    let _ = context;
    let _ = policy;
    Ok(())
}

/// 后台有效账号闸门。
fn ensure_assignee_ready(account: &AccountCore) -> Result<()> {
    match ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission {
        ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission => {}
    }
    if account.is_kind(entities::AccountKind::Admin) && account.can_login() {
        return Ok(());
    }
    Err(Error::ValidationError(
        "指定审批人账号不存在、已停用或任职失效".to_string(),
    ))
}

/// 重验静态 `approval_instance:decide`。
async fn ensure_static_decide_permission(rbac: &SharedRbacService, account: &AccountCore) -> Result<()> {
    let permission = entities::Permission::parse(STATIC_APPROVE_PERMISSION)
        .map_err(|error| Error::Internal(format!("静态审批权限不变量损坏: {error}")))?;
    let allowed = rbac
        .enforce(&subject(account.kind, &account.base.id), &permission)
        .await?;
    if allowed {
        return Ok(());
    }
    Err(Error::ValidationError("指定审批人缺少审批权限".to_string()))
}

/// 校验运行管理员权限。
async fn ensure_runtime_admin(
    rbac: &SharedRbacService,
    actor: &AuditActor,
    policy: &ProcessRequiredApprovalPolicy,
) -> Result<()> {
    let allowed = rbac
        .enforce(
            &subject(actor.kind(), actor.id()),
            &policy.runtime_admin_permission,
        )
        .await?;
    if allowed {
        return Ok(());
    }
    Err(Error::Forbidden("没有该单据类型的审批运行管理权限".to_string()))
}

/// 记录无审批政策事实，不查询定义。
async fn record_no_approval_policy(
    db: &Database,
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
    let audit = actor.clone().resource_log_with_message(
        DEFINITION_POLICY_AUDIT_ACTION,
        "business_document",
        command.business_object_id.clone(),
        Some(format!(
            "requirement=NO_APPROVAL document_type={}",
            command.document_type.as_str()
        )),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

/// 写入 `approval.definition.bound` 审计。
async fn write_bound_audit(
    db: &Database,
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
    let audit = actor.clone().resource_log_with_message(
        DEFINITION_BOUND_AUDIT_ACTION,
        "business_document",
        command.business_object_id.clone(),
        Some(message),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

/// 应用升级值对象并写原因。
fn apply_upgrade(
    document: &mut BusinessDocument,
    graph: &DefinitionGraph,
    reason: &str,
    actor: &AuditActor,
) -> Result<()> {
    if reason.trim().is_empty() {
        return Err(Error::ValidationError("升级原因不能为空".to_string()));
    }
    let expected = document
        .approval_binding
        .as_ref()
        .map(|binding| binding.approval_binding_version)
        .ok_or_else(|| Error::ValidationError("尚未绑定审批定义".to_string()))?;
    document.upgrade_approval_binding(
        ApprovalProcessDefinitionId::new(graph.definition.base.id.clone()),
        graph.definition.definition_version,
        expected,
        Instant::now(),
    )?;
    let _ = actor;
    Ok(())
}

/// 持久化升级后的注册行并写升级审计。
async fn persist_upgraded_document(
    db: &Database,
    document: &mut BusinessDocument,
    previous: &ApprovalDefinitionBinding,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalDefinitionBinding> {
    let binding = document
        .approval_binding
        .clone()
        .ok_or_else(|| Error::Internal("升级后绑定丢失".to_string()))?;
    db.business_documents().update(document, executor).await?;
    let message = format!(
        "from_definition={} from_version={} to_definition={} to_version={} actor={}",
        previous.approval_process_definition_id.as_ref(),
        previous.approval_definition_version,
        binding.approval_process_definition_id.as_ref(),
        binding.approval_definition_version,
        actor.id()
    );
    let audit = actor.clone().resource_log_with_message(
        DEFINITION_UPGRADED_AUDIT_ACTION,
        "business_document",
        document.base.id.clone(),
        Some(message),
    )?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(binding)
}

/// 读取注册行。
async fn load_registered_document(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<BusinessDocument> {
    db.business_documents()
        .find_by_id(document_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))
}

/// 是否已存在审批实例。
async fn document_has_started_instance(
    db: &Database,
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let subject = subject_ref_for(document_type, document_id)?;
    let found = db
        .approval_process_instances()
        .find_one(
            doc! {
                "subject.subject_kind": subject.subject_kind(),
                "subject.subject_id": subject.subject_id(),
            },
            executor,
        )
        .await?;
    Ok(found.is_some())
}

/// 按展示顺序排序节点。
fn sorted_nodes(nodes: &[ApprovalNodeDefinition]) -> Result<Vec<ApprovalNodeDefinition>> {
    let mut nodes = nodes.to_vec();
    nodes.sort_by_key(|node| node.display_order);
    for (index, node) in nodes.iter().enumerate() {
        let expected =
            u32::try_from(index + 1).map_err(|_| Error::ValidationError("节点顺序溢出".to_string()))?;
        if node.display_order != expected {
            return Err(Error::ValidationError(
                "节点顺序必须从 1 连续且无重复".to_string(),
            ));
        }
    }
    Ok(nodes)
}

/// 节点数量必须在 1..=20。
fn ensure_node_count(count: usize) -> Result<()> {
    if (1..=MAX_DEFINITION_NODES).contains(&count) {
        return Ok(());
    }
    Err(Error::ValidationError("发布定义图节点数量非法".to_string()))
}

/// 比较已存连线与线性生成器。
fn ensure_transitions_match(
    actual: &[ApprovalTransitionDefinition],
    expected: &[bpm::graph::LinearTransitionDraft],
) -> Result<()> {
    if actual.len() != expected.len() {
        return Err(Error::ValidationError("连线与线性生成器结果不一致".to_string()));
    }
    let mut actual_keys = actual
        .iter()
        .map(|item| {
            (
                item.from_node_key.clone(),
                item.event.as_str(),
                item.to_node_key.clone(),
                item.terminal_result.map(|value| value.as_str()),
            )
        })
        .collect::<Vec<_>>();
    let mut expected_keys = expected
        .iter()
        .map(|draft| {
            (
                draft.from_node_key.clone(),
                draft.event.as_str(),
                draft.to_node_key.clone(),
                draft.terminal_result.map(|item| item.as_str()),
            )
        })
        .collect::<Vec<_>>();
    actual_keys.sort();
    expected_keys.sort();
    if actual_keys == expected_keys {
        return Ok(());
    }
    Err(Error::ValidationError("连线与线性生成器结果不一致".to_string()))
}

/// 映射 BPM 模型错误。
fn map_model_error(error: ModelError) -> Error {
    match error {
        ModelError::InvalidField(message) | ModelError::InvalidTransition(message) => {
            Error::ValidationError(message.to_string())
        }
        other => Error::ValidationError(other.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::approval::business_adapter::{
        document_type_of_sales_business, ensure_runtime_cut_over, subject_ref_for,
        APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER,
    };
    use crate::approval::policy::{policy_of, ALL_DOCUMENT_TYPES};
    use crate::document_registry::new_registered_document;
    use bpm::model::types::{ApprovalDefinitionStatus, ApprovalTransitionEvent};
    use entities::sales_order::BusinessType;

    /// 绑定政策：无审批跳过，必须审批要求发布定义。
    #[test]
    fn binding_policy_skips_no_approval_and_requires_published() {
        for document_type in ALL_DOCUMENT_TYPES {
            let policy = policy_of(document_type).expect("政策必须存在");
            match policy.requirement() {
                ApprovalRequirement::NoApproval => {
                    assert_eq!(
                        binding_decision(policy.requirement()),
                        BindingDecision::SkipNoApproval
                    );
                }
                ApprovalRequirement::ProcessRequired => {
                    assert_eq!(
                        binding_decision(policy.requirement()),
                        BindingDecision::RequirePublished
                    );
                }
            }
        }
    }

    /// 缺失发布定义失败关闭。
    #[test]
    fn missing_published_definition_fails_closed() {
        let error = published_definition_or_not_configured::<()>(None).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!("数据冲突: {APPROVAL_PROCESS_NOT_CONFIGURED}")
        );
        assert!(published_definition_or_not_configured(Some(1)).is_ok());
    }

    /// 绑定必须整体写入，禁止半绑定。
    #[test]
    fn attach_binding_is_atomic() {
        let mut document =
            new_registered_document("doc-1", DocumentType::StockAdjustment, "").expect("草稿可空编号");
        assert!(document.approval_binding.is_none());
        let binding = binding_from_published(
            ApprovalProcessDefinitionId::new("def-1"),
            1,
            Instant::from_unix_secs(10),
        )
        .unwrap();
        attach_published_binding(&mut document, binding).unwrap();
        assert_eq!(
            document
                .approval_binding
                .as_ref()
                .unwrap()
                .approval_binding_version,
            1
        );
        assert!(attach_published_binding(
            &mut document,
            binding_from_published(
                ApprovalProcessDefinitionId::new("def-2"),
                2,
                Instant::from_unix_secs(11),
            )
            .unwrap()
        )
        .is_err());
    }

    /// 升级闸门：管理员、未提交、未启动、双 CAS。
    #[test]
    fn upgrade_unsubmitted_requires_admin_and_dual_cas() {
        assert!(ensure_upgrade_unsubmitted_allowed(false, false, 1, 1, 1, 1, true).is_ok());
        assert!(ensure_upgrade_unsubmitted_allowed(false, false, 1, 1, 1, 1, false).is_err());
        assert!(ensure_upgrade_unsubmitted_allowed(true, false, 1, 1, 1, 1, true).is_err());
        assert!(ensure_upgrade_unsubmitted_allowed(false, true, 1, 1, 1, 1, true).is_err());
        assert!(ensure_upgrade_unsubmitted_allowed(false, false, 2, 1, 1, 1, true).is_err());
        assert!(ensure_upgrade_unsubmitted_allowed(false, false, 1, 1, 2, 1, true).is_err());
    }

    /// 未 cut-over 类型不得进入目标运行时。
    #[test]
    fn process_required_except_pilot_is_not_cut_over() {
        assert!(ensure_runtime_cut_over(DocumentType::StockAdjustment).is_ok());
        let error = ensure_runtime_cut_over(DocumentType::PurchaseOrder).unwrap_err();
        assert!(error.to_string().contains(APPROVAL_DOCUMENT_TYPE_NOT_CUT_OVER));
    }

    /// 20 个类型均可构造唯一 SubjectRef；销售按 BusinessType 分派。
    #[test]
    fn subject_refs_are_unique_and_sales_dispatch_is_exhaustive() {
        let sales = subject_ref_for(DocumentType::SalesOrder, "so-1").unwrap();
        let voucher = subject_ref_for(DocumentType::VoucherSalesOrder, "so-1").unwrap();
        assert_ne!(sales.subject_kind(), voucher.subject_kind());
        assert_eq!(
            document_type_of_sales_business(BusinessType::GoodsService),
            DocumentType::SalesOrder
        );
        assert_eq!(
            document_type_of_sales_business(BusinessType::Voucher),
            DocumentType::VoucherSalesOrder
        );
        for document_type in ALL_DOCUMENT_TYPES {
            let subject = subject_ref_for(document_type, "id-1").unwrap();
            assert_eq!(subject.subject_kind(), process_kind_of(document_type).as_str());
        }
    }

    /// 发布图节点数量非法时失败关闭。
    #[test]
    fn published_graph_node_count_fails_closed() {
        assert!(ensure_node_count(0).is_err());
        assert!(ensure_node_count(1).is_ok());
        assert!(ensure_node_count(21).is_err());
        assert!(ensure_published_status(ApprovalDefinitionStatus::Published).is_ok());
        assert!(ensure_published_status(ApprovalDefinitionStatus::Draft).is_err());
        assert!(ensure_published_status(ApprovalDefinitionStatus::Retired).is_err());
    }

    /// 连线与生成器不一致时失败关闭。
    #[test]
    fn transition_mismatch_fails_closed() {
        let expected = generate_linear_transitions(&["n1".into(), "n2".into()]).unwrap();
        assert!(ensure_transitions_match(&[], &expected).is_err());
        assert_eq!(expected[0].event, ApprovalTransitionEvent::Approve);
    }

    /// 20 个 DocumentType 的 BusinessDocument 注册清点。
    #[test]
    fn business_document_registration_inventory() {
        const ROWS: &[(DocumentType, &str, &str)] = &[
            (
                DocumentType::SalesOrder,
                "已注册",
                "backend/services/src/sales_order/command.rs:170",
            ),
            (
                DocumentType::VoucherSalesOrder,
                "已注册(共用入口，类型分派属销售单子阶段)",
                "backend/services/src/sales_order/command.rs:170",
            ),
            (
                DocumentType::SalesChangeOrder,
                "待子阶段补齐",
                "backend/services/src/sales_review/sales_change_order.rs:139",
            ),
            (
                DocumentType::PurchaseOrder,
                "本阶段新增",
                "backend/services/src/purchase_order/draft_from_confirmation.rs",
            ),
            (
                DocumentType::PurchaseChangeOrder,
                "本阶段新增",
                "backend/services/src/purchase_order/change.rs:38",
            ),
            (
                DocumentType::StockAdjustment,
                "本阶段新增",
                "backend/services/src/inventory/mod.rs:492",
            ),
            (
                DocumentType::CustomerReceipt,
                "待子阶段补齐",
                "backend/services/src/receivable/mod.rs:735",
            ),
            (
                DocumentType::SupplierPayment,
                "本阶段新增",
                "backend/services/src/payable/mod.rs:288",
            ),
            (
                DocumentType::CustomerRefund,
                "本阶段新增",
                "backend/services/src/returns/customer_refund.rs:113",
            ),
            (
                DocumentType::SupplierRefund,
                "本阶段新增",
                "backend/services/src/returns/supplier_refund.rs:50",
            ),
            (
                DocumentType::ReceiptReversal,
                "本阶段新增",
                "backend/services/src/returns/receipt_reversal.rs:33",
            ),
            (
                DocumentType::PaymentReversal,
                "本阶段新增",
                "backend/services/src/returns/payment_reversal.rs:29",
            ),
            (
                DocumentType::PurchaseReceipt,
                "本阶段新增",
                "backend/services/src/fulfillment/purchase_receipt.rs:122",
            ),
            (
                DocumentType::Delivery,
                "本阶段新增",
                "backend/services/src/fulfillment/delivery.rs:131",
            ),
            (
                DocumentType::ElectronicDelivery,
                "本阶段新增",
                "backend/services/src/fulfillment/electronic_delivery.rs:102",
            ),
            (
                DocumentType::ServiceFulfillment,
                "本阶段新增",
                "backend/services/src/fulfillment/service_fulfillment.rs:101",
            ),
            (
                DocumentType::CustomerAcceptance,
                "本阶段新增",
                "backend/services/src/fulfillment/customer_acceptance.rs:132",
            ),
            (
                DocumentType::Invoice,
                "待子阶段补齐",
                "backend/services/src/receivable/mod.rs:995",
            ),
            (
                DocumentType::SalesReturnCase,
                "本阶段新增",
                "backend/services/src/returns/sales_return.rs:88",
            ),
            (
                DocumentType::PurchaseReturnOrder,
                "本阶段新增",
                "backend/services/src/returns/purchase_return.rs:90",
            ),
        ];
        assert_eq!(ROWS.len(), 20);
        let mut seen = std::collections::HashSet::new();
        for (document_type, status, entry) in ROWS {
            assert!(!status.is_empty());
            assert!(entry.contains("backend/services/src/"));
            assert!(seen.insert(*document_type));
            let _ = new_registered_document("id-1", *document_type, "").expect("空编号草稿可注册");
        }
    }

    /// 草稿允许空编号；正式号原样登记。
    #[test]
    fn draft_allows_empty_document_no() {
        let empty = new_registered_document("po-1", DocumentType::PurchaseOrder, "   ").unwrap();
        assert!(empty.document_no.is_empty());
        let numbered = new_registered_document("adj-1", DocumentType::StockAdjustment, " ADJ-1 ").unwrap();
        assert_eq!(numbered.document_no, "ADJ-1");
        assert_eq!(numbered.base.id, "adj-1");
    }
}
