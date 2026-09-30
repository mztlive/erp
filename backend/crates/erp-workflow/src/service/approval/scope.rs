//! 审批静态资格、类型管理权限与真实业务来源授权。

use application_core::AuditActor;
use persistence_core::{Executor, NoTransaction};

use super::dto::ApprovalRecoveryAuthorization;
use super::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

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

/// 在当前执行器中证明静态动作由仍启用的角色授予。
///
/// # 参数
/// * `rbac` - 权限事实端口。
/// * `actor` - 已认证主体。
/// * `permission` - 静态动作权限。
/// * `executor` - 当前事务执行器。
/// # 返回
/// 返回有效授权角色；空集表示无动作权限，不产生对象访问权。
/// # 错误
/// 账号失效或策略版本变化时失败关闭。
pub async fn approval_action_roles_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    if !approval_actor_is_active_with_executor(rbac, actor, executor).await? {
        return Ok(Vec::new());
    }
    let snapshot = rbac.role_permission_snapshot(actor.kind(), actor.id(), &[permission]).await?;
    let roles = rbac.enabled_role_ids(&snapshot.granting_role_ids(permission), executor).await?;
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    Ok(roles)
}

/// 校验候选审批人同一启用角色同时授予审批读取与决定能力。
///
/// # 参数
/// * `rbac` / `actor` / `executor` - 当前授权事实及事务快照。
/// # 返回
/// 资格成立时返回 true；该结果不授予任何具体单据读取权。
/// # 错误
/// 账号或策略读取失败时拒绝。
pub async fn approval_participant_permissions_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if !approval_actor_is_active_with_executor(rbac, actor, executor).await? {
        return Ok(false);
    }
    let required = ["approval_instance:read", "approval_instance:decide"];
    let snapshot = rbac.role_permission_snapshot(actor.kind(), actor.id(), &required).await?;
    let roles = rbac.enabled_role_ids(&snapshot.granting_role_ids_for_all(&required), executor).await?;
    rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
    Ok(!roles.is_empty())
}

/// 证明运行管理动作、类型管理资格和真实业务来源同时成立。
///
/// # 参数
/// * `rbac` / `actor` / `executor` - 当前身份和授权事务。
/// * `permission` - 当前管理动作。
/// * `document_type` / `document_id` - 精确主体。
/// # 返回
/// 所有边界同时成立时成功。
/// # 错误
/// 任何动作、类型或来源权限不足时拒绝。
pub async fn require_approval_management_with_executor(
    rbac: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    permission: &str,
    document_type: DocumentType,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let roles = approval_action_roles_with_executor(rbac, actor, permission, executor).await?;
    let read = approval_action_roles_with_executor(rbac, actor, "approval_instance:read", executor).await?;
    let visibility = definition_management_visibility_with_executor(rbac, actor, executor).await?;
    if roles.is_empty()
        || read.is_empty()
        || !visibility.runtime_admin_types().contains(&document_type)
        || !rbac.approval_source_readable(actor, document_type, document_id, executor).await?
    {
        return Err(Error::Forbidden("缺少审批动作、类型管理资格或业务来源访问权".into()));
    }
    Ok(())
}

/// 在稳定 Casbin policy 版本下形成恢复授权锚点。
///
/// Handler 必须把返回值原样注入恢复命令；运行时在同一恢复事务内重新读取账号、
/// 角色绑定、启用角色和 policy 版本；具体恢复命令另验类型管理与真实业务来源。
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
        let granting_role_ids =
            approval_action_roles_with_executor(rbac, actor, "approval_instance:resume", &mut NoTransaction)
                .await?;
        if granting_role_ids.is_empty() {
            return Err(Error::Forbidden("缺少审批恢复权限".into()));
        }
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
/// * `document_id` - 已加载强事实证明的精确业务主体
/// * `executor` - 调用方持有的事务执行器
///
/// # 返回
/// 动作权限、定义管理权限和真实来源读权同时成立时，
/// 返回定义管理授权角色中确定性的 `actor_role`。
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
    document_id: &str,
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
        approval_action_roles_with_executor(rbac, actor, "approval_instance:upgrade_binding", executor)
            .await?;
    let read = approval_action_roles_with_executor(rbac, actor, "approval_instance:read", executor).await?;
    if upgrade.is_empty()
        || read.is_empty()
        || !rbac.approval_source_readable(actor, document_type, document_id, executor).await?
    {
        return Err(Error::Forbidden("审批绑定升级动作或业务来源读取权限不足".into()));
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
