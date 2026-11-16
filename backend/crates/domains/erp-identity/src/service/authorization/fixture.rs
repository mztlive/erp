//! 显式身份和范围的内存设施；只随 test-support 编译，不替代生产完整装配。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use persistence_core::Executor;

use super::facilities::Facilities;
use super::{AuthorizationService, rules};
use crate::entity::access_control::ResolvedScope;
use crate::entity::access_control::scope_spec::{self, ActionKind};
use crate::entity::authorization::{AccessContext, ResourceTargets};
use crate::entity::resource_access::ResourceKind;
use crate::repository::authorization::binding::{FrozenMapping, validate_rule};
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeFacts};
use crate::{AccessFactsPort, AccessMapping, Error, Permission, QualifiedPermissions, Result};

/// 构造以真实 require/query/validate 执行固定政策的内存测试服务。
/// # 参数
/// actor 为唯一有效测试账号，permissions 是该账号单个启用角色的显式权限；
/// scope 是该角色的显式范围，facts/mappings 由消费者提供实际待测实现或窄替身。
/// # 返回
/// 仅包含显式绑定的测试服务；未绑定入口仍拒绝。查询凭证由生产编译器签发，
/// 保留原执行器、阶段、映射到期与服务实例检查，不提供可修改凭证。
/// # 错误
/// 非法权限、Facts 资源、模型或固定规则能力错配返回错误；不访问数据库。
/// 此函数仅供库单测，不经过生产 Builder，也不改变其完整装配要求。
pub fn authorization(
    actor: AuditActor,
    permissions: Vec<Permission>,
    scope: ResolvedScope,
    facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
    mappings: BTreeMap<Permission, Arc<dyn AccessMapping>>,
) -> Result<Arc<AuthorizationService>> {
    for permission in &permissions {
        if permission.resource().contains('*') || permission.action().contains('*') {
            return Err(Error::ValidationError("测试资格必须显式列出具体权限".into()));
        }
        scope_spec::kind(permission)?;
    }
    let facilities = Arc::new(Identity { actor, permissions, scope });
    for resource in facts.keys() {
        if !scope_spec::actions(resource)
            .iter()
            .any(|permission| scope_spec::kind(permission).is_ok_and(|kind| kind != ActionKind::Function))
        {
            return Err(Error::ValidationError("测试 Facts 资源未登记".into()));
        }
    }
    let mut cache = Vec::new();
    let mut frozen = BTreeMap::new();
    for (permission, mapping) in mappings {
        if !rules::query_supported(&permission)? || permission.resource() != mapping.model().resource {
            return Err(Error::ValidationError("测试查询绑定错配".into()));
        }
        mapping.model().validate()?;
        let mapping = FrozenMapping::freeze(mapping, &mut cache, 0)?;
        validate_rule(&rules::binding_condition(&permission, &mapping.model().output)?, mapping.as_ref(), 0)?;
        frozen.insert(permission, mapping);
    }
    Ok(Arc::new(AuthorizationService { facilities, facts, mappings: frozen, token: Arc::new(()) }))
}

struct Identity {
    actor: AuditActor,
    permissions: Vec<Permission>,
    scope: ResolvedScope,
}

#[async_trait]
impl Facilities for Identity {
    async fn permissions(
        &self,
        actor: &AuditActor,
        requested: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        if actor.id() != self.actor.id() || actor.kind() != self.actor.kind() {
            return Ok(None);
        }
        let permissions =
            self.permissions.iter().filter(|p| requested.contains(p)).cloned().collect::<Vec<_>>();
        Ok(Some(QualifiedPermissions::test_grants_for(actor, &[("fixture", &permissions)])))
    }

    async fn scope(
        &self,
        context: &AccessContext,
        _: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        _: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        if !qualified.same_role(required) {
            return Err(Error::Forbidden("测试角色不具备完整资格".into()));
        }
        Ok(AuthorizedDataScope::test_fixture(DataScopeFacts {
            resource: context.permission.resource().into(),
            action: context.permission.action().into(),
            scope: self.scope.clone(),
            organizations: Default::default(),
            policy_version: 1,
            scope_version: "fixture:1".into(),
            as_of: context.as_of,
        }))
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

#[cfg(test)]
mod tests;
