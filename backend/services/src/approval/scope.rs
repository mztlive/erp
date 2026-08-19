//! 阻塞审批管理接口的数据范围解析，以及定义管理的类型级可见范围。

use database::{AccessControlExt, Executor, MongoCasbinAdapter, NoTransaction};
use entities::{
    access_control::{DataScope, DataScopeSubjectType, DataScopeType},
    document_registry::DocumentType,
    Permission,
};
use mongodb::Database;

use crate::{
    audit::AuditActor,
    errors::{Error, Result},
    iam::{subject, RbacService},
};

use super::dto::ApprovalRecoveryAuthorization;
use super::policy::{policy_of, DocumentApprovalPolicy, ALL_DOCUMENT_TYPES};

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

/// 服务端计算的组织级诊断范围。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApprovalManagementScope {
    /// 公司级，可查询全部组织。
    Company,
    /// 仅可查询显式授权的组织或团队标识。
    Organizations(Vec<String>),
}

/// 定义管理的类型级可见范围。不是具体单据 DataScope。
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
    /// 返回类型级可见范围，不含单据 DataScope。
    pub fn from_type_permissions(
        definition_admin_types: Vec<DocumentType>,
        runtime_admin_types: Vec<DocumentType>,
    ) -> Self {
        Self {
            definition_admin_types,
            runtime_admin_types,
        }
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
            self.definition_admin_types
                .iter()
                .copied()
                .filter(|item| other.can_define(*item))
                .collect(),
            self.runtime_admin_types
                .iter()
                .copied()
                .filter(|item| other.runtime_admin_types.contains(item))
                .collect(),
        )
    }
}

impl ApprovalManagementScope {
    /// 返回 Repository 查询所需的可选组织切片。
    pub fn organization_ids(&self) -> Option<&[String]> {
        match self {
            Self::Company => None,
            Self::Organizations(ids) => Some(ids),
        }
    }

    /// 判断冻结责任组织是否落在当前权限来源可证明的范围内。
    pub fn covers(&self, organization_id: &str) -> bool {
        match self {
            Self::Company => true,
            Self::Organizations(ids) => ids.iter().any(|id| id == organization_id),
        }
    }
}

/// 计算定义管理的类型级可见范围。
///
/// 只按各 `DocumentType` 已注册的 `definition_admin_permission` 与
/// `runtime_admin_permission` 判定，不得把系统管理员角色名当成全部类型管理权。
///
/// # 错误
/// 政策读取或 RBAC 判定失败时返回服务错误。
pub async fn definition_management_visibility(
    rbac: &RbacService,
    actor: &AuditActor,
) -> Result<DefinitionManagementVisibility> {
    let subject = subject(actor.kind(), actor.id());
    let mut enforced = Vec::new();
    for document_type in ALL_DOCUMENT_TYPES {
        let DocumentApprovalPolicy::ProcessRequired(policy) = policy_of(document_type)? else {
            continue;
        };
        let can_define = rbac
            .enforce(&subject, &policy.definition_admin_permission)
            .await?;
        let can_runtime = rbac.enforce(&subject, &policy.runtime_admin_permission).await?;
        enforced.push((document_type, can_define, can_runtime));
    }
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

/// 从已认证身份、当前角色与数据范围形成阻塞审批查询边界。
///
/// 仅采用实际授予 `approval_instance:read` 的角色范围，并分别与用户范围
/// 求交后再合并，禁止把另一角色的权限与组织范围交叉放大。用户未配置独立范围
/// 时沿用角色范围；角色未配置可证明范围时失败关闭为空集合。空集合会交给
/// Repository 直接返回空页，不会查询全量后在应用层隐藏。
///
/// # 错误
/// 当前 RBAC policy 或数据范围仓储读取失败时返回服务错误。
pub async fn approval_management_scope(
    db: &Database,
    rbac: &RbacService,
    actor: &AuditActor,
) -> Result<ApprovalManagementScope> {
    permission_scope(db, rbac, actor, "approval_instance:read").await
}

/// 从实际授予恢复权限的角色与用户范围形成恢复授权边界。
///
/// # 错误
/// 当前 RBAC policy 或数据范围仓储读取失败时返回服务错误。
pub async fn approval_recovery_scope(
    db: &Database,
    rbac: &RbacService,
    actor: &AuditActor,
) -> Result<ApprovalManagementScope> {
    Ok(approval_recovery_authorization_scope(
        &approval_recovery_authorization(db, rbac, actor).await?,
    ))
}

/// 在稳定 Casbin policy 版本下形成恢复授权锚点。
///
/// Handler 必须把返回值原样注入恢复命令；运行时在同一恢复事务内重新读取账号、
/// 角色绑定、启用角色、数据范围和 policy 版本，禁止只信任事务外范围判断。
pub async fn approval_recovery_authorization(
    db: &Database,
    rbac: &RbacService,
    actor: &AuditActor,
) -> Result<ApprovalRecoveryAuthorization> {
    let adapter = MongoCasbinAdapter::new(db.clone());
    for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
        let before = adapter.policy_revision(&mut NoTransaction).await?;
        db.accounts()
            .find_by_id(actor.id(), &mut NoTransaction)
            .await?
            .filter(|account| account.kind == actor.kind() && account.can_login())
            .ok_or_else(|| Error::Forbidden("恢复账号不存在、已停用或身份已变化".to_string()))?;
        let (scope, granting_role_ids) =
            permission_scope_and_roles(db, rbac, actor, "approval_instance:resume").await?;
        let after = adapter.policy_revision(&mut NoTransaction).await?;
        if before == after {
            return Ok(ApprovalRecoveryAuthorization {
                actor_kind: actor.kind(),
                policy_revision: before,
                granting_role_ids,
                organization_ids: scope.organization_ids().map(ToOwned::to_owned),
            });
        }
    }
    Err(Error::Rbac(
        "审批恢复授权策略持续变化，无法形成稳定快照".to_string(),
    ))
}

async fn permission_scope(
    db: &Database,
    rbac: &RbacService,
    actor: &AuditActor,
    permission: &str,
) -> Result<ApprovalManagementScope> {
    permission_scope_and_roles(db, rbac, actor, permission)
        .await
        .map(|(scope, _)| scope)
}

async fn permission_scope_and_roles(
    db: &Database,
    rbac: &RbacService,
    actor: &AuditActor,
    permission: &str,
) -> Result<(ApprovalManagementScope, Vec<String>)> {
    let role_ids = rbac.role_ids(actor.kind(), actor.id()).await?;
    let user_scopes = db
        .data_scopes()
        .list_by_subject(DataScopeSubjectType::User, actor.id(), &mut NoTransaction)
        .await?;
    let required = Permission::parse(permission)?;
    let mut organizations = Vec::new();
    let mut granting_role_ids = Vec::new();
    for role_id in role_ids {
        if !rbac.enforce(&format!("role:{role_id}"), &required).await? {
            continue;
        }
        let role_scopes = db
            .data_scopes()
            .list_by_subject(DataScopeSubjectType::Role, &role_id, &mut NoTransaction)
            .await?;
        let Some(role) = organization_coverage(&role_scopes, false) else {
            continue;
        };
        let Some(user) = organization_coverage(&user_scopes, true) else {
            continue;
        };
        match intersect_coverage(role, user) {
            OrganizationCoverage::All => {
                granting_role_ids.push(role_id);
                return Ok((ApprovalManagementScope::Company, granting_role_ids));
            }
            OrganizationCoverage::Targets(targets) if !targets.is_empty() => {
                organizations.extend(targets);
                granting_role_ids.push(role_id);
            }
            OrganizationCoverage::Targets(_) => {}
        }
    }
    organizations.sort();
    organizations.dedup();
    granting_role_ids.sort();
    granting_role_ids.dedup();
    Ok((
        ApprovalManagementScope::Organizations(organizations),
        granting_role_ids,
    ))
}

/// 从恢复授权锚点恢复 Repository/管理服务使用的组织范围。
pub fn approval_recovery_authorization_scope(
    authorization: &ApprovalRecoveryAuthorization,
) -> ApprovalManagementScope {
    authorization.organization_ids.clone().map_or(
        ApprovalManagementScope::Company,
        ApprovalManagementScope::Organizations,
    )
}

/// 在恢复事务快照内重新证明账号、角色、权限策略版本与组织范围。
pub(crate) async fn ensure_recovery_authorization(
    db: &Database,
    authorization: &ApprovalRecoveryAuthorization,
    actor_id: &str,
    owner_organization_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    if authorization.granting_role_ids.is_empty()
        || !approval_recovery_authorization_scope(authorization).covers(owner_organization_id)
    {
        return Err(Error::Forbidden("审批恢复授权范围不覆盖当前实例".to_string()));
    }
    db.accounts()
        .find_by_id(actor_id, executor)
        .await?
        .filter(|account| account.kind == authorization.actor_kind && account.can_login())
        .ok_or_else(|| Error::Forbidden("恢复账号不存在、已停用或身份已变化".to_string()))?;

    let adapter = MongoCasbinAdapter::new(db.clone());
    let revision = adapter.policy_revision(executor).await?;
    if revision != authorization.policy_revision {
        return Err(Error::Forbidden(
            "审批恢复授权策略已变化，请刷新后重试".to_string(),
        ));
    }
    let assigned_roles = adapter
        .subject_roles(&subject(authorization.actor_kind, actor_id), executor)
        .await?;
    let user_scopes = db
        .data_scopes()
        .list_by_subject(DataScopeSubjectType::User, actor_id, executor)
        .await?;
    let mut currently_covered = false;
    for role_id in &authorization.granting_role_ids {
        if !assigned_roles
            .iter()
            .any(|role| role == &format!("role:{role_id}"))
        {
            return Err(Error::Forbidden(
                "审批恢复角色绑定已变化，请刷新后重试".to_string(),
            ));
        }
        db.roles()
            .find_by_id(role_id, executor)
            .await?
            .filter(|role| !role.disabled)
            .ok_or_else(|| Error::Forbidden("审批恢复角色已停用或不存在".to_string()))?;
        let role_scopes = db
            .data_scopes()
            .list_by_subject(DataScopeSubjectType::Role, role_id, executor)
            .await?;
        let Some(role) = organization_coverage(&role_scopes, false) else {
            continue;
        };
        let Some(user) = organization_coverage(&user_scopes, true) else {
            continue;
        };
        currently_covered |= match intersect_coverage(role, user) {
            OrganizationCoverage::All => true,
            OrganizationCoverage::Targets(targets) => {
                targets.iter().any(|target| target == owner_organization_id)
            }
        };
    }
    if !currently_covered {
        return Err(Error::Forbidden("审批恢复数据范围已不覆盖当前实例".to_string()));
    }
    let final_revision = adapter.policy_revision(executor).await?;
    if final_revision != authorization.policy_revision {
        return Err(Error::Forbidden(
            "审批恢复授权策略已变化，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum OrganizationCoverage {
    All,
    Targets(Vec<String>),
}

fn organization_coverage(scopes: &[DataScope], empty_is_all: bool) -> Option<OrganizationCoverage> {
    if scopes
        .iter()
        .any(|scope| scope.scope_type == DataScopeType::Company)
    {
        return Some(OrganizationCoverage::All);
    }
    let mut organizations = scopes
        .iter()
        .filter(|scope| {
            matches!(
                scope.scope_type,
                DataScopeType::Organization | DataScopeType::Team
            )
        })
        .flat_map(|scope| scope.scope_targets.clone())
        .collect::<Vec<_>>();
    organizations.sort();
    organizations.dedup();
    if organizations.is_empty() {
        return empty_is_all.then_some(OrganizationCoverage::All);
    }
    Some(OrganizationCoverage::Targets(organizations))
}

fn intersect_coverage(role: OrganizationCoverage, user: OrganizationCoverage) -> OrganizationCoverage {
    match (role, user) {
        (OrganizationCoverage::All, OrganizationCoverage::All) => OrganizationCoverage::All,
        (OrganizationCoverage::Targets(targets), OrganizationCoverage::All)
        | (OrganizationCoverage::All, OrganizationCoverage::Targets(targets)) => {
            OrganizationCoverage::Targets(targets)
        }
        (OrganizationCoverage::Targets(role), OrganizationCoverage::Targets(user)) => {
            OrganizationCoverage::Targets(
                role.into_iter()
                    .filter(|organization_id| user.contains(organization_id))
                    .collect(),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        intersect_coverage, ApprovalManagementScope, DefinitionManagementVisibility, OrganizationCoverage,
    };
    use entities::document_registry::DocumentType;

    #[test]
    fn company_scope_has_no_repository_organization_filter() {
        assert!(ApprovalManagementScope::Company.organization_ids().is_none());
        let empty = ApprovalManagementScope::Organizations(Vec::new());
        assert_eq!(empty.organization_ids(), Some([].as_slice()));
    }

    #[test]
    fn user_scope_restricts_role_company_scope() {
        assert_eq!(
            intersect_coverage(
                OrganizationCoverage::All,
                OrganizationCoverage::Targets(vec!["organization-1".to_string()]),
            ),
            OrganizationCoverage::Targets(vec!["organization-1".to_string()])
        );
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
        assert_eq!(
            visibility.definition_admin_types(),
            &[DocumentType::StockAdjustment]
        );
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
        assert_eq!(
            intersected.definition_admin_types(),
            &[DocumentType::StockAdjustment]
        );
        assert_eq!(intersected.runtime_admin_types(), &[DocumentType::SalesOrder]);
        assert!(!intersected.can_define(DocumentType::SalesOrder));
        assert!(!intersected.can_read_detail(DocumentType::CustomerReceipt));
    }
}
