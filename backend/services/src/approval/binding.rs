//! 单据与当前已发布定义的统一绑定端口。
//!
//! 端口接收调用方 `Executor`，不得自行开嵌套事务，也不得把执行器传入 BPM。

use std::collections::HashMap;

use bpm::ids::{ApprovalCommandReceiptId, ApprovalProcessDefinitionId};
use bpm::model::types::ModelError;
use bpm::model::{ApprovalCommandReceipt, Timestamp};
use database::repository::bpm::DefinitionGraph;
use database::{AccessControlExt, BpmExt, DocumentRegistryExt, Executor, MongoCasbinAdapter};
use entities::common::time::Instant;
use entities::document_registry::business_document::{
    ApprovalBindingUpgradeError, ApprovalBindingUpgradeInput, ApprovalDefinitionBinding,
};
use entities::document_registry::workflow_action::ApprovalBindingActionContext;
use entities::document_registry::{
    BusinessDocument, BusinessDocumentId, DocumentType, WorkflowAction, WorkflowActionData, WorkflowActionId,
    WorkflowActionType,
};
use entities::{AccountCore, RoleIdSet};
use mongodb::Database;
use serde::{Deserialize, Serialize};

use crate::audit::AuditActor;
use crate::errors::{Error, ErrorCode, Result};
use crate::iam::{subject, SharedRbacService};

use super::business_adapter::{
    adapter_spec_of, assignment_scope_covers_organization, ensure_separation_of_duties,
    revalidate_assignee_binding_access, BindingRevalidationContext,
};
use super::execution::idempotency::{payload_conflict_error, ReceiptBranch};
use super::execution::{map_receipt_first_write_error, upgrade_binding_identity, PreparedCommandIdentity};
use super::policy::{
    policy_of, require_process_required, ApprovalRequirement, ApproverEligibilityPolicy,
    ProcessRequiredApprovalPolicy, STATIC_APPROVE_PERMISSION,
};
use super::process_kind::process_kind_of;
use super::scope::{
    approval_actor_is_active_with_executor, approval_binding_upgrade_authorization_with_executor,
    approval_document_read_scope_with_executor,
};
use super::upgrade_subject::{
    ensure_initial_unsubmitted_approval_upgrade_subject, load_approval_upgrade_subject_facts,
    ApprovalUpgradeSubjectFacts,
};

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
    /// 路由与强实体必须共同证明的精确单据类型。
    pub document_type: DocumentType,
    /// 强业务对象与注册行共用的精确 ID。
    pub document_id: String,
    /// 客户端签署的强业务对象版本。
    pub expected_business_object_version: u64,
    /// 期望的绑定 CAS 版本。
    pub expected_binding_version: u64,
    /// 升级原因；命令身份与不可变动作均使用其 trim 后值。
    pub reason: String,
    /// 运行层在任何仓储访问前预构造的精确 V3 命令身份。
    pub identity: PreparedCommandIdentity,
    /// Fresh 分支预生成的不可变动作 ID。
    pub action_id: WorkflowActionId,
    /// Fresh 分支预生成的命令收据 ID。
    pub receipt_id: ApprovalCommandReceiptId,
}

/// 绑定升级结果类别。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpgradeBindingOutcome {
    /// 本次事务应用了新绑定。
    Applied,
    /// 同载荷收据经当前授权后回读原动作。
    Replay,
}

/// 绑定升级返回的新定义绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeBindingView {
    /// 审批定义 ID。
    pub approval_process_definition_id: String,
    /// 定义业务版本。
    pub approval_definition_version: u32,
    /// 绑定 CAS 版本；字符串形态避免 JavaScript 精度丢失。
    pub approval_binding_version: String,
    /// 绑定发生时间。
    pub approval_definition_bound_at: Instant,
}

/// 从不可变 `WorkflowAction` 投影的绑定升级结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeBindingResultView {
    /// 精确单据类型。
    pub document_type: DocumentType,
    /// 精确强业务对象 ID。
    pub document_id: String,
    /// 原命令签署的强业务对象版本。
    pub original_business_object_version: String,
    /// 升级后绑定；不从当前可变注册行伪造。
    pub new_binding: UpgradeBindingView,
    /// 收据 `result_ref` 指向的不可变动作 ID。
    pub action_id: String,
    /// 本次是应用还是授权回读。
    pub outcome: UpgradeBindingOutcome,
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
/// 目标固定为当前唯一 `PUBLISHED` 定义，禁止客户端提交定义 ID。
/// 本端口不开事务；运行层必须传入同一外层事务执行器。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `command` - 精确对象、强版本、绑定 CAS、原因和预构造幂等身份
/// * `actor` - 已认证操作人；仍需在事务内重验账号与授权
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// Fresh 返回从新动作投影的 `Applied`；同载荷收据返回从原动作
/// 严格重建的 `Replay`。
///
/// # 错误
/// 身份、授权、收据、动作、强实体、注册行、定义图或人员重验失败时
/// 返回错误。任何收据冲突之外的 duplicate 不得进入恢复。
pub async fn upgrade_unsubmitted_document_definition(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<UpgradeBindingResultView> {
    let reason = normalized_upgrade_reason(command)?;
    ensure_prepared_upgrade_identity(command, actor, reason)?;

    let authorized = load_authorized_upgrade_context(db, rbac, command, actor, executor).await?;

    let receipt = find_upgrade_receipt(db, &command.identity, executor).await?;
    match command.identity.classify(receipt.as_ref()) {
        ReceiptBranch::SamePayload(receipt) => {
            return replay_upgrade_result(db, command, actor, reason, receipt, executor).await;
        }
        ReceiptBranch::PayloadConflict => return Err(payload_conflict_error()),
        ReceiptBranch::Fresh => {}
    }

    authorized
        .facts
        .ensure_expected_business_object_version(command.expected_business_object_version)?;
    ensure_initial_unsubmitted_approval_upgrade_subject(db, &authorized.facts, executor).await?;
    apply_fresh_upgrade(
        db,
        rbac,
        command,
        actor,
        reason,
        &authorized.facts,
        &authorized.policy,
        &authorized.actor_role,
        executor,
    )
    .await
}

/// 在 unknown/duplicate 恢复的新事务中只执行授权回读。
///
/// # 返回
/// 同载荷 V3 收据及其不可变动作完整时返回 `Some(Replay)`；收据尚不存在
/// 返回 `None`。
///
/// # 错误
/// 强业务身份、当前账号、三重授权、policy revision、收据载荷或动作证明
/// 任一失败时返回错误。
///
/// # 关键业务约束
/// 本端口绝不调用 Fresh 门禁与任何写入；运行层必须为每次恢复尝试传入可用的
/// 新事务执行器。
pub async fn replay_unsubmitted_document_definition_upgrade(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<UpgradeBindingResultView>> {
    let reason = normalized_upgrade_reason(command)?;
    ensure_prepared_upgrade_identity(command, actor, reason)?;
    let _authorized = load_authorized_upgrade_context(db, rbac, command, actor, executor).await?;
    let receipt = find_upgrade_receipt(db, &command.identity, executor).await?;
    match command.identity.classify(receipt.as_ref()) {
        ReceiptBranch::SamePayload(receipt) => {
            replay_upgrade_result(db, command, actor, reason, receipt, executor)
                .await
                .map(Some)
        }
        ReceiptBranch::PayloadConflict => Err(payload_conflict_error()),
        ReceiptBranch::Fresh => Ok(None),
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
    let graph = published_definition_or_not_configured(
        db.bpm_workflow()
            .load_published_definition_graph(process_kind_of(document_type), executor)
            .await?,
    )?;
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
/// 状态不是已发布时映射为未配置；图结构损坏时返回稳定校验错误。
///
/// # 关键业务约束
/// Service 不得复制节点顺序、入口或连线算法；仓储过滤不能替代 BPM 确认。
fn revalidate_published_graph(graph: &DefinitionGraph) -> Result<()> {
    graph.validate_published_linear().map_err(|error| match error {
        ModelError::InvalidStatus(_) => process_not_configured(),
        other => map_model_error(other),
    })
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
    let accounts = load_assignee_accounts(db, &assignee_ids, executor).await?;
    for user_id in &assignee_ids {
        let account = require_ready_assignee(accounts.get(user_id))?;
        if policy.document_type == DocumentType::StockAdjustment {
            revalidate_stock_adjustment_binding_access(db, rbac, context, account, executor).await?;
            continue;
        }
        ensure_static_decide_permission(rbac, account).await?;
        let (user_scopes, role_scope_sets) = load_assignee_scope_sets(db, rbac, account, executor).await?;
        revalidate_assignee_binding_access_by_role(&spec, &user_scopes, &role_scope_sets, context, user_id)?;
    }
    Ok(())
}

/// 一次批量读取定义内全部审批人账号事实。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `assignee_ids` - BPM 按确定顺序提取的审批人 ID
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回按账号 ID 索引的未软删除账号事实；缺失 ID 不会补齐。
///
/// # 错误
/// Repository 批量查询失败时返回错误。
///
/// # 关键业务约束
/// Repository 不保证 `$in` 结果顺序；Service 必须继续按 `assignee_ids`
/// 逐用户查表与重验 RBAC，保留首错与精确错误语义。
async fn load_assignee_accounts(
    db: &Database,
    assignee_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, AccountCore>> {
    Ok(db
        .accounts()
        .list_by_ids(assignee_ids, executor)
        .await?
        .into_iter()
        .map(|account| (account.base.id.clone(), account))
        .collect())
}

/// 同时加载用户与实际授予审批权限的启用角色 DataScope。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `account` - 已通过账号级权限重验的审批人
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回用户范围事实，以及按授权角色隔离的角色范围事实。
///
/// # 错误
/// 用户范围、角色、RBAC 或角色范围事实读取失败时返回错误。
///
/// # 关键业务约束
/// 角色范围不得脱离实际授予 `approval_instance:decide` 的角色单独生效。
async fn load_assignee_scope_sets(
    db: &Database,
    rbac: &SharedRbacService,
    account: &AccountCore,
    executor: &mut dyn Executor,
) -> Result<(Vec<entities::access_control::DataScope>, Vec<RoleScopeFacts>)> {
    let user_scopes = db
        .data_scopes()
        .list_by_subject(
            entities::access_control::DataScopeSubjectType::User,
            &account.base.id,
            executor,
        )
        .await?;
    let role_scope_sets = load_enabled_decide_role_scopes(db, rbac, account, executor).await?;
    Ok((user_scopes, role_scope_sets))
}

/// 单个实际授权角色的范围事实。
#[derive(Debug)]
struct RoleScopeFacts(Vec<entities::access_control::DataScope>);

/// 批量读取审批人当前启用且实际授予审批权限的角色范围事实。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `account` - 已通过后台有效性重验的审批人账号
/// * `executor` - 调用方事务执行器
///
/// # 返回
/// 返回按实际授权角色隔离的未软删除 DataScope 事实。
///
/// # 错误
/// Casbin 角色读取、角色过滤、角色权限判定或 DataScope 批量查询失败时返回错误；
/// 没有启用且实际授予审批权限的角色时返回固定权限错误。
///
/// # 关键业务约束
/// Repository 一次批量返回事实；Service 只允许实际授予权限的角色进入范围交集。
async fn load_enabled_decide_role_scopes(
    db: &Database,
    rbac: &SharedRbacService,
    account: &AccountCore,
    executor: &mut dyn Executor,
) -> Result<Vec<RoleScopeFacts>> {
    let role_ids = load_enabled_role_ids(db, account, executor).await?;
    let permission = static_decide_permission()?;
    let granting_role_ids =
        require_granting_role_ids(permission_granting_role_ids(rbac, role_ids, &permission).await?)?;
    let role_scopes = db
        .data_scopes()
        .list_by_subjects(
            entities::access_control::DataScopeSubjectType::Role,
            &granting_role_ids,
            executor,
        )
        .await?;
    Ok(group_role_scope_facts(&granting_role_ids, role_scopes))
}

/// 筛出实际授予静态审批权限的启用角色。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `role_ids` - 已由 Repository 证明仍启用的角色 ID
/// * `permission` - 静态审批权限
///
/// # 返回
/// 返回按启用角色顺序保留的实际授权角色 ID。
///
/// # 错误
/// 任一角色的 RBAC 判定失败时返回错误。
///
/// # 关键业务约束
/// 不得用账号主体整体授权结果替代角色级来源判断。
async fn permission_granting_role_ids(
    rbac: &SharedRbacService,
    role_ids: Vec<String>,
    permission: &entities::Permission,
) -> Result<Vec<String>> {
    let mut granting = Vec::new();
    for role_id in role_ids {
        if rbac.enforce(&format!("role:{role_id}"), permission).await? {
            granting.push(role_id);
        }
    }
    Ok(granting)
}

/// 要求至少一个启用角色实际授予静态审批权限。
///
/// # 参数
/// * `role_ids` - 经启用过滤和角色级 RBAC 判定后的角色 ID
///
/// # 返回
/// 非空时原样返回角色 ID，保留确定顺序。
///
/// # 错误
/// 空集合按绑定合同返回“指定审批人缺少审批权限”。
///
/// # 关键业务约束
/// 账号级权限可能来自已停用角色的残留 Casbin 事实，不得据此放行范围。
fn require_granting_role_ids(role_ids: Vec<String>) -> Result<Vec<String>> {
    require_static_decide_permission(!role_ids.is_empty())?;
    Ok(role_ids)
}

/// 将批量角色范围事实恢复为逐授权角色隔离的集合。
///
/// # 参数
/// * `role_ids` - 实际授予审批权限的启用角色 ID
/// * `scopes` - Repository 批量返回的角色范围事实
///
/// # 返回
/// 返回与 `role_ids` 同序的范围集合；缺失角色事实保留为空集合。
///
/// # 错误
/// 无；未知主体事实不会进入授权结果。
///
/// # 关键业务约束
/// 每个角色的权限与范围必须保持在同一集合内，不得跨角色拼接。
fn group_role_scope_facts(
    role_ids: &[String],
    scopes: Vec<entities::access_control::DataScope>,
) -> Vec<RoleScopeFacts> {
    let mut scopes_by_role = HashMap::<String, Vec<_>>::new();
    for scope in scopes {
        scopes_by_role
            .entry(scope.subject_id.clone())
            .or_default()
            .push(scope);
    }
    role_ids
        .iter()
        .map(|role_id| RoleScopeFacts(scopes_by_role.remove(role_id).unwrap_or_default()))
        .collect()
}

/// 按授权角色逐一重验审批人绑定范围与对象读取权。
///
/// # 参数
/// * `spec` - 当前单据审批适配器规格
/// * `user_scopes` - 当前审批人的用户范围事实
/// * `role_scope_sets` - 按实际授权角色隔离的范围事实
/// * `context` - 当前单据组织与创建人上下文
/// * `assignee_user_id` - 当前审批人账号 ID
///
/// # 返回
/// 至少一个授权角色与用户范围共同覆盖单据组织且对象可读时返回 `Ok(())`。
///
/// # 错误
/// 没有同一授权角色覆盖组织，或对象读取权失败时返回原绑定合同错误。
///
/// # 关键业务约束
/// 权限来自角色 A、范围来自角色 B 时必须失败关闭。
fn revalidate_assignee_binding_access_by_role(
    spec: &super::business_adapter::ApprovalAdapterSpec,
    user_scopes: &[entities::access_control::DataScope],
    role_scope_sets: &[RoleScopeFacts],
    context: &BindingRevalidationContext,
    assignee_user_id: &str,
) -> Result<()> {
    let role_scopes = role_scope_sets
        .iter()
        .map(|facts| facts.0.as_slice())
        .find(|role_scopes| {
            assignment_scope_covers_organization(user_scopes, role_scopes, &context.organization_id)
        })
        .unwrap_or(&[]);
    revalidate_assignee_binding_access(spec, user_scopes, role_scopes, context, assignee_user_id)
}

/// 库存调整绑定在同一 executor 内分别证明决定与对象读取范围。
async fn revalidate_stock_adjustment_binding_access(
    db: &Database,
    rbac: &SharedRbacService,
    context: &BindingRevalidationContext,
    account: &AccountCore,
    executor: &mut dyn Executor,
) -> Result<()> {
    let assignee = AuditActor::new(account.base.id.clone(), account.base.id.clone(), account.kind);
    let decide_scope =
        crate::approval::approval_decide_scope_with_executor(db, rbac, &assignee, executor).await?;
    let read_scope = approval_document_read_scope_with_executor(
        db,
        rbac,
        &assignee,
        DocumentType::StockAdjustment,
        executor,
    )
    .await?;
    if !decide_scope.covers(&context.organization_id) {
        return Err(Error::ValidationError(
            "指定审批人缺少审批权限或数据范围不覆盖当前单据组织".to_string(),
        ));
    }
    if !read_scope.covers(&context.organization_id) {
        return Err(Error::ValidationError(
            "指定审批人不能读取当前库存调整单".to_string(),
        ));
    }
    Ok(())
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
    Err(assignee_unavailable_error())
}

/// 按当前审批人 ID 从批量快照映射中取回可承担责任的账号。
///
/// # 参数
/// * `account` - 批量映射中按当前审批人 ID 查得的可选账号
///
/// # 返回
/// 账号存在且当前可承担后台审批责任时返回其引用。
///
/// # 错误
/// 账号缺失、已停用或任职失效时返回合同固定的校验错误。
///
/// # 关键业务约束
/// 调用方必须按 BPM 审批人顺序逐个调用，不得依赖 `HashMap` 迭代顺序。
fn require_ready_assignee(account: Option<&AccountCore>) -> Result<&AccountCore> {
    let account = account.ok_or_else(assignee_unavailable_error)?;
    ensure_assignee_ready(account)?;
    Ok(account)
}

/// 构造审批人账号不可用的合同固定错误。
///
/// # 返回
/// 返回同时覆盖缺失、停用与任职失效的校验错误。
///
/// # 错误
/// 无；本方法只构造 Service 错误值。
fn assignee_unavailable_error() -> Error {
    Error::ValidationError("指定审批人账号不存在、已停用或任职失效".to_string())
}

/// 重验单个审批人的静态 `approval_instance:decide` 权限。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `account` - 已通过后台有效性重验的审批人账号
///
/// # 返回
/// 当前审批人拥有静态决定权限时返回 `Ok(())`。
///
/// # 错误
/// 权限常量损坏、RBAC 查询失败或当前用户缺少权限时返回错误。
///
/// # 关键业务约束
/// 调用方必须在 BPM 审批人顺序内逐用户调用，禁止用合并主体结果替代。
async fn ensure_static_decide_permission(rbac: &SharedRbacService, account: &AccountCore) -> Result<()> {
    let permission = static_decide_permission()?;
    let allowed = rbac
        .enforce(&subject(account.kind, &account.base.id), &permission)
        .await?;
    require_static_decide_permission(allowed)
}

/// 解析固定的静态审批权限不变量。
///
/// # 返回
/// 返回规范化的 `approval_instance:decide` 权限。
///
/// # 错误
/// 固定权限常量损坏时返回内部错误。
fn static_decide_permission() -> Result<entities::Permission> {
    entities::Permission::parse(STATIC_APPROVE_PERMISSION)
        .map_err(|error| Error::Internal(format!("静态审批权限不变量损坏: {error}")))
}

/// 将单用户 RBAC 判定收敛为绑定阶段的固定错误语义。
///
/// # 参数
/// * `allowed` - 当前审批人的 `approval_instance:decide` 判定结果
///
/// # 返回
/// 拥有静态审批权限时返回 `Ok(())`。
///
/// # 错误
/// 缺少权限时返回绑定合同固定的校验错误。
fn require_static_decide_permission(allowed: bool) -> Result<()> {
    if allowed {
        return Ok(());
    }
    Err(Error::ValidationError("指定审批人缺少审批权限".to_string()))
}

/// 得到命令签署与不可变动作共用的规范化原因。
fn normalized_upgrade_reason(command: &UpgradeUnsubmittedDefinitionCommand) -> Result<&str> {
    let reason = command.reason.trim();
    if reason.is_empty() {
        return Err(Error::ValidationError("升级原因不能为空".to_string()));
    }
    if command.action_id.as_ref().trim().is_empty() || command.receipt_id.as_ref().trim().is_empty() {
        return Err(Error::Internal("绑定升级预生成 ID 不完整".to_string()));
    }
    Ok(reason)
}

/// 同一事务快照内已重验的升级上下文。
struct AuthorizedUpgradeContext {
    facts: ApprovalUpgradeSubjectFacts,
    policy: ProcessRequiredApprovalPolicy,
    actor_role: String,
}

/// 先加载精确强事实，再重验账号与当前三重授权。
async fn load_authorized_upgrade_context(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<AuthorizedUpgradeContext> {
    let facts =
        load_approval_upgrade_subject_facts(db, command.document_type, &command.document_id, executor)
            .await?;
    ensure_active_upgrade_actor(db, actor, executor).await?;
    let policy = require_process_required(facts.document_type)?;
    let authorization = approval_binding_upgrade_authorization_with_executor(
        db,
        rbac,
        actor,
        facts.document_type,
        &policy.definition_admin_permission,
        &facts.responsible_org_id,
        executor,
    )
    .await?;
    Ok(AuthorizedUpgradeContext {
        facts,
        policy,
        actor_role: authorization.actor_role,
    })
}

/// 证明运行层传入的身份精确签署了本命令。
fn ensure_prepared_upgrade_identity(
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    reason: &str,
) -> Result<()> {
    let expected = upgrade_binding_identity(
        command.document_type.as_str(),
        &command.document_id,
        command.expected_business_object_version,
        command.expected_binding_version,
        reason,
        actor.id(),
        command.identity.idempotency_key().clone(),
    )?;
    let scopes = command.identity.scope_candidates();
    if command.identity.current() != expected.current()
        || scopes.len() != 1
        || scopes.first().copied() != Some(command.identity.current().scope().as_str())
    {
        return Err(payload_conflict_error());
    }
    Ok(())
}

/// 在强业务对象存在性已证明后，事务内重验操作人。
async fn ensure_active_upgrade_actor(
    db: &Database,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    if approval_actor_is_active_with_executor(db, actor, executor).await? {
        return Ok(());
    }
    Err(Error::Forbidden(
        "审批绑定升级账号不存在、已停用或身份已变化".to_string(),
    ))
}

/// 只按预构造 V3 身份查找绑定升级收据。
async fn find_upgrade_receipt(
    db: &Database,
    identity: &PreparedCommandIdentity,
    executor: &mut dyn Executor,
) -> Result<Option<ApprovalCommandReceipt>> {
    db.bpm_workflow()
        .find_command_receipt(
            identity.current().command_kind(),
            identity.current().scope().as_str(),
            identity.idempotency_key(),
            executor,
        )
        .await
        .map_err(Into::into)
}

/// 同载荷收据只能回读并严格证明其不可变动作。
async fn replay_upgrade_result(
    db: &Database,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    reason: &str,
    receipt: &ApprovalCommandReceipt,
    executor: &mut dyn Executor,
) -> Result<UpgradeBindingResultView> {
    let action = db
        .workflow_actions()
        .find_by_id(&receipt.result_ref, executor)
        .await?
        .ok_or_else(payload_conflict_error)?;
    upgrade_result_from_action(
        command,
        actor,
        reason,
        receipt,
        &action,
        UpgradeBindingOutcome::Replay,
    )
}

/// Fresh 分支完成全部读取与预构造后，以收据作为第一物理写。
#[allow(clippy::too_many_arguments)]
async fn apply_fresh_upgrade(
    db: &Database,
    rbac: &SharedRbacService,
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    reason: &str,
    facts: &ApprovalUpgradeSubjectFacts,
    policy: &ProcessRequiredApprovalPolicy,
    actor_role: &str,
    executor: &mut dyn Executor,
) -> Result<UpgradeBindingResultView> {
    let mut document = load_registered_document(db, &command.document_id, executor).await?;
    ensure_registered_upgrade_subject(&document, facts)?;
    document
        .ensure_unsubmitted_approval_binding_upgrade(command.expected_binding_version, reason)
        .map_err(map_binding_upgrade_error)?;
    let previous = document
        .approval_binding
        .clone()
        .ok_or_else(|| Error::ValidationError("尚未绑定审批定义".to_string()))?;

    let published = load_published_graph(db, facts.document_type, executor).await?;
    revalidate_binding_graph(db, rbac, policy, &facts.binding_context(), &published, executor).await?;
    ensure_upgrade_changes_definition(&previous, &published)?;

    let current_definition_id = ApprovalProcessDefinitionId::new(published.definition.base.id.clone());
    let current_binding_version = previous
        .approval_binding_version
        .checked_add(1)
        .ok_or_else(|| Error::Internal("审批绑定版本溢出".to_string()))?;
    let action = WorkflowAction::new_with_approval_binding_context(
        command.action_id.clone(),
        WorkflowActionData {
            document_id: BusinessDocumentId::new(facts.document_id.clone()),
            action_type: WorkflowActionType::ApprovalDefinitionUpgraded,
            from_status: "DRAFT".to_string(),
            to_status: "DRAFT".to_string(),
            actor_id: actor.id().to_string(),
            actor_role: actor_role.to_string(),
            comment: Some(reason.to_string()),
        },
        ApprovalBindingActionContext {
            previous_definition_id: previous.approval_process_definition_id.clone(),
            previous_definition_version: previous.approval_definition_version,
            previous_binding_version: previous.approval_binding_version,
            current_definition_id: current_definition_id.clone(),
            current_definition_version: published.definition.definition_version,
            current_binding_version,
            business_object_version: facts.business_object_version,
        },
    )?;
    let action_at = action_timestamp(&action)?;
    document
        .upgrade_unsubmitted_approval_binding(ApprovalBindingUpgradeInput {
            approval_process_definition_id: current_definition_id,
            approval_definition_version: published.definition.definition_version,
            expected_binding_version: command.expected_binding_version,
            reason,
            at: action_at,
        })
        .map_err(map_binding_upgrade_error)?;
    ensure_action_matches_upgraded_document(&action, &document)?;

    let receipt = ApprovalCommandReceipt::new(
        command.receipt_id.clone(),
        command.identity.current(),
        action.base.id.clone(),
        Timestamp::from_utc(action_at.as_utc()),
    )
    .map_err(map_model_error)?;
    let audit = upgraded_binding_audit(actor, facts, &previous, &action)?;
    let view = upgrade_result_from_action(
        command,
        actor,
        reason,
        &receipt,
        &action,
        UpgradeBindingOutcome::Applied,
    )?;

    db.bpm_workflow()
        .insert_command_receipt(&receipt, executor)
        .await
        .map_err(map_receipt_first_write_error)?;
    db.business_documents().update(&mut document, executor).await?;
    db.workflow_actions().create(&action, executor).await?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(view)
}

/// 注册投影必须与同一事务已读取的强业务事实精确一致。
fn ensure_registered_upgrade_subject(
    document: &BusinessDocument,
    facts: &ApprovalUpgradeSubjectFacts,
) -> Result<()> {
    let conflicting_document_no = !document.document_no.is_empty()
        && !facts.document_no.is_empty()
        && document.document_no != facts.document_no;
    if document.base.id != facts.document_id
        || document.document_type != facts.document_type
        || conflicting_document_no
    {
        return Err(Error::ConflictError(
            "业务单据注册事实与强业务对象不一致".to_string(),
        ));
    }
    Ok(())
}

/// 升级目标必须是不同且更高的发布定义。
fn ensure_upgrade_changes_definition(
    previous: &ApprovalDefinitionBinding,
    published: &DefinitionGraph,
) -> Result<()> {
    if previous.approval_process_definition_id.as_ref() == published.definition.base.id
        || published.definition.definition_version <= previous.approval_definition_version
    {
        return Err(Error::ConflictError(
            "当前绑定已是最新发布定义，禁止空升级或降级".to_string(),
        ));
    }
    Ok(())
}

/// 将不可变动作创建时间转为绑定与收据共用时间。
fn action_timestamp(action: &WorkflowAction) -> Result<Instant> {
    let secs = i64::try_from(action.base.created_at)
        .map_err(|_| Error::Internal("工作流动作时间无法转换为绑定时间".to_string()))?;
    Ok(Instant::from_unix_secs(secs))
}

/// 写入前证明内存中的注册绑定与不可变动作完全一致。
fn ensure_action_matches_upgraded_document(
    action: &WorkflowAction,
    document: &BusinessDocument,
) -> Result<()> {
    let context = action
        .approval_binding_context
        .as_ref()
        .ok_or_else(|| Error::Internal("绑定升级动作缺少结构化上下文".to_string()))?;
    let binding = document
        .approval_binding
        .as_ref()
        .ok_or_else(|| Error::Internal("升级后绑定丢失".to_string()))?;
    if binding.approval_process_definition_id != context.current_definition_id
        || binding.approval_definition_version != context.current_definition_version
        || binding.approval_binding_version != context.current_binding_version
        || binding.approval_definition_bound_at != action_timestamp(action)?
    {
        return Err(Error::Internal("绑定升级动作与注册绑定不一致".to_string()));
    }
    Ok(())
}

/// 在首笔写入前构造升级审计。
fn upgraded_binding_audit(
    actor: &AuditActor,
    facts: &ApprovalUpgradeSubjectFacts,
    previous: &ApprovalDefinitionBinding,
    action: &WorkflowAction,
) -> Result<entities::AuditLog> {
    let context = action
        .approval_binding_context
        .as_ref()
        .ok_or_else(|| Error::Internal("绑定升级动作缺少结构化上下文".to_string()))?;
    let message = format!(
        "document_type={} business_object_version={} from_definition={} from_version={} to_definition={} to_version={} action_id={}",
        facts.document_type.as_str(),
        facts.business_object_version,
        previous.approval_process_definition_id.as_ref(),
        previous.approval_definition_version,
        context.current_definition_id.as_ref(),
        context.current_definition_version,
        action.base.id,
    );
    actor.clone().resource_log_with_message(
        DEFINITION_UPGRADED_AUDIT_ACTION,
        "business_document",
        facts.document_id.clone(),
        Some(message),
    )
}

/// 从收据指向的动作严格证明并重建绑定升级结果。
fn upgrade_result_from_action(
    command: &UpgradeUnsubmittedDefinitionCommand,
    actor: &AuditActor,
    reason: &str,
    receipt: &ApprovalCommandReceipt,
    action: &WorkflowAction,
    outcome: UpgradeBindingOutcome,
) -> Result<UpgradeBindingResultView> {
    if !matches!(
        command.identity.classify(Some(receipt)),
        ReceiptBranch::SamePayload(_)
    ) {
        return Err(payload_conflict_error());
    }
    let expected_current_binding_version = command
        .expected_binding_version
        .checked_add(1)
        .ok_or_else(payload_conflict_error)?;
    let Some(context) = action.approval_binding_context.as_ref() else {
        return Err(payload_conflict_error());
    };
    let immutable_metadata = action.base.version == 1
        && action.base.deleted_at == 0
        && action.base.created_at == action.base.updated_at
        && receipt.base.version == 1
        && receipt.base.deleted_at == 0
        && receipt.base.created_at == receipt.base.updated_at
        && receipt.base.created_at == action.base.created_at;
    let exact_action = !action.base.id.trim().is_empty()
        && receipt.result_ref == action.base.id
        && action.document_id.as_ref() == command.document_id
        && action.action_type == WorkflowActionType::ApprovalDefinitionUpgraded
        && action.from_status == "DRAFT"
        && action.to_status == "DRAFT"
        && action.actor_id == actor.id()
        && !action.actor_role.trim().is_empty()
        && action.comment.as_deref() == Some(reason)
        && action.approval_context.is_none();
    let exact_context = context.business_object_version == command.expected_business_object_version
        && context.business_object_version > 0
        && context.previous_binding_version == command.expected_binding_version
        && context.current_binding_version == expected_current_binding_version
        && context.previous_binding_version > 0
        && context.previous_definition_version > 0
        && context.current_definition_version > context.previous_definition_version
        && !context.previous_definition_id.as_ref().trim().is_empty()
        && !context.current_definition_id.as_ref().trim().is_empty()
        && context.previous_definition_id != context.current_definition_id;
    if !immutable_metadata || !exact_action || !exact_context {
        return Err(payload_conflict_error());
    }
    Ok(UpgradeBindingResultView {
        document_type: command.document_type,
        document_id: command.document_id.clone(),
        original_business_object_version: context.business_object_version.to_string(),
        new_binding: UpgradeBindingView {
            approval_process_definition_id: context.current_definition_id.to_string(),
            approval_definition_version: context.current_definition_version,
            approval_binding_version: context.current_binding_version.to_string(),
            approval_definition_bound_at: action_timestamp(action).map_err(|_| payload_conflict_error())?,
        },
        action_id: action.base.id.clone(),
        outcome,
    })
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
    use crate::approval::business_adapter::ensure_runtime_cut_over;
    use crate::approval::policy::{policy_of, ALL_DOCUMENT_TYPES};
    use crate::document_registry::new_registered_document;
    use bpm::model::IdempotencyKey;
    use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
    use entities::ids::DataScopeId;
    use entities::{AccountCoreData, AccountKind, AccountStatus, LoginAccount, Secret};

    /// 构造审批人账号快照。
    fn assignee_account(id: &str, status: AccountStatus) -> AccountCore {
        AccountCore::new(
            id.to_string(),
            AccountCoreData {
                secret: Secret::new(LoginAccount::new(format!("login-{id}")).unwrap(), "password123")
                    .unwrap(),
                name: id.to_string(),
                kind: AccountKind::Admin,
                status,
                email: None,
                phone: None,
                avatar: None,
            },
        )
        .unwrap()
    }

    /// 构造角色组织范围事实。
    fn role_scope(id: &str, role_id: &str, organization_id: &str) -> DataScope {
        DataScope::new(
            DataScopeId::new(id),
            DataScopeData {
                subject_type: DataScopeSubjectType::Role,
                subject_id: role_id.to_string(),
                scope_type: DataScopeType::Organization,
                scope_targets: vec![organization_id.to_string()],
            },
        )
        .unwrap()
    }

    /// 构造严格回读单测命令。
    fn upgrade_command() -> UpgradeUnsubmittedDefinitionCommand {
        let identity = upgrade_binding_identity(
            DocumentType::StockAdjustment.as_str(),
            "adjustment-1",
            7,
            1,
            "升级至当前发布定义",
            "admin-1",
            IdempotencyKey::parse("upgrade-key-1").unwrap(),
        )
        .unwrap();
        UpgradeUnsubmittedDefinitionCommand {
            document_type: DocumentType::StockAdjustment,
            document_id: "adjustment-1".to_string(),
            expected_business_object_version: 7,
            expected_binding_version: 1,
            reason: "升级至当前发布定义".to_string(),
            identity,
            action_id: WorkflowActionId::new("action-1"),
            receipt_id: ApprovalCommandReceiptId::new("receipt-1"),
        }
    }

    /// 构造收据指向的不可变升级动作。
    fn upgrade_action(command: &UpgradeUnsubmittedDefinitionCommand) -> WorkflowAction {
        WorkflowAction::new_with_approval_binding_context(
            command.action_id.clone(),
            WorkflowActionData {
                document_id: BusinessDocumentId::new(command.document_id.clone()),
                action_type: WorkflowActionType::ApprovalDefinitionUpgraded,
                from_status: "DRAFT".to_string(),
                to_status: "DRAFT".to_string(),
                actor_id: "admin-1".to_string(),
                actor_role: "role-definition-admin".to_string(),
                comment: Some(command.reason.clone()),
            },
            ApprovalBindingActionContext {
                previous_definition_id: ApprovalProcessDefinitionId::new("definition-1"),
                previous_definition_version: 1,
                previous_binding_version: 1,
                current_definition_id: ApprovalProcessDefinitionId::new("definition-2"),
                current_definition_version: 2,
                current_binding_version: 2,
                business_object_version: 7,
            },
        )
        .unwrap()
    }

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

    /// 同载荷结果必须完全由收据指向的不可变动作重建。
    #[test]
    fn upgrade_result_is_rebuilt_from_strict_action_proof() {
        let command = upgrade_command();
        let actor = AuditActor::new("admin-1".to_string(), "admin-1".to_string(), AccountKind::Admin);
        let action = upgrade_action(&command);
        let receipt = ApprovalCommandReceipt::new(
            command.receipt_id.clone(),
            command.identity.current(),
            action.base.id.clone(),
            Timestamp::from_unix_secs(i64::try_from(action.base.created_at).unwrap()).unwrap(),
        )
        .unwrap();

        let view = upgrade_result_from_action(
            &command,
            &actor,
            &command.reason,
            &receipt,
            &action,
            UpgradeBindingOutcome::Replay,
        )
        .expect("完整动作证明必须可回读");

        assert_eq!(view.document_type, DocumentType::StockAdjustment);
        assert_eq!(view.document_id, "adjustment-1");
        assert_eq!(view.original_business_object_version, "7");
        assert_eq!(view.new_binding.approval_process_definition_id, "definition-2");
        assert_eq!(view.new_binding.approval_binding_version, "2");
        assert_eq!(view.action_id, "action-1");
        assert_eq!(view.outcome, UpgradeBindingOutcome::Replay);

        let mut corrupt = action.clone();
        corrupt.comment = Some("被篡改的原因".to_string());
        assert!(upgrade_result_from_action(
            &command,
            &actor,
            &command.reason,
            &receipt,
            &corrupt,
            UpgradeBindingOutcome::Replay,
        )
        .is_err());
    }

    /// 生产编排必须在当前授权后分流，Fresh 内的收据是第一物理写。
    #[test]
    fn upgrade_orchestration_is_authorized_replay_and_receipt_first() {
        let production = include_str!("binding.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码必须存在");
        let upgrade = production
            .split("pub async fn upgrade_unsubmitted_document_definition")
            .nth(1)
            .expect("必须存在升级端口")
            .split("/// 在 unknown/duplicate 恢复的新事务中")
            .next()
            .unwrap();
        assert!(
            upgrade.find("load_authorized_upgrade_context").unwrap()
                < upgrade.find("find_upgrade_receipt").unwrap()
        );
        assert!(
            upgrade.find("ReceiptBranch::SamePayload").unwrap()
                < upgrade.find("ensure_expected_business_object_version").unwrap()
        );
        assert!(!upgrade.contains("NoTransaction"));

        let authorization = production
            .split("async fn load_authorized_upgrade_context")
            .nth(1)
            .expect("必须存在升级授权上下文")
            .split("/// 证明运行层传入的身份")
            .next()
            .unwrap();
        assert!(
            authorization.find("load_approval_upgrade_subject_facts").unwrap()
                < authorization.find("ensure_active_upgrade_actor").unwrap()
        );
        assert!(
            authorization.find("ensure_active_upgrade_actor").unwrap()
                < authorization
                    .find("approval_binding_upgrade_authorization_with_executor")
                    .unwrap()
        );

        let recovery = production
            .split("pub async fn replay_unsubmitted_document_definition_upgrade")
            .nth(1)
            .expect("必须存在只读恢复端口")
            .split("/// 将已计算绑定写入单据实体")
            .next()
            .unwrap();
        assert!(
            recovery.find("load_authorized_upgrade_context").unwrap()
                < recovery.find("find_upgrade_receipt").unwrap()
        );
        assert!(recovery.contains("ReceiptBranch::Fresh => Ok(None)"));
        assert!(!recovery.contains("apply_fresh_upgrade"));
        assert!(!recovery.contains("insert_command_receipt"));

        let fresh = production
            .split("async fn apply_fresh_upgrade")
            .nth(1)
            .expect("必须存在 Fresh 编排")
            .split("/// 注册投影必须")
            .next()
            .unwrap();
        let receipt_write = fresh.find("insert_command_receipt").unwrap();
        assert!(fresh.find("ApprovalCommandReceipt::new").unwrap() < receipt_write);
        assert!(fresh.find("upgrade_result_from_action").unwrap() < receipt_write);
        assert!(receipt_write < fresh.find("business_documents().update").unwrap());
        assert!(receipt_write < fresh.find("workflow_actions().create").unwrap());
        assert!(receipt_write < fresh.find("audit_logs().create").unwrap());
        assert!(!fresh.contains("outbox"));
    }

    /// 单号只在注册与强实体两端均有值时作为一致性证明。
    #[test]
    fn upgrade_registry_identity_allows_one_sided_empty_document_number() {
        let document = new_registered_document("adjustment-1", DocumentType::StockAdjustment, "").unwrap();
        let facts = ApprovalUpgradeSubjectFacts {
            document_type: DocumentType::StockAdjustment,
            document_id: "adjustment-1".to_string(),
            business_object_version: 1,
            document_no: "ADJ-1".to_string(),
            responsible_org_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };
        assert!(ensure_registered_upgrade_subject(&document, &facts).is_ok());

        let conflicting =
            new_registered_document("adjustment-1", DocumentType::StockAdjustment, "ADJ-OTHER").unwrap();
        assert!(ensure_registered_upgrade_subject(&conflicting, &facts).is_err());
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

    /// BPM 确认已发布线性图；草稿/退役映射为未配置，损坏发布图不得被仓储过滤放过。
    #[test]
    fn published_graph_revalidation_uses_bpm_and_maps_configuration_errors() {
        let at = bpm::Timestamp::from_unix_secs(1).unwrap();
        let mut definition = bpm::model::ApprovalProcessDefinition::new_draft(
            bpm::ids::ApprovalProcessDefinitionId::new("def"),
            bpm::ProcessKind::StockAdjustment,
            1,
            "库存调整",
            "n1",
            bpm::ParticipantId::new("admin").unwrap(),
            at,
        )
        .unwrap();
        let graph = DefinitionGraph {
            definition: definition.clone(),
            nodes: Vec::new(),
            transitions: Vec::new(),
        };
        let draft_error = revalidate_published_graph(&graph).unwrap_err();
        assert_eq!(
            draft_error.to_string(),
            ErrorCode::ApprovalProcessNotConfigured.as_str()
        );

        definition
            .publish(bpm::ParticipantId::new("admin").unwrap(), at)
            .unwrap();
        let published_corrupt = DefinitionGraph {
            definition,
            nodes: Vec::new(),
            transitions: Vec::new(),
        };
        let corrupt = revalidate_published_graph(&published_corrupt).unwrap_err();
        assert_ne!(
            corrupt.to_string(),
            ErrorCode::ApprovalProcessNotConfigured.as_str()
        );

        let loader = include_str!("binding.rs")
            .split("async fn load_published_graph")
            .nth(1)
            .and_then(|body| body.split("fn revalidate_published_graph").next())
            .expect("加载函数");
        assert!(loader.contains("load_published_definition_graph"));
        assert!(!loader.contains("load_definition_graph"));
        assert!(!loader.contains("find_published_by_process_kind"));
    }

    /// 全部必须审批类型进入目标运行时。
    #[test]
    fn process_required_types_are_cut_over() {
        assert!(ensure_runtime_cut_over(DocumentType::StockAdjustment).is_ok());
        assert!(ensure_runtime_cut_over(DocumentType::PurchaseOrder).is_ok());
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
        assert_eq!(
            production.matches(".list_by_ids(assignee_ids, executor)").count(),
            1
        );
        assert!(production.contains(".list_by_subjects("));
        assert!(!production.contains(".find_approval_assignee_by_id(user_id, executor)"));
        assert!(production.contains("DataScopeSubjectType::Role"));
        let role_loader = production
            .split("async fn load_enabled_decide_role_scopes")
            .nth(1)
            .expect("必须存在授权角色范围加载器")
            .split("/// 筛出实际授予静态审批权限的启用角色")
            .next()
            .unwrap();
        assert!(
            role_loader.find("load_enabled_role_ids").unwrap()
                < role_loader.find("permission_granting_role_ids").unwrap()
        );
        assert!(role_loader.contains("&granting_role_ids"));
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

    /// 批量账号映射必须按 BPM 审批人顺序查表，不得依赖 Repository 返回顺序。
    #[test]
    fn assignee_account_map_preserves_bpm_validation_order() {
        let accounts = [
            assignee_account("u1", AccountStatus::Active),
            assignee_account("u2", AccountStatus::Active),
        ]
        .into_iter()
        .map(|account| (account.base.id.clone(), account))
        .collect::<HashMap<_, _>>();
        let assignee_ids = ["u2", "u1"];

        let ordered = assignee_ids
            .iter()
            .map(|user_id| {
                require_ready_assignee(accounts.get(*user_id))
                    .unwrap()
                    .base
                    .id
                    .as_str()
            })
            .collect::<Vec<_>>();

        assert_eq!(ordered, assignee_ids);
    }

    /// 缺失与停用审批人必须保留同一精确校验错误。
    #[test]
    fn missing_and_inactive_assignees_keep_exact_error() {
        let expected = "指定审批人账号不存在、已停用或任职失效";
        let missing = require_ready_assignee(None).unwrap_err();
        let inactive = assignee_account("u1", AccountStatus::Suspended);
        let inactive = require_ready_assignee(Some(&inactive)).unwrap_err();

        assert!(matches!(missing, Error::ValidationError(message) if message == expected));
        assert!(matches!(inactive, Error::ValidationError(message) if message == expected));
    }

    /// 逐用户 RBAC 失败必须保留绑定阶段的精确错误，不得借用定义期文案。
    #[test]
    fn missing_static_decide_permission_keeps_binding_error() {
        let error = require_static_decide_permission(false).unwrap_err();

        assert!(matches!(
            error,
            Error::ValidationError(message) if message == "指定审批人缺少审批权限"
        ));
        assert!(require_static_decide_permission(true).is_ok());
    }

    /// 权限角色与范围角色不同时必须失败关闭，禁止跨角色拼接授权。
    #[test]
    fn permission_and_scope_from_different_roles_cannot_be_combined() {
        let role_scope_sets = group_role_scope_facts(
            &["role-with-permission".to_string()],
            vec![role_scope("scope-b", "role-with-scope", "org-1")],
        );
        let spec = crate::approval::business_adapter::adapter_spec_of(DocumentType::StockAdjustment)
            .expect("试点必须有适配器");
        let context = BindingRevalidationContext {
            organization_id: "org-1".to_string(),
            creator_id: "creator-1".to_string(),
        };

        let error = revalidate_assignee_binding_access_by_role(&spec, &[], &role_scope_sets, &context, "u1")
            .unwrap_err();

        assert!(matches!(
            error,
            Error::ValidationError(message) if message == "审批人数据范围不覆盖当前单据组织"
        ));
        assert!(role_scope_sets[0].0.is_empty());
    }

    /// 已停用授权角色被过滤后不得靠账号级残留权限继续放行。
    #[test]
    fn no_enabled_granting_role_fails_with_exact_permission_error() {
        let error = require_granting_role_ids(Vec::new()).unwrap_err();

        assert!(matches!(
            error,
            Error::ValidationError(message) if message == "指定审批人缺少审批权限"
        ));
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
