//! 仅 test-support 提供的身份设施替身；仍执行真实 require/query/validate 和查询编译器。
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use super::AuthorizationService;
use super::facilities::Facilities;
use crate::entity::access_control::{ResolvedScope, ScopeClause};
use crate::entity::authorization::{AccessContext, ResourceTargets};
use crate::entity::organization_change::OrganizationState;
use crate::entity::resource_access::ResourceKind;
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeFacts};
use crate::{AccessFactsPort, AccessMapping, CompiledAccess, Permission, QualifiedPermissions, Result};

/// 为单个领域的内联单测注入功能资格、范围和实际 Mapping；不改变生产 Builder 合同。
pub struct AuthorizationFixture {
    grants: Vec<(String, Vec<Permission>)>,
    scope: ResolvedScope,
    facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
    mappings: BTreeMap<Permission, Arc<dyn AccessMapping>>,
}

impl AuthorizationFixture {
    /// 声明测试角色授予的权限，默认范围为本人。
    /// # 参数
    /// grants 为测试启用角色及其明确权限，不扩展为其他资格。
    /// # 返回
    /// 尚未绑定任何业务 Facts 或 Mapping 的测试设施。
    /// # 错误
    /// 不执行 I/O。
    pub fn new(grants: &[(&str, &[Permission])]) -> Self {
        Self {
            grants: grants.iter().map(|(role, permissions)| ((*role).into(), permissions.to_vec())).collect(),
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
                user_limit: None,
            },
            facts: BTreeMap::new(),
            mappings: BTreeMap::new(),
        }
    }

    /// 指定身份设施返回的范围，规则和候选授权仍由真实服务执行。
    /// # 参数
    /// scope 为本次测试的范围事实。
    /// # 返回
    /// 更新后的测试设施。
    /// # 错误
    /// 无。
    pub fn scope(mut self, scope: ResolvedScope) -> Self {
        self.scope = scope;
        self
    }

    /// 绑定测试实际消费的业务事实读取器。
    /// # 参数
    /// resource 为固定资源，facts 为测试事实 Port。
    /// # 返回
    /// 更新后的测试设施。
    /// # 错误
    /// 无；缺失绑定在实际 require 时保持拒绝。
    pub fn facts(mut self, resource: &str, facts: Arc<dyn AccessFactsPort>) -> Self {
        self.facts.insert(resource.into(), facts);
        self
    }

    /// 绑定实际查询编译要执行的 Mapping。
    /// # 参数
    /// permission 为具体查询入口，mapping 为测试或领域 Mapping。
    /// # 返回
    /// 更新后的测试设施。
    /// # 错误
    /// 无；未绑定查询仍由 query 拒绝。
    pub fn mapping(mut self, permission: Permission, mapping: Arc<dyn AccessMapping>) -> Self {
        self.mappings.insert(permission, mapping);
        self
    }

    /// 构造不连接数据库的测试授权服务。
    /// # 参数
    /// 无。
    /// # 返回
    /// 只含明确测试绑定的真实授权服务；禁止用于生产装配。
    /// # 错误
    /// 不执行 I/O；本工厂只在 test-support feature 下存在。
    pub fn build(self) -> Arc<AuthorizationService> {
        Arc::new(AuthorizationService {
            facilities: Arc::new(FixtureFacilities { grants: self.grants, scope: self.scope }),
            facts: self.facts,
            mappings: self.mappings,
            token: Arc::new(()),
        })
    }
}

struct FixtureFacilities {
    grants: Vec<(String, Vec<Permission>)>,
    scope: ResolvedScope,
}

#[async_trait]
impl Facilities for FixtureFacilities {
    async fn permissions(
        &self,
        _: &AuditActor,
        requested: &[Permission],
        _: &mut dyn Executor,
    ) -> Result<Option<QualifiedPermissions>> {
        let grants = self
            .grants
            .iter()
            .map(|(role, permissions)| {
                (
                    role.as_str(),
                    permissions.iter().filter(|p| requested.contains(p)).cloned().collect::<Vec<_>>(),
                )
            })
            .collect::<Vec<_>>();
        let borrowed =
            grants.iter().map(|(role, permissions)| (*role, permissions.as_slice())).collect::<Vec<_>>();
        Ok(Some(QualifiedPermissions::test_grants(&borrowed)))
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
                resource: context.resource.clone(),
                action: context.permission.action().into(),
                scope: self.scope.clone(),
                organizations: OrganizationState::default(),
                policy_version: 1,
                scope_version: "fixture-v1".into(),
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

/// 将真实 query 生成的凭证收紧为已到期，以确定性验证消费与交付拒绝。
/// # 参数
/// access 为已由实际服务编译的查询结果。
/// # 返回
/// 无；只缩短有效期，不制造或扩大授权。
/// # 错误
/// 无；仅 test-support 可调用。
pub fn expire_access(access: &mut CompiledAccess) {
    access.proof.valid_until = Some(Instant::from_unix_secs(1));
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_permissions::sales_inquiry;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;
    use crate::repository::authorization::testing::mapping;

    #[tokio::test]
    async fn isolated_fixture_executes_real_query_and_preserves_missing_binding_denial_and_expiry() {
        let permission = sales_inquiry::QUERY_SALES_LIST;
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let service = AuthorizationFixture::new(&[("reader", std::slice::from_ref(&permission))])
            .mapping(permission.clone(), mapping(&permission))
            .build();
        let mut executor = SnapshotExecutor::default();
        let mut access = service.query(&actor, &permission, &mut executor).await.unwrap();
        assert!(!access.stages().is_empty());
        assert!(access.has_role_scope());
        expire_access(&mut access);
        assert!(matches!(service.validate(&access, &mut executor).await,
            Err(Error::ConflictError(message)) if message.starts_with("DATA_SCOPE_CHANGED")));
        let missing = AuthorizationFixture::new(&[("reader", std::slice::from_ref(&permission))]).build();
        assert!(matches!(missing.query(&actor, &permission, &mut executor).await,
            Err(Error::ValidationError(message)) if message == "缺少查询 Mapping 绑定"));
        let denied = AuthorizationFixture::new(&[]).mapping(permission.clone(), mapping(&permission)).build();
        assert!(matches!(denied.query(&actor, &permission, &mut executor).await, Err(Error::Forbidden(_))));
    }
}
