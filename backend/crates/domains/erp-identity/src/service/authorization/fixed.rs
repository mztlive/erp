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
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{AccessFactsPort, AccessMapping, Permission, QualifiedPermissions, Result};

/// 用固定权限、范围、对象事实及本次 Mapping 装配内存测试服务，仍执行生产 `require`、`query` 和 `validate`。
///
/// 不调用生产构建器的全量绑定检查，也不放宽它；对选用映射仍拒绝重复登记、资源或模型错配及固定规则能力不足。
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

// 为授权测试提供单启用角色及固定人员范围的内存身份设施。
struct FixedIdentity {
    // 测试角色拥有的完整权限集合，读取时与请求取交集。
    permissions: Vec<Permission>,
    // 每次范围解析返回的固定人员范围。
    scope: ResolvedScope,
}

#[async_trait]
impl Facilities for FixedIdentity {
    // 以一个固定启用角色授予请求与样本共有的权限，保留同角色资格的生产判断。
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

    // 返回测试范围及稳定版本，并沿用请求的授权时点。
    async fn scope(
        &self,
        context: &AccessContext,
        _: &Permission,
        _: &[Permission],
        _: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        Ok(AuthorizedDataScope {
            scope: self.scope.clone(),
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "fixed-scope-v1".into(),
            as_of: context.as_of,
        })
    }

    // 为不模拟记录授权变更的测试返回固定修订号。
    async fn grant_revision(&self, _: &AuditActor, _: ResourceKind, _: &mut dyn Executor) -> Result<u64> {
        Ok(1)
    }

    // 返回空补权集合，使测试只按配置的资格与范围判断。
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

// 按资源和目标 ID 读取预先提供的内存事实，不访问数据库。
struct FixedFacts(
    // 供事实端口按请求资源和 ID 筛选的固定对象集合。
    Vec<ObjectFacts>,
);

#[async_trait]
impl AccessFactsPort for FixedFacts {
    // 返回资源与目标 ID 同时匹配的样本，由生产授权流程校验结果完整性。
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

    // 验证内存绑定不会绕过生产无对象入口的权限和快照约束。
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
