//! 审批动作采用内部业务组织、仓库或结算主体的独立身份维度。

use std::{slice, sync::Arc};

use application_core::AuditActor;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::{AccessControlExt, Error as IdentityError, Permission, SharedRbacService};
use erp_workflow::ports::{WorkflowDataScope, WorkflowScopeObject, WorkflowScopePredicate};
use erp_workflow::{Error, Result};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;

struct Predicate(AuthorizedDataScope);

impl WorkflowScopePredicate for Predicate {
    fn allows(&self, object: &WorkflowScopeObject) -> bool {
        let org = if self.0.resource == "supplier_settlement_statement" {
            self.0
                .organizations
                .memberships
                .iter()
                .find(|membership| {
                    !membership.base.is_deleted()
                        && membership.user_id == object.owner_user_id
                        && membership.validity.contains(self.0.as_of)
                })
                .map(|membership| membership.org_unit_id.as_str())
        } else {
            object.business_org_unit_id.as_deref()
        };
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
        let Some(clause) = self.0.role_scopes.get(role) else {
            return false;
        };
        let org = if self.0.resource == "supplier_settlement_statement" {
            self.0
                .organizations
                .memberships
                .iter()
                .find(|m| {
                    !m.base.is_deleted()
                        && m.user_id == object.owner_user_id
                        && m.validity.contains(self.0.as_of)
                })
                .map(|m| m.org_unit_id.as_str())
        } else {
            object.business_org_unit_id.as_deref()
        };
        let target = ScopedObject {
            owned: object.owner_user_id == self.0.user_id,
            collaborating: false,
            historical_read_participant: false,
            org_unit_id: org,
            settlement_party_id: object.settlement_party_id.as_deref(),
            warehouse_id: object.warehouse_id.as_deref(),
        };
        clause.covers(&target)
            && self
                .0
                .scope
                .user_limit
                .as_ref()
                .is_none_or(|limit| limit.covers(&target))
    }
}

pub(super) async fn resolve(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    permission: &str,
    executor: &mut dyn Executor,
) -> Result<Option<WorkflowDataScope>> {
    let (resource, action) = permission
        .split_once(':')
        .ok_or_else(|| Error::ValidationError("工作流动作格式不合法".into()))?;
    let scope = match DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, resource, action, executor)
        .await
    {
        Ok(scope) => scope,
        Err(IdentityError::Forbidden(_)) => return Ok(None),
        Err(error) => return Err(map_service(error.into())),
    };
    let permission = Permission::parse(permission).map_err(|e| map_service(e.into()))?;
    let snapshot = rbac
        .role_permission_snapshot(actor.kind(), actor.id(), slice::from_ref(&permission))
        .await
        .map_err(|e| map_service(e.into()))?;
    rbac.ensure_policy_snapshot_with_executor(scope.policy_version, executor)
        .await
        .map_err(|e| map_service(e.into()))?;
    let role_ids = db
        .roles()
        .enabled_roles(&snapshot.granting_role_ids(&permission), executor)
        .await
        .map_err(|e| map_service(e.into()))?
        .into_iter()
        .map(|role| role.base.id)
        .collect();
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
    use super::*;
    use erp_core::common::time::Instant;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};
    use std::collections::BTreeSet;

    #[test]
    fn approval_identity_dimensions_and_user_cap_do_not_cross_grant() {
        let mut predicate = Predicate(AuthorizedDataScope {
            user_id: "approver".into(),
            resource: "approval_instance".into(),
            action: "decide".into(),
            role_scopes: Default::default(),
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
        });
        let order = WorkflowScopeObject {
            owner_user_id: "owner".into(),
            business_org_unit_id: Some("same-id".into()),
            ..Default::default()
        };
        let warehouse = WorkflowScopeObject {
            warehouse_id: Some("same-id".into()),
            ..Default::default()
        };
        let party = WorkflowScopeObject {
            settlement_party_id: Some("same-id".into()),
            ..Default::default()
        };
        assert!(predicate.allows(&order));
        assert!(!predicate.allows(&warehouse));
        assert!(!predicate.allows(&party));
        predicate.0.scope.role_clauses[0] = ScopeClause {
            warehouse_ids: BTreeSet::from(["same-id".into()]),
            ..Default::default()
        };
        assert!(predicate.allows(&warehouse));
        assert!(!predicate.allows(&order));
        predicate.0.role_scopes.insert(
            "finance".into(),
            ScopeClause {
                warehouse_ids: BTreeSet::from(["other".into()]),
                ..Default::default()
            },
        );
        predicate
            .0
            .role_scopes
            .insert("other-role".into(), predicate.0.scope.role_clauses[0].clone());
        assert!(!predicate.allows_role("finance", &warehouse));
        assert!(predicate.allows_role("other-role", &warehouse));
        predicate.0.scope.user_limit = Some(ScopeClause::default());
        assert!(!predicate.allows(&warehouse));
        predicate.0.scope.role_clauses.clear();
        predicate.0.scope.user_limit = Some(ScopeClause {
            company: true,
            ..Default::default()
        });
        assert!(!predicate.allows(&warehouse));
    }
}
