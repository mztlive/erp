use std::collections::HashMap;

use bpm::model::types::ModelError;
use database::repository::bpm::DefinitionGraph;
use entities::document_registry::DocumentType;
use erp_identity::AccessControlExt;
use erp_identity::MongoCasbinAdapter;
use erp_identity::{AccountCore, RoleIdSet};
use mongodb::Database;
use persistence_core::Executor;

use crate::errors::{Error, Result};
use application_core::AuditActor;
use erp_identity::{subject, SharedRbacService};

use super::super::business_adapter::{
    adapter_spec_of, assignment_scope_covers_organization, ensure_separation_of_duties,
    revalidate_assignee_binding_access, BindingRevalidationContext,
};
use super::super::policy::{
    ApproverEligibilityPolicy, ProcessRequiredApprovalPolicy, STATIC_APPROVE_PERMISSION,
};
use super::super::scope::approval_document_read_scope_with_executor;
use super::process_not_configured;
use super::types::RoleScopeFacts;
use super::upgrade::map_model_error;

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
pub(super) fn revalidate_published_graph(graph: &DefinitionGraph) -> Result<()> {
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
pub(super) async fn revalidate_binding_graph(
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
) -> Result<(Vec<erp_identity::access_control::DataScope>, Vec<RoleScopeFacts>)> {
    let user_scopes = db
        .data_scopes()
        .list_by_subject(
            erp_identity::access_control::DataScopeSubjectType::User,
            &account.base.id,
            executor,
        )
        .await?;
    let role_scope_sets = load_enabled_decide_role_scopes(db, rbac, account, executor).await?;
    Ok((user_scopes, role_scope_sets))
}

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
            erp_identity::access_control::DataScopeSubjectType::Role,
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
    permission: &erp_identity::Permission,
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
pub(super) fn require_granting_role_ids(role_ids: Vec<String>) -> Result<Vec<String>> {
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
pub(super) fn group_role_scope_facts(
    role_ids: &[String],
    scopes: Vec<erp_identity::access_control::DataScope>,
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
pub(super) fn revalidate_assignee_binding_access_by_role(
    spec: &super::super::business_adapter::ApprovalAdapterSpec,
    user_scopes: &[erp_identity::access_control::DataScope],
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
pub(super) fn require_ready_assignee(account: Option<&AccountCore>) -> Result<&AccountCore> {
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
fn static_decide_permission() -> Result<erp_identity::Permission> {
    erp_identity::Permission::parse(STATIC_APPROVE_PERMISSION)
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
pub(super) fn require_static_decide_permission(allowed: bool) -> Result<()> {
    if allowed {
        return Ok(());
    }
    Err(Error::ValidationError("指定审批人缺少审批权限".to_string()))
}
