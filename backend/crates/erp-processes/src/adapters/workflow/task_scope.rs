//! 任务管理范围按当前具体负责人的内部组织关系编译，不改写责任组织。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::{Error as IdentityError, SharedRbacService};
use erp_workflow::{Error, Result};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;

/// None 是公共解析器证明的公司范围；空集合保持失败关闭。
pub(super) async fn managed_owners(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Option<Vec<String>>> {
    let scope = match DataScopeService::new(db.clone(), rbac.clone())
        .resolve(actor, "work_item", "manage", executor)
        .await
    {
        Ok(scope) => scope,
        Err(IdentityError::Forbidden(_)) => return Ok(Some(Vec::new())),
        Err(error) => return Err(map_service(error.into())),
    };
    let owners = owners(&scope);
    if owners.as_ref().is_some_and(|ids| ids.len() > 20_000) {
        return Err(Error::ValidationError("任务管理范围超过 20000 人，请缩小配置范围".into()));
    }
    Ok(owners)
}

fn owners(context: &AuthorizedDataScope) -> Option<Vec<String>> {
    let object = |user: &str, org: Option<&str>| {
        context.scope.allows(
            &ScopedObject {
                owned: user == context.user_id,
                collaborating: false,
                historical_read_participant: false,
                org_unit_id: org,
                settlement_party_id: None,
                warehouse_id: None,
            },
            false,
        )
    };
    // 无归属、非本人仍获准，只可能由角色 Company 与个人上限共同证明。
    if object("", None) {
        return None;
    }
    let mut users = BTreeSet::new();
    if object(&context.user_id, None) {
        users.insert(context.user_id.clone());
    }
    for membership in &context.organizations.memberships {
        if !membership.base.is_deleted()
            && membership.validity.contains(context.as_of)
            && object(&membership.user_id, Some(&membership.org_unit_id))
        {
            users.insert(membership.user_id.clone());
        }
    }
    Some(users.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_identity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::entity::organization::{OrgMembership, OrgValidity};

    use super::*;

    fn context() -> AuthorizedDataScope {
        let membership = |id: &str, user: &str, org: &str, end: Option<i64>| OrgMembership {
            base: BaseModel::new(id.into()),
            user_id: user.into(),
            org_unit_id: org.into(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(1),
                valid_to: end.map(Instant::from_unix_secs),
            },
            changed_by: "admin".into(),
            reason: "fixture".into(),
        };
        let mut context = AuthorizedDataScope {
            user_id: "manager".into(),
            resource: "work_item".into(),
            action: "manage".into(),
            role_scopes: Default::default(),
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause {
                    org_unit_ids: BTreeSet::from(["dept-a".into()]),
                    ..Default::default()
                }],
                user_limit: None,
            },
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(10),
        };
        context.organizations.memberships = vec![
            membership("a", "worker-a", "dept-a", None),
            membership("b", "worker-b", "dept-b", None),
            membership("old", "expired", "dept-a", Some(10)),
        ];
        context
    }

    #[test]
    fn task_manager_uses_current_membership_not_responsibility_organization() {
        let mut scope = context();
        assert_eq!(owners(&scope), Some(vec!["worker-a".into()]));
        scope.scope.user_limit = Some(ScopeClause { self_owned: true, ..Default::default() });
        assert_eq!(owners(&scope), Some(vec![]));
        scope.scope.role_clauses = vec![ScopeClause { company: true, ..Default::default() }];
        assert_eq!(owners(&scope), Some(vec!["manager".into()]));
        scope.scope.user_limit = None;
        assert_eq!(owners(&scope), None);
        scope.scope.role_clauses.clear();
        assert_eq!(owners(&scope), Some(vec![]));
    }
}
