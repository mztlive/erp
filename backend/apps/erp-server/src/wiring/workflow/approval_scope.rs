//! 工作流管理读取复用业务来源范围；该适配器不授予审批任务访问权。

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::{Error as IdentityError, Permission, SharedRbacService};
#[cfg(test)]
use erp_identity::{RoleId, service::access_control::resolve::DataScopeFacts};
use erp_workflow::ports::{WorkflowDataScope, WorkflowScopeObject, WorkflowScopePredicate};
use erp_workflow::{Error, Result};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;

struct Predicate(AuthorizedDataScope);

impl WorkflowScopePredicate for Predicate {
    fn allows(&self, object: &WorkflowScopeObject) -> bool {
        let org = object.business_org_unit_id.as_deref();
        self.0.scope.allows(
            &ScopedObject {
                owned: object.owner_user_id == self.0.user_id,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: org,
                settlement_party_id: object.settlement_party_id.as_deref(),
                warehouse_id: object.warehouse_id.as_deref(),
            },
            false,
        )
    }
    fn allows_role(&self, role: &str, object: &WorkflowScopeObject) -> bool {
        self.0.qualified_roles.contains(role) && self.allows(object)
    }
}

/// 按 `资源:动作` 解析工作流管理读取范围，不授予审批任务访问权。
///
/// # 参数
/// * `db` - 身份与角色集合所在数据库。
/// * `rbac` - 现有 RBAC 快照服务。
/// * `actor` - 当前操作人。
/// * `permission` - `资源:动作` 形式的权限码。
/// * `executor` - 调用方执行器。
///
/// # 返回
/// 无该动作权限时返回 `None`；否则返回带角色与谓词的范围。
///
/// # 错误
/// 权限码缺少冒号、权限无法解析、策略快照或角色查询失败时返回对应错误。
pub(super) async fn resolve(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<Option<WorkflowDataScope>> {
    let (resource, action) =
        permission.split_once(':').ok_or_else(|| Error::ValidationError("工作流动作格式不合法".into()))?;
    let scope = match DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, &Permission::parse(format!("{}:{}", resource, action))?, executor)
        .await
    {
        Ok(scope) => scope,
        Err(IdentityError::Forbidden(_)) => return Ok(None),
        Err(error) => return Err(map_service(error.into())),
    };
    let role_ids = scope.qualified_roles.iter().map(ToString::to_string).collect();
    Ok(Some(WorkflowDataScope::new(
        scope.resource.clone(),
        scope.action.clone(),
        scope.policy_version,
        scope.scope_version.clone(),
        role_ids,
        scope.scope.has_role_scope(),
        Arc::new(Predicate(scope)),
    )))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use erp_core::common::time::Instant;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};

    use super::*;

    #[test]
    fn source_identity_dimensions_and_user_cap_do_not_cross_grant() {
        let mut predicate = Predicate(AuthorizedDataScope::test_fixture(DataScopeFacts {
            user_id: "approver".into(),
            resource: "supplier_settlement_statement".into(),
            action: "detail".into(),
            qualified_roles: Default::default(),
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: BTreeSet::from(["same-id".into()]),
                    ..Default::default()
                }],
                user_limit: None,
            },
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(10),
        }));
        let order = WorkflowScopeObject {
            owner_user_id: "owner".into(),
            business_org_unit_id: Some("same-id".into()),
            ..Default::default()
        };
        let warehouse = WorkflowScopeObject { warehouse_id: Some("same-id".into()), ..Default::default() };
        let party = WorkflowScopeObject { settlement_party_id: Some("same-id".into()), ..Default::default() };
        assert!(predicate.allows(&order));
        assert!(!predicate.allows(&warehouse));
        assert!(!predicate.allows(&party));
        predicate.0.test_facts_mut().scope.role_clauses[0] =
            ScopeClause { warehouse_ids: BTreeSet::from(["same-id".into()]), ..Default::default() };
        assert!(predicate.allows(&warehouse));
        assert!(!predicate.allows(&order));
        predicate.0.test_facts_mut().qualified_roles.insert(RoleId::parse("finance").unwrap());
        predicate.0.test_facts_mut().qualified_roles.insert(RoleId::parse("other-role").unwrap());
        assert!(predicate.allows_role("finance", &warehouse));
        assert!(!predicate.allows_role("missing-role", &warehouse));
        assert!(predicate.allows_role("other-role", &warehouse));
        predicate.0.test_facts_mut().scope.user_limit = Some(ScopeClause::default());
        assert!(!predicate.allows(&warehouse));
        predicate.0.test_facts_mut().scope.role_clauses.clear();
        predicate.0.test_facts_mut().scope.user_limit =
            Some(ScopeClause { company: true, ..Default::default() });
        assert!(!predicate.allows(&warehouse));
    }
    #[test]
    fn settlement_review_uses_saved_business_org_without_preparer_membership() {
        let clause = ScopeClause { org_unit_ids: BTreeSet::from(["saved-org".into()]), ..Default::default() };
        let mut predicate = Predicate(AuthorizedDataScope::test_fixture(DataScopeFacts {
            user_id: "reviewer".into(),
            resource: "supplier_settlement_statement".into(),
            action: "detail".into(),
            qualified_roles: [RoleId::parse("finance").unwrap()].into(),
            scope: ResolvedScope { role_clauses: vec![clause], user_limit: None },
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(10),
        }));
        // 经办人已无有效成员关系，保存的业务组织仍是判断依据。
        let mut statement = WorkflowScopeObject {
            owner_user_id: "former-preparer".into(),
            business_org_unit_id: Some("saved-org".into()),
            ..Default::default()
        };
        assert!(predicate.allows(&statement));
        assert!(predicate.allows_role("finance", &statement));
        statement.business_org_unit_id = Some("other-org".into());
        assert!(!predicate.allows(&statement));
        assert!(!predicate.allows_role("finance", &statement));
        statement.business_org_unit_id = Some("saved-org".into());
        predicate.0.test_facts_mut().scope.user_limit = Some(ScopeClause::default());
        assert!(!predicate.allows_role("finance", &statement));
    }
}
