//! 审批静态资格、类型管理权限与真实业务来源授权。
use std::slice;

use application_core::AuditActor;
use erp_identity::entity::authorization::{FactValue, ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessTarget, AuthorizationService, ResourceTargets, SharedRbacService};
use erp_permissions::{Permission, approval_instance};
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};

use super::policy::{ALL_DOCUMENT_TYPES, DocumentApprovalPolicy, policy_of};
use crate::entity::approval_qualification::ApprovalQualification;
use crate::entity::document_registry::DocumentType;
use crate::error::{Error, Result};
use crate::ports::ApprovalUpgradeSubjectFacts;

/// 定义管理的类型级可见范围。不是具体单据 对象范围。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 错误
    /// 不返回错误。
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
    ///
    /// # 错误
    /// 不返回错误。
    pub fn can_read_detail(&self, document_type: DocumentType) -> bool {
        self.can_define(document_type) || self.runtime_admin_types.contains(&document_type)
    }

    /// 返回具备定义管理权的类型切片。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回类型级管理范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn definition_admin_types(&self) -> &[DocumentType] {
        &self.definition_admin_types
    }

    /// 返回具备运行管理权的类型切片。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回类型级运行管理范围。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn runtime_admin_types(&self) -> &[DocumentType] {
        &self.runtime_admin_types
    }
}

/// 计算定义管理的类型级可见范围。
///
/// 只按各 `DocumentType` 已注册的 `definition_admin_permission` 与
/// `runtime_admin_permission` 判定，不得把系统管理员角色名当成全部类型管理权。
/// 账号绑定的停用角色即使仍残留 Casbin `g/p` 事实也不参与授权；必须由
/// 同一个仍启用的角色实际授予目标类型权限。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `actor` - 已认证操作人
///
/// # 返回
/// 返回仅由启用且实际授权角色形成的类型级可见范围。
///
/// # 错误
/// 角色、政策读取或 RBAC 判定失败时返回服务错误。
pub async fn definition_management_visibility(
    authorization: &AuthorizationService,
    db: &Database,
    identity_service: &SharedRbacService,
    actor: &AuditActor,
) -> Result<DefinitionManagementVisibility> {
    definition_management_visibility_with_executor(
        authorization,
        db,
        identity_service,
        actor,
        &mut NoTransaction,
    )
    .await
}

/// 在调用方执行器的同一数据库快照内计算定义与运行管理的类型级可见范围。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `actor` - 已认证操作人
/// * `executor` - 调用方持有的事务或非事务执行器
///
/// # 返回
/// 返回仅由事务快照内仍启用角色形成的类型级可见范围。
///
/// # 错误
/// 角色、政策读取或 RBAC 判定失败时返回服务错误。
pub async fn definition_management_visibility_with_executor(
    authorization: &AuthorizationService,
    _db: &Database,
    _identity_service: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<DefinitionManagementVisibility> {
    let mut policies = Vec::new();
    for document_type in ALL_DOCUMENT_TYPES {
        if let DocumentApprovalPolicy::ProcessRequired(policy) = policy_of(document_type)? {
            policies.push((
                document_type,
                policy.definition_admin_permission.clone(),
                policy.runtime_admin_permission.clone(),
            ));
        }
    }
    let required = policies
        .iter()
        .flat_map(|(_, define, runtime)| [define.clone(), runtime.clone()])
        .collect::<Vec<_>>();
    let qualification = authorization.permissions(actor, &required, executor).await?;
    let Some(qualification) = qualification else {
        return Ok(DefinitionManagementVisibility::default());
    };
    let permissions = qualification.permissions();
    let mut enforced = Vec::new();
    for (document_type, definition_permission, runtime_permission) in policies {
        let can_define = permissions.covers_one(&definition_permission);
        let can_runtime = permissions.covers_one(&runtime_permission);
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

/// 校验候选审批人同一启用角色同时授予审批读取与决定能力。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `identity_service` / `actor` / `executor` - 当前授权事实及事务快照。
/// # 返回
/// 资格成立时返回 true；该结果不授予任何具体单据读取权。
/// # 错误
/// 账号或策略读取失败时拒绝。
pub async fn approval_participant_permissions_with_executor(
    authorization: &AuthorizationService,
    db: &Database,
    identity_service: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<bool> {
    Ok(matches!(
        approver_qualification(authorization, db, identity_service, actor, executor).await?,
        ApprovalQualification::Eligible(_)
    ))
}

/// 统一读取审批资格，保留账号失效与权限不足的区别供各用例处理。
/// # 参数
/// identity_service 为共享身份服务，actor 为待核验审批人，executor 沿用原执行器。
/// # 返回
/// 返回账号失效、同角色权限不足或携带账号摘要的合格结果。
/// # 错误
/// 身份、策略和数据库错误保持原类别。
pub(super) async fn approver_qualification(
    authorization: &AuthorizationService,
    _db: &Database,
    _identity_service: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<ApprovalQualification> {
    let required = [approval_instance::COPIES_HISTORY.clone(), approval_instance::SUBMIT_DECISION.clone()];
    let qualified = authorization.permissions(actor, &required, executor).await?;
    Ok(ApprovalQualification::from_account(
        qualified.as_ref().map(|value| value.account().clone()),
        qualified.as_ref().is_some_and(|value| value.same_role(&required)),
    ))
}

/// 使用固定管理入口检查真实审批实例、冻结类型和完整业务来源。
/// # 参数
/// authorization 为共享服务；actor 为当前管理者；permission 为固定管理操作。
/// document_type 为调用方已加载的类型上下文；instance_id 必须是真实审批实例主键。
/// executor 为本次读取阶段或原写事务。
/// # 返回
/// identity 固定的功能资格、实际类型管理和来源读取条件均成立时成功。
/// # 错误
/// 缺少资格、冻结事实、完整来源或执行边界时拒绝。
pub async fn require_approval_management_with_executor(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    permission: &Permission,
    _document_type: DocumentType,
    instance_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization.require(actor, permission, AccessTarget::record(instance_id)?, executor).await?;
    Ok(())
}

/// 绑定升级在同一授权快照中得到的服务端身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ApprovalBindingUpgradeAuthorization {
    /// 真正授予当前类型定义管理权的启用角色。
    pub(crate) actor_role: String,
}

/// 在原事务授权未启动单据的升级候选，并选择定义管理角色作为审计来源。
/// # 参数
/// authorization 为共享服务，actor 为当前管理者，facts 为拥有领域读取的强业务事实。
/// executor 为同一原业务事务。
/// # 返回
/// 完整授权通过后返回定义管理角色；角色只用于审计。
/// # 错误
/// 资格、真实来源、强主体或审计角色缺失时拒绝。
pub(crate) async fn approval_binding_upgrade_authorization_with_executor(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    facts: &ApprovalUpgradeSubjectFacts,
    executor: &mut dyn Executor,
) -> Result<ApprovalBindingUpgradeAuthorization> {
    let DocumentApprovalPolicy::ProcessRequired(policy) = policy_of(facts.document_type)? else {
        return Err(Error::Forbidden("缺少审批类型定义管理资格".into()));
    };
    let definition = policy.definition_admin_permission.clone();
    let upgrade = approval_instance::UPGRADE_BINDING.clone();
    authorization.require(actor, &upgrade, binding_upgrade_target(facts)?, executor).await?;
    let qualified = authorization
        .permissions(actor, slice::from_ref(&definition), executor)
        .await?
        .ok_or_else(|| Error::Forbidden("审批绑定升级账号不存在、已停用或身份已变化".into()))?;
    let mut roles = qualified.roles(&definition);
    roles.sort();
    let actor_role =
        roles.into_iter().next().ok_or_else(|| Error::Forbidden("没有该类型定义管理权".into()))?.to_string();
    Ok(ApprovalBindingUpgradeAuthorization { actor_role })
}

/// 使用强业务来源构造升级候选，不把未启动的业务单据伪装成审批实例。
fn binding_upgrade_target(facts: &ApprovalUpgradeSubjectFacts) -> Result<AccessTarget> {
    let ids = ResourceTargets::new(&[facts.source_id.as_str()])?;
    let candidate = ObjectFacts {
        resource: "approval_instance".into(),
        subject_type: FactValue::Known(facts.document_type.as_str().into()),
        subject_source_resource: FactValue::Known(facts.source_resource.clone()),
        relations: [(
            RelationKey::parse("subject_source")?,
            RelatedObjects { expected_ids: ids.ids().iter().cloned().collect(), objects: vec![] },
        )]
        .into(),
        ..Default::default()
    };
    Ok(AccessTarget::Candidate(candidate))
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_identity::AccountSummary;

    use crate::entity::document_registry::DocumentType;

    fn account(id: &str, can_login: bool) -> AccountSummary {
        AccountSummary::new(id, AccountKind::Admin, can_login).with_display_name(id)
    }

    #[test]
    fn active_actor_revalidation_rejects_inactive_and_guessed_identity() {
        let actor = AuditActor::new("user-1".to_string(), "user-1".to_string(), AccountKind::Admin);
        assert!(account("user-1", true).matches_actor(&actor));
        assert!(!account("user-1", false).matches_actor(&actor));
        assert!(!account("guessed-user", true).matches_actor(&actor));
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
    #[test]
    fn binding_upgrade_uses_real_parent_identity_without_claiming_a_persisted_instance() {
        let mut facts = super::ApprovalUpgradeSubjectFacts {
            document_type: DocumentType::SalesChangeOrder,
            document_id: "change".into(),
            business_object_version: 3,
            document_no: String::new(),
            responsible_org_id: "org".into(),
            creator_id: "creator".into(),
            source_resource: "sales_order".into(),
            source_id: "order".into(),
        };
        let super::AccessTarget::Candidate(candidate) = super::binding_upgrade_target(&facts).unwrap() else {
            panic!("unsubmitted upgrade requires a candidate");
        };
        assert!(candidate.id.is_empty());
        assert!(candidate.version.is_empty());
        assert_eq!(candidate.subject_type, super::FactValue::Known("sales_change_order".into()));
        let relation = candidate.relations.values().next().unwrap();
        assert_eq!(relation.expected_ids, ["order".into()].into());
        assert!(relation.objects.is_empty());
        facts.source_id.clear();
        assert!(super::binding_upgrade_target(&facts).is_err());
    }
}
