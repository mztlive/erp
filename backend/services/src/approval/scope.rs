//! 阻塞审批管理接口的数据范围解析，以及定义管理的类型级可见范围。

use std::collections::HashMap;

use database::{AccessControlExt, MongoCasbinAdapter, NoTransaction};
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

/// 在当前 RBAC 与 DataScope 事实上计算权限的组织范围与授权角色。
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
/// 角色、RBAC、权限解析或 DataScope 事实读取失败时返回错误。
///
/// # 关键业务约束
/// Repository 批量返回事实；Service 必须逐角色完成权限与范围交集。
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
    let permitted_role_ids = permission_granting_role_ids(rbac, role_ids, &required).await?;
    let role_scopes = db
        .data_scopes()
        .list_by_subjects(
            DataScopeSubjectType::Role,
            &permitted_role_ids,
            &mut NoTransaction,
        )
        .await?;
    Ok(scope_from_role_facts(
        &user_scopes,
        permitted_role_ids,
        role_scopes,
    ))
}

/// 从当前 Casbin 事实中收敛实际授予目标权限的角色 ID。
///
/// # 参数
/// * `rbac` - 共享 RBAC 服务
/// * `role_ids` - 当前用户绑定的角色 ID
/// * `required` - 本次组织范围需要的权限
///
/// # 返回
/// 返回按输入顺序保留的授权角色 ID。
///
/// # 错误
/// 任一角色的 RBAC 判定失败时返回错误。
///
/// # 关键业务约束
/// 只有实际授予目标权限的角色才能进入 DataScope 授权交集。
async fn permission_granting_role_ids(
    rbac: &RbacService,
    role_ids: Vec<String>,
    required: &Permission,
) -> Result<Vec<String>> {
    let mut permitted = Vec::new();
    for role_id in role_ids {
        if !rbac.enforce(&format!("role:{role_id}"), required).await? {
            continue;
        }
        permitted.push(role_id);
    }
    Ok(permitted)
}

/// 用批量读取的角色范围事实计算最终组织授权。
///
/// # 参数
/// * `user_scopes` - 当前用户自身范围事实
/// * `permitted_role_ids` - 实际授予目标权限的角色 ID
/// * `role_scopes` - Repository 批量返回的角色范围事实
///
/// # 返回
/// 返回用户/角色逐角色求交后的组织范围与真正生效的角色 ID。
///
/// # 错误
/// 无；缺失角色事实按无可证明范围失败关闭。
///
/// # 关键业务约束
/// 必须先对每个角色与用户范围求交，再合并结果；禁止跨角色拼接权限与范围。
fn scope_from_role_facts(
    user_scopes: &[DataScope],
    permitted_role_ids: Vec<String>,
    role_scopes: Vec<DataScope>,
) -> (ApprovalManagementScope, Vec<String>) {
    let mut scopes_by_role = scopes_by_subject(role_scopes);
    let mut organizations = Vec::new();
    let mut granting_role_ids = Vec::new();
    for role_id in permitted_role_ids {
        let role_scopes = scopes_by_role.remove(&role_id).unwrap_or_default();
        let Some(role) = organization_coverage(&role_scopes, false) else {
            continue;
        };
        let Some(user) = organization_coverage(user_scopes, true) else {
            continue;
        };
        match intersect_coverage(role, user) {
            OrganizationCoverage::All => {
                granting_role_ids.push(role_id);
                return (ApprovalManagementScope::Company, granting_role_ids);
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
    (
        ApprovalManagementScope::Organizations(organizations),
        granting_role_ids,
    )
}

/// 将 Repository 批量返回的 DataScope 事实按主体 ID 分组。
///
/// # 参数
/// * `scopes` - 同一主体类型的 DataScope 事实
///
/// # 返回
/// 返回主体 ID 到该主体范围事实的映射。
///
/// # 错误
/// 无；本方法只对 Repository 事实分组，不执行授权判断。
fn scopes_by_subject(scopes: Vec<DataScope>) -> HashMap<String, Vec<DataScope>> {
    let mut grouped = HashMap::new();
    for scope in scopes {
        grouped
            .entry(scope.subject_id.clone())
            .or_insert_with(Vec::new)
            .push(scope);
    }
    grouped
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
        intersect_coverage, scope_from_role_facts, ApprovalManagementScope, DefinitionManagementVisibility,
        OrganizationCoverage,
    };
    use entities::access_control::{DataScope, DataScopeData, DataScopeSubjectType, DataScopeType};
    use entities::document_registry::DocumentType;
    use entities::ids::DataScopeId;

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

    /// 批量角色事实仍逐角色授权：缺失角色失败关闭，未授权角色不得扩大范围。
    #[test]
    fn role_fact_batch_keeps_role_isolation_and_missing_semantics() {
        let (scope, roles) = scope_from_role_facts(
            &[],
            vec!["role-a".to_string(), "missing-role".to_string()],
            vec![
                role_scope("scope-a", "role-a", "org-a"),
                role_scope("scope-b", "role-b", "org-b"),
            ],
        );

        assert_eq!(
            scope,
            ApprovalManagementScope::Organizations(vec!["org-a".to_string()])
        );
        assert_eq!(roles, vec!["role-a"]);
    }

    /// 空授权角色必须返回空组织范围，不得解读为公司级。
    #[test]
    fn empty_role_fact_batch_fails_closed() {
        assert_eq!(
            scope_from_role_facts(&[], Vec::new(), Vec::new()),
            (ApprovalManagementScope::Organizations(Vec::new()), Vec::new())
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
