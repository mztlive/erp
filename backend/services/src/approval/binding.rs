//! 单据与当前已发布定义的统一绑定端口。
//!
//! 端口接收调用方 `Executor`，不得自行开嵌套事务，也不得把执行器传入 BPM。

use bpm::ids::ApprovalProcessDefinitionId;
use bpm::model::types::ModelError;
use database::repository::bpm::DefinitionGraph;
use database::{AccessControlExt, BpmExt, DocumentRegistryExt, Executor, MongoCasbinAdapter};
use entities::common::time::Instant;
use entities::document_registry::business_document::{
    ApprovalBindingUpgradeError, ApprovalBindingUpgradeInput, ApprovalDefinitionBinding,
};
use entities::document_registry::{BusinessDocument, DocumentType};
use entities::{AccountCore, RoleIdSet};
use mongodb::Database;

use crate::audit::AuditActor;
use crate::errors::{Error, ErrorCode, Result};
use crate::iam::{subject, SharedRbacService};

use super::business_adapter::{
    adapter_spec_of, ensure_published_status, ensure_separation_of_duties,
    revalidate_assignee_binding_access, subject_ref_for, BindingRevalidationContext,
};
use super::policy::{
    policy_of, require_process_required, ApprovalRequirement, ApproverEligibilityPolicy,
    ProcessRequiredApprovalPolicy, STATIC_APPROVE_PERMISSION,
};
use super::process_kind::process_kind_of;

/// 绑定审计动作。
pub const DEFINITION_BOUND_AUDIT_ACTION: &str = "approval.definition.bound";
/// 无审批政策事实审计动作。
pub const DEFINITION_POLICY_AUDIT_ACTION: &str = "approval.definition.policy";
/// 未提交升级审计动作。
pub const DEFINITION_UPGRADED_AUDIT_ACTION: &str = "approval.definition.upgraded";

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
    Error::from_approval_code(ErrorCode::ApprovalProcessNotConfigured)
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
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `command` - 单据、双 CAS、原因与重验上下文
/// * `actor` - 已认证运行管理员
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回升级后完整审批绑定。
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
    let approval_started =
        document_has_started_instance(db, document.document_type, &document.base.id, executor).await?;
    document
        .ensure_unsubmitted_approval_binding_upgrade(
            approval_started,
            command.expected_document_version,
            command.expected_binding_version,
            &command.reason,
        )
        .map_err(map_binding_upgrade_error)?;
    let published = load_published_graph(db, document.document_type, executor).await?;
    revalidate_binding_graph(db, rbac, &policy, &command.context, &published, executor).await?;
    document
        .upgrade_unsubmitted_approval_binding(ApprovalBindingUpgradeInput {
            approval_process_definition_id: ApprovalProcessDefinitionId::new(
                published.definition.base.id.clone(),
            ),
            approval_definition_version: published.definition.definition_version,
            approval_started,
            expected_document_version: command.expected_document_version,
            expected_binding_version: command.expected_binding_version,
            reason: &command.reason,
            at: Instant::now(),
        })
        .map_err(map_binding_upgrade_error)?;
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

/// 复用 BPM 图原语重验发布结构；不得把 Executor 传入 BPM。
///
/// # 参数
/// * `graph` - Repository 一次性加载的发布定义图
///
/// # 返回
/// 状态为已发布且线性图完整时返回 `Ok(())`。
///
/// # 错误
/// 状态或 BPM 图结构损坏时返回稳定校验错误。
///
/// # 关键业务约束
/// Service 不得复制节点顺序、入口或连线算法。
fn revalidate_published_graph(graph: &DefinitionGraph) -> Result<()> {
    ensure_published_status(graph.definition.status)?;
    graph.validate_linear().map_err(map_model_error)
}

/// Adapter 重验指定用户、权限、DataScope、读取权与岗位分离。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `policy` - 当前单据类型必须审批政策
/// * `context` - 当前单据组织与创建人事实
/// * `graph` - 已由 BPM 校验的定义图
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 全部定义审批人通过静态与对象访问重验时返回 `Ok(())`。
///
/// # 错误
/// 岗位分离、账号、权限、数据范围或对象读取权失败时返回错误。
///
/// # 关键业务约束
/// 审批人集合由 BPM 图确定性提取，Service 只编排外部资格判断。
async fn revalidate_binding_graph(
    db: &Database,
    rbac: &SharedRbacService,
    policy: &ProcessRequiredApprovalPolicy,
    context: &BindingRevalidationContext,
    graph: &DefinitionGraph,
    executor: &mut dyn Executor,
) -> Result<()> {
    let spec = adapter_spec_of(policy.document_type)?;
    let assignee_ids = graph.assignee_ids();
    ensure_separation_of_duties(
        policy.separation_of_duties_policy,
        &context.creator_id,
        &assignee_ids,
    )?;
    for user_id in &assignee_ids {
        let account = revalidate_one_assignee(db, rbac, user_id, executor).await?;
        let (user_scopes, role_scopes) = load_assignee_scope_sets(db, &account, executor).await?;
        revalidate_assignee_binding_access(&spec, &user_scopes, &role_scopes, context, user_id)?;
    }
    Ok(())
}

/// 重验单个指定审批人的账号与静态权限。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `user_id` - 定义指定的审批人账号 ID
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回存在且当前可承担后台审批责任的账号。
///
/// # 错误
/// 账号缺失、已停用、身份不符或缺少静态审批权限时返回错误。
///
/// # 关键业务约束
/// Repository 只读取账号事实，后台有效性由实体判断，静态权限由 Service 查询 RBAC。
async fn revalidate_one_assignee(
    db: &Database,
    rbac: &SharedRbacService,
    user_id: &str,
    executor: &mut dyn Executor,
) -> Result<AccountCore> {
    let account = db
        .accounts()
        .find_approval_assignee_by_id(user_id, executor)
        .await?
        .ok_or_else(|| Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string()))?;
    ensure_assignee_ready(&account)?;
    ensure_static_decide_permission(rbac, &account).await?;
    Ok(account)
}

/// 同时加载用户与启用角色的 DataScope，强度与 Resolver 一致。
async fn load_assignee_scope_sets(
    db: &Database,
    account: &AccountCore,
    executor: &mut dyn Executor,
) -> Result<(
    Vec<entities::access_control::DataScope>,
    Vec<entities::access_control::DataScope>,
)> {
    let user_scopes = db
        .data_scopes()
        .list_by_subject(
            entities::access_control::DataScopeSubjectType::User,
            &account.base.id,
            executor,
        )
        .await?;
    let role_scopes = load_enabled_role_scopes(db, account, executor).await?;
    Ok((user_scopes, role_scopes))
}

/// 读取审批人当前启用角色的全部组织范围。
async fn load_enabled_role_scopes(
    db: &Database,
    account: &AccountCore,
    executor: &mut dyn Executor,
) -> Result<Vec<entities::access_control::DataScope>> {
    let mut role_scopes = Vec::new();
    for role_id in load_enabled_role_ids(db, account, executor).await? {
        role_scopes.extend(
            db.data_scopes()
                .list_by_subject(
                    entities::access_control::DataScopeSubjectType::Role,
                    &role_id,
                    executor,
                )
                .await?,
        );
    }
    Ok(role_scopes)
}

/// 读取 Casbin 绑定且仍然启用的角色 ID。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `account` - 已重验的审批人账号
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回按 Casbin 角色键确定化并经角色仓储过滤后的启用角色 ID。
///
/// # 错误
/// Casbin 查询、角色键解析或角色仓储查询失败时返回错误。
///
/// # 关键业务约束
/// 角色键过滤、排序与去重由 `RoleIdSet` 统一实现，Service 不保留第二套解析规则。
async fn load_enabled_role_ids(
    db: &Database,
    account: &AccountCore,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    let role_ids = RoleIdSet::from_casbin_role_keys(
        MongoCasbinAdapter::new(db.clone())
            .subject_roles(&subject(account.kind, &account.base.id), executor)
            .await?,
    )?
    .to_strings();
    if role_ids.is_empty() {
        return Ok(role_ids);
    }
    Ok(db
        .roles()
        .enabled_roles(&role_ids, executor)
        .await?
        .into_iter()
        .map(|role| role.base.id)
        .collect())
}

/// 将账号实体的后台有效性判断映射为绑定校验错误。
///
/// # 参数
/// * `account` - Repository 返回的审批人账号
///
/// # 返回
/// 账号可承担后台责任时返回 `Ok(())`。
///
/// # 错误
/// 账号已停用或身份不满足后台责任时返回校验错误。
///
/// # 关键业务约束
/// 类型与状态组合规则只由 `AccountCore::is_active_backoffice` 提供。
fn ensure_assignee_ready(account: &AccountCore) -> Result<()> {
    match ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission {
        ApproverEligibilityPolicy::ActiveBackofficeWithDecidePermission => {}
    }
    if account.is_active_backoffice() {
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

/// 查询单据是否已经启动过审批实例。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `document_type` - ERP 单据类型
/// * `document_id` - 业务单据注册 ID
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 任意未删除审批实例命中该主体时返回 `true`。
///
/// # 错误
/// 主体引用构造或 Repository 查询失败时返回错误。
///
/// # 关键业务约束
/// 查询条件由 BPM Repository 封装，终态实例仍证明单据已经启动过审批。
async fn document_has_started_instance(
    db: &Database,
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    let subject = subject_ref_for(document_type, document_id)?;
    db.bpm_workflow()
        .has_started_instance_for_subject(&subject, executor)
        .await
        .map_err(Into::into)
}

/// 将 ERP 单据绑定升级错误映射为稳定的 Service 错误语义。
///
/// # 参数
/// * `error` - 实体层返回的升级失败原因
///
/// # 返回
/// 返回保持验证错误、冲突错误与实体不变量分类的 Service 错误。
///
/// # 错误
/// 无；本方法只执行错误分类转换。
///
/// # 关键业务约束
/// 已提交、已启动与双 CAS 失败必须保持冲突语义，缺失绑定和空原因保持校验语义。
fn map_binding_upgrade_error(error: ApprovalBindingUpgradeError) -> Error {
    match error {
        ApprovalBindingUpgradeError::MissingBinding => Error::ValidationError("尚未绑定审批定义".to_string()),
        ApprovalBindingUpgradeError::Formalized => {
            Error::ConflictError("已提交单据不能升级审批绑定".to_string())
        }
        ApprovalBindingUpgradeError::ApprovalStarted => {
            Error::ConflictError("已启动单据不能升级审批绑定".to_string())
        }
        ApprovalBindingUpgradeError::VersionConflict => {
            Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string())
        }
        ApprovalBindingUpgradeError::EmptyReason => Error::ValidationError("升级原因不能为空".to_string()),
        ApprovalBindingUpgradeError::BindingInvariant(error) => Error::Logic(error),
    }
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
    };
    use crate::approval::policy::{policy_of, ALL_DOCUMENT_TYPES};
    use crate::document_registry::new_registered_document;
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
            ErrorCode::ApprovalProcessNotConfigured.as_str()
        );
        assert!(published_definition_or_not_configured(Some(1)).is_ok());
    }

    /// 全部必须审批类型进入目标运行时。
    #[test]
    fn process_required_types_are_cut_over() {
        assert!(ensure_runtime_cut_over(DocumentType::StockAdjustment).is_ok());
        assert!(ensure_runtime_cut_over(DocumentType::PurchaseOrder).is_ok());
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

    /// bind/upgrade 生产路径必须走用户+角色范围与 Adapter 读取权闸门。
    #[test]
    fn bind_and_upgrade_use_shared_scope_and_read_gate() {
        let production = include_str!("binding.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");
        assert!(production.contains("revalidate_assignee_binding_access"));
        assert!(production.contains("load_assignee_scope_sets"));
        assert!(production.contains("DataScopeSubjectType::Role"));
        assert!(!production.contains("ensure_binding_scope(&spec, &scopes, &context.organization_id, true)"));
        let spec = crate::approval::business_adapter::adapter_spec_of(DocumentType::StockAdjustment)
            .expect("试点必须有适配器");
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };
        let error = crate::approval::business_adapter::revalidate_assignee_binding_access(
            &spec,
            &[],
            &[],
            &context,
            "u1",
        )
        .unwrap_err();
        assert!(error.to_string().contains("数据范围不覆盖当前单据组织"));
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
