//! 阻塞审批管理接口的数据范围解析，以及定义管理的类型级可见范围。

use application_core::AuditActor;
use persistence_core::{Executor, NoTransaction};

use super::dto::ApprovalRecoveryAuthorization;
use super::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use crate::entity::document_registry::DocumentType;
use crate::entity::work_item::WorkItemType;
use crate::error::{Error, Result};
use crate::ports::{WorkflowAuthorizationPort, WorkflowDataScope, WorkflowScopeObject};

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

/// 服务端计算的组织级诊断范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalManagementScope {
    /// 公共解析器返回的当前对象范围。
    Resolved(WorkflowDataScope),
    /// 没有完整动作权限或正向范围。
    Empty,
}

/// 定义管理的类型级可见范围。不是具体单据 对象范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DefinitionManagementVisibility {
    definition_admin_types: Vec<DocumentType>,
    runtime_admin_types: Vec<DocumentType>,
}

impl DefinitionManagementVisibility {
    /// 由已判定的类型级权限构造可见范围。
    ///
    /// # 参数
    /// * `definition_admin_types` - 具备定义管理权的类型
    /// * `runtime_admin_types` - 具备运行管理权的类型
    ///
    /// # 返回
    /// 返回类型级可见范围，不含单据 对象范围。
    pub fn from_type_permissions(
        definition_admin_types: Vec<DocumentType>,
        runtime_admin_types: Vec<DocumentType>,
    ) -> Self {
        Self { definition_admin_types, runtime_admin_types }
    }

    /// 判断是否具备该类型的定义管理权。
    ///
    /// # 参数
    /// * `document_type` - 固定单据类型
    ///
    /// # 返回
    /// 拥有 `definition_admin_permission` 时返回 `true`。
    pub fn can_define(&self, document_type: DocumentType) -> bool {
        self.definition_admin_types.contains(&document_type)
    }

    /// 判断是否可读取该类型定义版本与详情。
    ///
    /// # 参数
    /// * `document_type` - 固定单据类型
    ///
    /// # 返回
    /// 拥有定义管理或运行管理权限时返回 `true`。
    pub fn can_read_detail(&self, document_type: DocumentType) -> bool {
        self.can_define(document_type) || self.runtime_admin_types.contains(&document_type)
    }

    /// 返回具备定义管理权的类型切片。
    ///
    /// # 返回
    /// 返回类型级管理范围。
    pub fn definition_admin_types(&self) -> &[DocumentType] {
        &self.definition_admin_types
    }

    /// 返回具备运行管理权的类型切片。
    ///
    /// # 返回
    /// 返回类型级运行管理范围。
    pub fn runtime_admin_types(&self) -> &[DocumentType] {
        &self.runtime_admin_types
    }

    /// 与另一范围求交，防止调用方扩大已证明的类型级权限。
    ///
    /// # 参数
    /// * `other` - 另一份类型级范围
    ///
    /// # 返回
    /// 返回两端都具备的类型集合。
    pub fn intersect(&self, other: &Self) -> Self {
        Self::from_type_permissions(
            self.definition_admin_types.iter().copied().filter(|item| other.can_define(*item)).collect(),
            self.runtime_admin_types
                .iter()
                .copied()
                .filter(|item| other.runtime_admin_types.contains(item))
                .collect(),
        )
    }
}

impl ApprovalManagementScope {
    /// 对当前强业务对象执行公共范围判定；旧组织集合不得判定新对象。
    pub fn covers_object(&self, object: &WorkflowScopeObject) -> bool {
        match self {
            Self::Resolved(scope) => scope.allows(object),
            _ => false,
        }
    }

    /// 判断当前权限是否没有任何可证明的数据范围。
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Resolved(scope) => !scope.has_role_scope,
            Self::Empty => true,
        }
    }
}

/// 重验当前审批读取主体仍是同类型的有效账号。
///
/// # 参数
/// * `auth` - 注入的授权 Port
/// * `actor` - Handler 已认证但可能已经失效的身份快照
///
/// # 返回
/// 账号仍存在、类型未漂移且允许登录时返回 `true`；不存在、停用或类型漂移时
/// 返回 `false`。
///
/// # 错误
/// 账号仓储读取失败时返回服务错误。
///
/// # 关键业务约束
/// 调用方必须在读取具体审批资源前执行本重验，并把 `false` 映射为不泄露资源
/// 存在性的拒绝结果。
pub async fn approval_actor_is_active<A: WorkflowAuthorizationPort>(
    auth: &A,
    actor: &AuditActor,
) -> Result<bool> {
    approval_actor_is_active_with_executor(auth, actor, &mut NoTransaction).await
}

/// 在调用方执行器的同一数据库快照内重验审批主体仍有效。
///
/// # 参数
/// * `auth` - 注入的授权 Port
/// * `actor` - Handler 已认证的身份快照
/// * `executor` - 调用方持有的事务或非事务执行器
///
/// # 返回
/// 账号仍存在、类型未漂移且允许登录时返回 `true`。
///
/// # 错误
/// 账号仓储读取失败时返回服务错误。
pub async fn approval_actor_is_active_with_executor<A: WorkflowAuthorizationPort>(
    auth: &A,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    Ok(auth
        .load_account(actor.id(), executor)
        .await?
        .as_ref()
        .is_some_and(|account| approval_account_matches_actor(account, actor)))
}

/// 判断当前账号事实是否仍与认证主体一致且可用。
fn approval_account_matches_actor(
    account: &crate::entity::work_item::WorkflowAccountFact,
    actor: &AuditActor,
) -> bool {
    account.id == actor.id() && account.kind == actor.kind() && account.can_login
}

/// 计算定义管理的类型级可见范围。
///
/// 只按各 `DocumentType` 已注册的 `definition_admin_permission` 与
/// `runtime_admin_permission` 判定，不得把系统管理员角色名当成全部类型管理权。
/// 账号绑定的停用角色即使仍残留 Casbin `g/p` 事实也不参与授权；必须由
/// 同一个仍启用的角色实际授予目标类型权限。
///
/// # 参数
/// * `rbac` - 注入的授权 Port
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回仅由启用且实际授权角色形成的类型级可见范围。
///
/// # 错误
/// 角色、政策读取或 RBAC 判定失败时返回服务错误。
pub async fn definition_management_visibility(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
) -> Result<DefinitionManagementVisibility> {
    definition_management_visibility_with_executor(rbac, actor, &mut NoTransaction).await
}

/// 在调用方执行器的同一数据库快照内计算定义与运行管理的类型级可见范围。
///
/// # 参数
/// * `rbac` - 注入的授权 Port
/// * `actor` - 已认证操作人
/// * `executor` - 调用方持有的事务或非事务执行器
///
/// # 返回
/// 返回仅由事务快照内仍启用角色形成的类型级可见范围。
///
/// # 错误
/// 角色、政策读取或 RBAC 判定失败时返回服务错误。
pub async fn definition_management_visibility_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<DefinitionManagementVisibility> {
    let policies = ALL_DOCUMENT_TYPES
        .iter()
        .copied()
        .filter_map(|document_type| match policy_of(document_type) {
            Ok(DocumentApprovalPolicy::ProcessRequired(policy)) => {
                Some(Ok((document_type, policy.definition_admin_permission, policy.runtime_admin_permission)))
            },
            Ok(DocumentApprovalPolicy::NoApproval(_)) => None,
            Err(error) => Some(Err(error)),
        })
        .collect::<Result<Vec<_>>>()?;
    let required_codes = policies
        .iter()
        .flat_map(|(_, define, runtime)| [define.clone(), runtime.clone()])
        .collect::<Vec<_>>();
    let required = required_codes.iter().map(String::as_str).collect::<Vec<_>>();
    let policy_snapshot = rbac.role_permission_snapshot(actor.kind(), actor.id(), &required).await?;
    let role_ids = rbac.enabled_role_ids(policy_snapshot.role_ids(), executor).await?;
    let mut enforced = Vec::new();
    for (document_type, definition_permission, runtime_permission) in policies {
        let can_define = policy_snapshot
            .granting_role_ids(&definition_permission)
            .iter()
            .any(|role_id| role_ids.contains(role_id));
        let can_runtime = policy_snapshot
            .granting_role_ids(&runtime_permission)
            .iter()
            .any(|role_id| role_ids.contains(role_id));
        enforced.push((document_type, can_define, can_runtime));
    }
    rbac.ensure_policy_snapshot_with_executor(policy_snapshot.policy_revision(), executor).await?;
    Ok(visibility_from_enforced_permissions(enforced))
}

/// 按各类型 enforce 结果构造可见范围，不把系统管理员角色当成全部类型管理权。
///
/// # 参数
/// * `rows` - `(单据类型, 定义管理, 运行管理)` 判定结果
///
/// # 返回
/// 返回仅包含已判定为真的类型集合。
fn visibility_from_enforced_permissions(
    rows: impl IntoIterator<Item = (DocumentType, bool, bool)>,
) -> DefinitionManagementVisibility {
    let mut definition_admin_types = Vec::new();
    let mut runtime_admin_types = Vec::new();
    for (document_type, can_define, can_runtime) in rows {
        if can_define {
            definition_admin_types.push(document_type);
        }
        if can_runtime {
            runtime_admin_types.push(document_type);
        }
    }
    DefinitionManagementVisibility::from_type_permissions(definition_admin_types, runtime_admin_types)
}

/// 计算指定审批单据类型的对象读取 对象范围。
///
/// # 参数
/// * `db` - 当前 MongoDB 数据库
/// * `rbac` - 当前 RBAC 服务
/// * `actor` - 已认证且已重验的操作人
/// * `document_type` - 审批运行时固定单据类型
///
/// # 返回
/// 返回由真正授予该业务对象读取权限的角色与用户范围形成的组织范围；未获得
/// 权限或没有可证明范围时返回空组织集合。
///
/// # 错误
/// 单据类型未登记 DocumentApproval 简报关系、权限格式非法或事实读取失败时
/// 返回服务错误。
///
/// # 关键业务约束
/// 读取权限必须来自 Entity-owned `WorkItemBriefRelation`，不得在审批 Service
/// 维护第二份 DocumentType 权限表；不同单据类型的范围必须分别计算，禁止先
/// 合并组织再交给 Repository。
pub async fn approval_document_read_scope(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    document_type: DocumentType,
) -> Result<ApprovalManagementScope> {
    approval_document_read_scope_with_executor(rbac, actor, document_type, &mut NoTransaction).await
}

/// 在调用方执行器的同一数据库快照内计算对象读取 对象范围。
///
/// # 错误
/// 单据类型未登记、权限格式非法或授权事实读取失败时返回服务错误。
pub async fn approval_document_read_scope_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    document_type: DocumentType,
    executor: &mut dyn Executor,
) -> Result<ApprovalManagementScope> {
    let relation = WorkItemType::DocumentApproval
        .brief_relation(document_type.as_str())
        .ok_or_else(|| Error::from_approval_code(crate::error::ErrorCode::ApprovalPolicyNotRegistered))?;
    let snapshot =
        rbac.role_permission_snapshot(actor.kind(), actor.id(), &[relation.read_permission]).await?;
    let roles =
        rbac.enabled_role_ids(&snapshot.granting_role_ids(relation.read_permission), executor).await?;
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    if roles.is_empty() {
        return Ok(ApprovalManagementScope::Empty);
    }
    permission_scope_with_executor(rbac, actor, "approval_instance:read", executor).await
}

/// 在调用方执行器的同一数据库快照内计算普通取消动作 对象范围。
///
/// # 错误
/// 账号角色、RBAC policy、权限代码或 对象范围 事实读取失败时返回服务错误。
#[allow(dead_code)]
pub async fn approval_cancel_scope_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalManagementScope> {
    permission_scope_with_executor(rbac, actor, "approval_instance:cancel", executor).await
}

/// 在调用方执行器的同一数据库快照内计算受阻取消动作 对象范围。
///
/// # 错误
/// 账号角色、RBAC policy、权限代码或 对象范围 事实读取失败时返回服务错误。
///
/// # 关键业务约束
/// `cancel_blocked` 与普通 `cancel` 是两个独立动作权限，禁止以恢复或普通取消
/// 权限替代；只有真正授予本动作的启用角色范围才能参与实例组织授权。
pub async fn approval_cancel_blocked_scope_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalManagementScope> {
    permission_scope_with_executor(rbac, actor, "approval_instance:cancel_blocked", executor).await
}

/// 在调用方执行器的同一数据库快照内计算审批决定动作 对象范围。
///
/// # 错误
/// 账号角色、RBAC policy、权限代码或 对象范围 事实读取失败时返回服务错误。
///
/// # 关键业务约束
/// 决定权限必须由同一个仍启用的角色授予，并与该角色及用户的组织范围求交；
/// 禁止把停用角色残留策略或不同角色的权限与范围拼接成有效授权。
pub async fn approval_decide_scope_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalManagementScope> {
    permission_scope_with_executor(rbac, actor, "approval_instance:decide", executor).await
}

/// 在调用方执行器的同一数据库快照内计算指定单据动作的 对象范围。
///
/// # 参数
/// * `db` - 当前 MongoDB 数据库
/// * `rbac` - 当前 RBAC 服务
/// * `actor` - 已认证且已重验的操作人
/// * `permission` - 业务单据已登记的稳定动作权限代码
/// * `executor` - 调用方持有的事务或非事务执行器
///
/// # 返回
/// 返回公共解析器证明的资源动作对象范围；没有有效
/// 授权或范围时返回空组织集合。
///
/// # 错误
/// 账号角色、RBAC policy、权限代码或 对象范围 事实读取失败时返回服务错误。
///
/// # 关键业务约束
/// 本方法仅供 Service 在具体单据事务内重验 actor-specific 动作权限。调用方仍
/// 必须把返回范围与当前业务对象的身份维度精确比对；不得把权限字符串或授权判断下沉到
/// Repository、Entity 或 BPM。
#[allow(dead_code)]
pub async fn approval_document_action_scope_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalManagementScope> {
    permission_scope_with_executor(rbac, actor, permission, executor).await
}

/// 从实际授予恢复权限的角色与用户范围形成恢复授权边界。
///
/// # 错误
/// 当前 RBAC policy 或数据范围仓储读取失败时返回服务错误。
pub async fn approval_recovery_scope(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
) -> Result<ApprovalManagementScope> {
    permission_scope_with_executor(rbac, actor, "approval_instance:resume", &mut NoTransaction).await
}

/// 在稳定 Casbin policy 版本下形成恢复授权锚点。
///
/// Handler 必须把返回值原样注入恢复命令；运行时在同一恢复事务内重新读取账号、
/// 角色绑定、启用角色、数据范围和 policy 版本，禁止只信任事务外范围判断。
pub async fn approval_recovery_authorization(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
) -> Result<ApprovalRecoveryAuthorization> {
    for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
        let before = rbac.current_policy_revision().await?;
        rbac.load_account(actor.id(), &mut NoTransaction)
            .await?
            .filter(|account| account.kind == actor.kind() && account.can_login)
            .ok_or_else(|| Error::Forbidden("恢复账号不存在、已停用或身份已变化".to_string()))?;
        let (_scope, granting_role_ids) =
            permission_scope_and_roles(rbac, actor, "approval_instance:resume").await?;
        let after = rbac.current_policy_revision().await?;
        if before == after {
            return Ok(ApprovalRecoveryAuthorization {
                actor_kind: actor.kind(),
                policy_revision: before,
                granting_role_ids,
            });
        }
    }
    Err(Error::Rbac("审批恢复授权策略持续变化，无法形成稳定快照".to_string()))
}

/// 使用调用方执行器计算权限与组织范围交集。
async fn permission_scope_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<ApprovalManagementScope> {
    permission_scope_and_roles_with_executor(rbac, actor, permission, executor).await.map(|(scope, _)| scope)
}

/// 在当前 RBAC 与 对象范围 事实上计算权限的组织范围与授权角色。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `actor` - 已认证操作人
/// * `permission` - 需要判定的稳定权限代码
///
/// # 返回
/// 返回不扩大用户/角色交集的组织范围与实际生效角色 ID。
///
/// # 错误
/// 角色、RBAC、权限解析或 对象范围 事实读取失败时返回错误。
///
/// # 关键业务约束
/// Repository 批量返回事实；Service 必须逐角色完成权限与范围交集。
async fn permission_scope_and_roles(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    permission: &str,
) -> Result<(ApprovalManagementScope, Vec<String>)> {
    permission_scope_and_roles_with_executor(rbac, actor, permission, &mut NoTransaction).await
}

/// 在调用方执行器内计算权限范围与实际授权角色。
pub(crate) async fn permission_scope_and_roles_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<(ApprovalManagementScope, Vec<String>)> {
    match rbac.resolve_workflow_scope(actor, permission, executor).await? {
        Some(scope) => {
            let roles = scope.granting_role_ids.clone();
            Ok((ApprovalManagementScope::Resolved(scope), roles))
        },
        None => Ok((ApprovalManagementScope::Empty, Vec::new())),
    }
}

/// 绑定升级在同一授权快照中得到的服务端身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApprovalBindingUpgradeAuthorization {
    /// 真正授予当前类型定义管理权的启用角色。
    pub(crate) actor_role: String,
}

/// 在调用方事务快照中形成绑定升级的三重授权证明。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `rbac` - 共享 RBAC 服务
/// * `actor` - 已在同一事务中重验为有效的操作人
/// * `document_type` - 强业务对象证明的精确单据类型
/// * `definition_admin_permission` - 该类型政策注册的定义管理权限
/// * `responsible_org_id` - 强业务对象或其固定责任链给出的组织
/// * `executor` - 调用方持有的事务执行器
///
/// # 返回
/// 动作权限、定义管理权限和对象读权范围分别覆盖责任组织时，
/// 返回真正覆盖该组织的定义管理授权角色中确定性的 `actor_role`。
///
/// # 错误
/// 权限、角色、对象范围、对象读取关系或 policy revision 任一无法证明时
/// 失败关闭。
///
/// # 关键业务约束
/// 三项权限在一次 Enforcer 读锁中冻结，并共享一次事务内 revision
/// fence。每项权限各自只能使用真正授予该权限的启用角色
/// 对象范围；三项权限可以来自不同角色。
pub(crate) async fn approval_binding_upgrade_authorization_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    document_type: DocumentType,
    definition_admin_permission: &str,
    object: &WorkflowScopeObject,
    executor: &mut dyn Executor,
) -> Result<ApprovalBindingUpgradeAuthorization> {
    let snapshot =
        rbac.role_permission_snapshot(actor.kind(), actor.id(), &[definition_admin_permission]).await?;
    let mut roles =
        rbac.enabled_role_ids(&snapshot.granting_role_ids(definition_admin_permission), executor).await?;
    roles.sort();
    let actor_role =
        roles.into_iter().next().ok_or_else(|| Error::Forbidden("没有该类型定义管理权".into()))?;
    let upgrade =
        permission_scope_with_executor(rbac, actor, "approval_instance:upgrade_binding", executor).await?;
    let read = approval_document_read_scope_with_executor(rbac, actor, document_type, executor).await?;
    if !upgrade.covers_object(object) || !read.covers_object(object) {
        return Err(Error::Forbidden("审批绑定升级动作或读取范围不覆盖当前业务对象".into()));
    }
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    Ok(ApprovalBindingUpgradeAuthorization { actor_role })
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;

    use super::{DefinitionManagementVisibility, approval_account_matches_actor};
    use crate::entity::document_registry::DocumentType;
    use crate::entity::work_item::WorkflowAccountFact;

    fn account(id: &str, can_login: bool) -> WorkflowAccountFact {
        WorkflowAccountFact::new(id, AccountKind::Admin, can_login).with_display_name(id)
    }

    #[test]
    fn active_actor_revalidation_rejects_inactive_and_guessed_identity() {
        let actor = AuditActor::new("user-1".to_string(), "user-1".to_string(), AccountKind::Admin);
        assert!(approval_account_matches_actor(&account("user-1", true), &actor));
        assert!(!approval_account_matches_actor(&account("user-1", false), &actor));
        assert!(!approval_account_matches_actor(&account("guessed-user", true), &actor));
    }

    /// 类型级可见范围只认已登记权限，不把系统管理员角色当成全部类型管理权。
    #[test]
    fn definition_visibility_is_type_level_not_role_name() {
        let visibility = super::visibility_from_enforced_permissions([
            (DocumentType::StockAdjustment, true, false),
            (DocumentType::SalesOrder, false, true),
            (DocumentType::Invoice, false, false),
        ]);
        assert!(visibility.can_define(DocumentType::StockAdjustment));
        assert!(!visibility.can_define(DocumentType::SalesOrder));
        assert!(visibility.can_read_detail(DocumentType::SalesOrder));
        assert!(!visibility.can_read_detail(DocumentType::Invoice));
        assert_eq!(visibility.definition_admin_types(), &[DocumentType::StockAdjustment]);
    }

    /// 求交不能放大调用方范围。
    #[test]
    fn visibility_intersect_cannot_enlarge_caller_scope() {
        let proven = DefinitionManagementVisibility::from_type_permissions(
            vec![DocumentType::StockAdjustment],
            vec![DocumentType::SalesOrder],
        );
        let claimed = DefinitionManagementVisibility::from_type_permissions(
            vec![DocumentType::StockAdjustment, DocumentType::SalesOrder],
            vec![DocumentType::SalesOrder, DocumentType::CustomerReceipt],
        );
        let intersected = proven.intersect(&claimed);
        assert_eq!(intersected.definition_admin_types(), &[DocumentType::StockAdjustment]);
        assert_eq!(intersected.runtime_admin_types(), &[DocumentType::SalesOrder]);
        assert!(!intersected.can_define(DocumentType::SalesOrder));
        assert!(!intersected.can_read_detail(DocumentType::CustomerReceipt));
    }
}
