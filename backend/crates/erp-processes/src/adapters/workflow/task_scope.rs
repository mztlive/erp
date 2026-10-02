//! 任务管理范围按当前具体负责人的内部组织关系编译，不改写责任组织。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_identity::access_control::ScopedObject;
use erp_identity::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use erp_identity::{Error as IdentityError, Permission, SharedRbacService};
use erp_workflow::ports::WorkflowQueueAccessFact;
use erp_workflow::repository::prelude::*;
use erp_workflow::{DocumentRegistryExt, Error, Result, WorkflowAuthorizationPort};
use mongodb::Database;
use persistence_core::Executor;

use super::map_service;

/// 首拍版本与管理范围共享同事务身份事实，权限和参与关系保留原查询及首错顺序。
pub(super) async fn queue_access(
    db: &Database,
    rbac: &SharedRbacService,
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    include_version: bool,
    executor: &mut dyn Executor,
) -> Result<WorkflowQueueAccessFact> {
    let service = DataScopeService::new(db.clone(), rbac.clone());
    let permission = Permission::parse("work_item:manage").expect("固定权限合法");
    let mut batch = service.batch(actor, &[permission], executor);
    let version = if include_version {
        Some(batch.authorization_version().await.map_err(|error| map_service(error.into()))?)
    } else {
        None
    };
    let mut facts = batch.read(|executor| Box::pin(access_inputs(db, auth, actor, executor))).await?;
    facts.identity_version = version;
    let scope = match batch.resolve_permissions("work_item", "manage", &[]).await {
        Ok(scope) => scope,
        Err(IdentityError::Forbidden(_)) => {
            facts.managed_owner_ids = Some(Vec::new());
            return Ok(facts);
        },
        Err(error) => return Err(map_service(error.into())),
    };
    facts.managed_owner_ids = limited_owners(&scope)?;
    Ok(facts)
}

/// 与原工作台相同，角色、权限代码、参与单据、逐角色管理证明依次读取。
async fn access_inputs(
    db: &Database,
    auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<WorkflowQueueAccessFact> {
    let roles = auth.role_ids_with_executor(actor.kind(), actor.id(), executor).await?;
    let permission_codes = auth.permission_codes(actor.kind(), actor.id()).await?;
    let permissions = permission_codes
        .iter()
        .map(|code| Permission::parse(code).expect("granted permission must be valid"))
        .collect::<Vec<_>>();
    let participant_document_ids =
        db.document_participants().document_ids_by_user(actor.id(), executor).await?;
    let manage = Permission::parse("work_item:manage").expect("固定权限合法");
    let can_manage = if permissions.iter().any(|permission| permission.covers(&manage)) {
        !auth.roles_granting_permission(&roles, "work_item:manage").await?.is_empty()
    } else {
        false
    };
    Ok(WorkflowQueueAccessFact {
        permission_codes,
        participant_document_ids,
        managed_owner_ids: Some(Vec::new()),
        can_manage,
        identity_version: None,
    })
}

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
    limited_owners(&scope)
}

/// 编译后的负责人集合沿用原 20000 人上限与验证错误。
fn limited_owners(scope: &AuthorizedDataScope) -> Result<Option<Vec<String>>> {
    let owners = owners(scope);
    if owners.as_ref().is_some_and(|ids| ids.len() > 20_000) {
        return Err(Error::ValidationError("任务管理范围超过 20000 人，请缩小配置范围".into()));
    }
    Ok(owners)
}

/// 按公共授权范围与当前有效内部组织关系编译具体任务负责人。
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

    /// 管理范围上限在批量和原读取路径共用，边界人数允许且超限保持原验证错误。
    #[test]
    fn managed_owner_limit_preserves_exact_boundary() {
        let mut scope = context();
        let template = scope.organizations.memberships[0].clone();
        scope.organizations.memberships = (0..20_000)
            .map(|index| {
                let mut membership = template.clone();
                membership.user_id = format!("worker-{index}");
                membership
            })
            .collect();
        assert_eq!(limited_owners(&scope).unwrap().unwrap().len(), 20_000);
        let mut extra = template;
        extra.user_id = "extra-worker".into();
        scope.organizations.memberships.push(extra);
        assert!(
            matches!(limited_owners(&scope), Err(Error::ValidationError(message)) if message == "任务管理范围超过 20000 人，请缩小配置范围")
        );
    }
}
