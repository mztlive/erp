//! 仅供测试的有限内存绑定；授权资格、范围规则及查询编译仍使用生产实现。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use persistence_core::Executor;

use super::facilities::Facilities;
use super::{AuthorizationBuilder, AuthorizationService, rules};
use crate::entity::access_control::ResolvedScope;
use crate::entity::authorization::{AccessContext, ObjectFacts, ResourceTargets};
use crate::entity::organization_change::OrganizationState;
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::binding::{FrozenMapping, validate_rule};
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeFacts};
use crate::{AccessFactsPort, AccessMapping, Permission, QualifiedPermissions, Result};

/// 组装有限测试绑定；不调用或放宽生产 Builder 的完整性校验。
/// # 参数
/// permissions、scope、objects 为固定身份事实；mappings 为测试消费的实际查询映射。
/// # 返回
/// 执行生产 require/query/validate 的共享服务。
/// # 错误
/// 模型、资源、重复映射或固定规则能力校验失败时拒绝构造。
pub(crate) fn authorization(
    permissions: &[Permission],
    scope: ResolvedScope,
    objects: Vec<ObjectFacts>,
    mappings: Vec<(Permission, Arc<dyn AccessMapping>)>,
) -> Result<Arc<AuthorizationService>> {
    let facilities = Arc::new(FixedIdentity { permissions: permissions.to_vec(), scope });
    let facts: Arc<dyn AccessFactsPort> = Arc::new(FixedFacts(objects));
    let mut builder = AuthorizationBuilder { facilities, facts: BTreeMap::new(), mappings: BTreeMap::new() };
    for permission in permissions {
        builder.facts.insert(permission.resource().into(), facts.clone());
    }
    for (permission, mapping) in mappings {
        builder = builder.mapping(&permission, mapping)?;
    }

    let mut cache = Vec::new();
    let mut frozen = BTreeMap::new();
    for (permission, mapping) in builder.mappings {
        let mapping = FrozenMapping::freeze(mapping, &mut cache, 0)?;
        let rule = rules::binding_condition(&permission, &mapping.model().output)?;
        validate_rule(&rule, mapping.as_ref(), 0)?;
        frozen.insert(permission, mapping);
    }

    Ok(Arc::new(AuthorizationService {
        facilities: builder.facilities,
        facts: builder.facts,
        mappings: frozen,
        token: Arc::new(()),
    }))
}

struct FixedIdentity {
    permissions: Vec<Permission>,
    scope: ResolvedScope,
}

#[async_trait]
impl Facilities for FixedIdentity {
    async fn permissions(
        &self,
        actor: &AuditActor,
        requested: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        let granted = self
            .permissions
            .iter()
            .filter(|permission| requested.contains(permission))
            .cloned()
            .collect::<Vec<_>>();
        Ok(Some(QualifiedPermissions::test_grants_for(actor, &[("fixed-role", &granted)])))
    }

    async fn scope(
        &self,
        context: &AccessContext,
        _: &Permission,
        _: &[Permission],
        _: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        Ok(AuthorizedDataScope {
            facts: DataScopeFacts {
                scope: self.scope.clone(),
                organizations: OrganizationState::default(),
                policy_version: 1,
                scope_version: "fixed-scope-v1".into(),
                as_of: context.as_of,
            },
        })
    }

    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Ok(1)
    }

    async fn grants(
        &self,
        _: &AccessContext,
        _: ResourceKind,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<BTreeSet<String>> {
        Ok(BTreeSet::new())
    }
}

struct FixedFacts(Vec<ObjectFacts>);

#[async_trait]
impl AccessFactsPort for FixedFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        Ok(self
            .0
            .iter()
            .filter(|object| object.resource == context.resource && ids.ids().contains(&object.id))
            .cloned()
            .collect())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_permissions::party;
    use persistence_core::NoTransaction;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;
    use crate::entity::authorization::AccessTarget;

    #[tokio::test]
    async fn fixed_facilities_preserve_production_permission_and_snapshot_rejection() {
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let scope = || ResolvedScope { role_clauses: Vec::new(), user_limit: None };
        let allowed = authorization(&[party::LIST], scope(), Vec::new(), Vec::new()).unwrap();
        allowed
            .require(&actor, &party::LIST, AccessTarget::Operation, &mut SnapshotExecutor::default())
            .await
            .unwrap();

        let denied = authorization(&[], scope(), Vec::new(), Vec::new()).unwrap();
        assert!(matches!(
            denied
                .require(&actor, &party::LIST, AccessTarget::Operation, &mut SnapshotExecutor::default())
                .await,
            Err(Error::Forbidden(_))
        ));
        assert!(
            matches!(allowed.require(&actor, &party::LIST, AccessTarget::Operation, &mut NoTransaction).await, Err(Error::ValidationError(message)) if message == "无对象操作及其业务读取必须使用快照事务")
        );
    }
}
