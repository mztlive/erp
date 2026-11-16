//! DataScope v2 唯一应用解析入口，复用现有 RBAC 版本和同角色权限证明。

use std::future::Future;
use std::ops::Deref;

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::access_control::{ResolvedScope, ScopeClause};
use crate::entity::access_control::governance::root_scope;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::access_control::scope_spec::{self, OrganizationLoad, ScopeSource};
use crate::entity::authorization::AccessContext;
use crate::entity::fingerprint::Fingerprint;
use crate::entity::organization_change::OrganizationState;
use crate::entity::role::Role;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, Permission, QualifiedPermissions, Result};

/// 服务端授权上下文；原始范围和组织事实不直接序列化给客户端。
pub(crate) struct AuthorizedDataScope {
    pub(crate) facts: DataScopeFacts,
}

/// 授权解析器产生的事实数据；构造事实本身不授予授权资格。
/// 消费者可按查询需要补充组织关系，不将其视为不可变的原始授权快照。
pub(crate) struct DataScopeFacts {
    pub resource: String,
    pub action: String,
    pub scope: ResolvedScope,
    pub organizations: OrganizationState,
    pub policy_version: u64,
    pub scope_version: String,
    pub as_of: Instant,
}

impl Deref for AuthorizedDataScope {
    type Target = DataScopeFacts;

    /// 只读访问本次解析事实；不提供可变解引用。
    fn deref(&self) -> &Self::Target {
        &self.facts
    }
}

#[cfg(test)]
impl AuthorizedDataScope {
    /// 为行为测试构造授权夹具，生产依赖不得启用 test-support。
    /// # 参数
    /// facts 为测试声明的身份与范围。
    /// # 返回
    /// 仅用于测试的授权上下文。
    /// # 错误
    /// 不执行 I/O。
    pub(crate) fn test_fixture(facts: DataScopeFacts) -> Self {
        Self { facts }
    }
}

/// 身份域应用解析器，不持有其他业务域实体或数据库集合。
#[derive(Clone)]
pub(crate) struct DataScopeService {
    db: Database,
}

impl DataScopeService {
    /// 使用统一授权已取得的完整资格，只读取本次实际需要的范围设施。
    /// # 参数
    /// context 为目标动作上下文，source 为固定范围来源；required、qualified 为本次完整资格。
    /// executor 沿用调用方读取阶段。
    /// # 返回
    /// 仅由本动作合格角色产生的范围与依赖版本。
    /// # 错误
    /// 来源无效、完整资格不足或范围读取失败时返回错误。
    pub(crate) async fn qualified(
        &self,
        context: &AccessContext,
        source: &Permission,
        required: &[Permission],
        qualified: &QualifiedPermissions,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let spec = scope_spec::require_runtime(source)?;
        let ids = qualified.roles_for_all(required);
        let roles = qualified
            .roles
            .iter()
            .filter(|role| ids.iter().any(|id| id.as_str() == role.base.id))
            .cloned()
            .collect::<Vec<_>>();
        if roles.is_empty() {
            return Err(Error::Forbidden("没有启用角色提供本操作的完整权限".into()));
        }
        let load = scope_spec::organization_load(spec);
        let state = self.scope_organizations(context.actor.id(), load, context.as_of, executor).await?;
        let target = (context.permission.resource(), context.permission.action());
        let origin = (source.resource(), source.action());
        let scope =
            self.resolved(context.actor.id(), &roles, origin, &state, context.as_of, executor).await?;
        let scope_version = scope_version(
            context.actor.id(),
            target,
            origin,
            (qualified.account_version, qualified.policy_revision()),
            &roles,
            &state,
            &scope,
        );
        Ok(AuthorizedDataScope {
            facts: DataScopeFacts {
                resource: target.0.into(),
                action: target.1.into(),
                scope,
                organizations: state,
                policy_version: qualified.policy_revision(),
                scope_version,
                as_of: context.as_of,
            },
        })
    }

    /// 绑定身份数据库；完整资格由统一授权服务传入。
    ///
    /// # 参数
    /// * `db` - 身份数据库。
    ///
    /// # 返回
    /// 返回资源动作范围解析器。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    /// 管理及工作项需要全员事实，其他授权只装载组织树及操作人关系。
    async fn scope_organizations(
        &self,
        actor_id: &str,
        load: OrganizationLoad,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationState> {
        let repository = OrganizationRepository::new(&self.db);
        match load {
            OrganizationLoad::AllMembers => Ok(repository.state(executor).await?),
            OrganizationLoad::Actor => repository.directory_state(actor_id, at, executor).await,
            OrganizationLoad::None => Ok(OrganizationState::default()),
        }
    }

    /// 批量取得主体规则，保留同角色证据；未来生效和到期关系按同一时点解释。
    async fn resolved(
        &self,
        user: &str,
        roles: &[Role],
        resource_action: (&str, &str),
        state: &OrganizationState,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedScope> {
        let (resource, action) = resource_action;
        let spec = scope_spec::require_runtime_action(resource, action)?;
        resolve_configured_scope(spec, roles, async {
            state.own_org(user, at)?;
            let configs =
                self.db.person_data_scopes().for_person(user, Some(resource), Some(action), executor).await?;
            match configs.first() {
                Some(config) => config.resolve(state, at),
                None if spec.default_self() => {
                    PersonDataScope::default_for(user, resource, action).resolve(state, at)
                },
                None => Ok(PersonDataScope::denied()),
            }
        })
        .await
    }
}

/// 身份、目标和来源、账号与策略版本，以及经本动作证明的角色、组织和范围。
/// # 参数
/// user、target、source 绑定身份与动作；versions、roles、state、scope 为本次授权事实。
/// # 返回
/// v2 稳定编码摘要；保留跨页版本比较合同，旧摘要触发重新分页。
/// # 错误
/// 无。
fn scope_version(
    user: &str,
    target: (&str, &str),
    source: (&str, &str),
    versions: (u64, u64),
    roles: &[Role],
    state: &OrganizationState,
    scope: &ResolvedScope,
) -> String {
    // v2 字段顺序：账号、账号/策略/组织版本、目标、来源、无序角色、范围。
    let mut hash = Fingerprint::new("shared-policy-v2");
    hash.text(user);
    hash.number(versions.0);
    hash.number(versions.1);
    hash.number(state.version);
    for value in [target.0, target.1, source.0, source.1] {
        hash.text(value);
    }
    hash.unordered(roles, |hash, role| {
        hash.text(&role.base.id);
        hash.number(role.base.version);
    });
    hash.scope(scope);
    hash.finish()
}

/// 仅在账号与同角色完整动作已取证后应用公司或人员范围；旧人员配置不再限制公司范围。
async fn resolve_configured_scope(
    spec: &scope_spec::ScopeSpec,
    qualified_roles: &[Role],
    person_scope: impl Future<Output = Result<ResolvedScope>>,
) -> Result<ResolvedScope> {
    if !qualified_roles.iter().any(|role| !role.disabled && !role.base.is_deleted()) {
        return Err(Error::Forbidden("没有启用角色提供本操作的完整权限".into()));
    }
    if matches!(spec.source(), ScopeSource::Company) {
        return Ok(ResolvedScope {
            role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
            user_limit: None,
        });
    }
    resolve_scope(qualified_roles, person_scope).await
}

/// 超管范围直接生效；其他角色才执行人员范围读取，读取失败不得退化为公司范围。
async fn resolve_scope(
    roles: &[Role],
    person_scope: impl Future<Output = Result<ResolvedScope>>,
) -> Result<ResolvedScope> {
    match root_scope(roles) {
        Some(scope) => Ok(scope),
        None => person_scope.await,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::RoleId;
    use crate::access_control::ScopedObject;
    use crate::entity::role::{ROOT_ROLE_ID, RoleData};

    /// 岗位共享不读取旧的撤权、本人或部门配置，且不会变成销售授权。
    #[tokio::test]
    async fn shared_scope_ignores_person_limits_but_sales_keeps_them() {
        let roles = [Role::new(RoleId::parse("role-buyer").unwrap(), RoleData::new("采购")).unwrap()];
        for resource in [
            "product",
            "supplier",
            "supplier_offering",
            "purchase_order",
            "supplier_fulfillment_order",
            "supplier_settlement_statement",
        ] {
            let spec = scope_spec::require_runtime_action(resource, "list").unwrap();
            let scope = resolve_configured_scope(spec, &roles, async {
                panic!("共享资源不得读取旧人员范围或要求主属部门");
            })
            .await
            .unwrap();
            assert!(scope.user_limit.is_none());
            assert!(scope.allows(
                &ScopedObject {
                    owned: false,
                    org_unit_id: Some("other-team"),
                    collaborating: false,
                    historical_read_participant: false,
                    settlement_party_id: None,
                    warehouse_id: None,
                },
                false
            ));
        }
        let sales = scope_spec::require_runtime_action("sales_order", "list").unwrap();
        let denied =
            resolve_configured_scope(sales, &roles, async { Ok(PersonDataScope::denied()) }).await.unwrap();
        assert!(!denied.has_role_scope());
        let failed = resolve_configured_scope(sales, &roles, async {
            Err(Error::Forbidden("人员范围失效".into()))
        })
        .await;
        assert!(matches!(failed, Err(Error::Forbidden(_))));
    }

    /// 共享政策不补授动作资格，失效角色和空资格均拒绝。
    #[tokio::test]
    async fn shared_scope_requires_current_qualified_role() {
        let mut disabled = Role::new(RoleId::parse("role-buyer").unwrap(), RoleData::new("采购")).unwrap();
        disabled.disabled = true;
        let mut deleted = disabled.clone();
        deleted.disabled = false;
        deleted.base.deleted_at = 1;
        for roles in [vec![], vec![disabled], vec![deleted]] {
            let result = resolve_configured_scope(
                scope_spec::require_runtime_action("product", "list").unwrap(),
                &roles,
                async { panic!("无动作资格不得读取人员范围") },
            )
            .await;
            assert!(matches!(result, Err(Error::Forbidden(_))));
        }
    }

    /// 已有超管不读人员配置；缺配置、旧撤权或读取错误均不改变固定范围。
    #[tokio::test]
    async fn root_scope_does_not_poll_person_configuration() {
        let root =
            Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超级管理员").with_system(true))
                .unwrap();
        let scope = resolve_scope(&[root], async {
            panic!("超级管理员不应读取人员范围配置");
        })
        .await
        .unwrap();
        assert_eq!(scope.role_clauses, vec![ScopeClause { company: true, ..Default::default() }]);
        assert!(scope.user_limit.is_none());
    }

    /// 普通账号即使名为 admin 也保留本人范围、撤权结果和读取错误。
    #[tokio::test]
    async fn ordinary_role_keeps_person_scope_and_errors() {
        let roles = [Role::new(RoleId::parse("role-custom").unwrap(), RoleData::new("超级管理员")).unwrap()];
        let state = OrganizationState::default();
        let at = Instant::from_unix_secs(0);
        let config = PersonDataScope::default_for("admin", "product", "create");
        let expected = config.resolve(&state, at).unwrap();
        let scope = resolve_scope(&roles, async { Ok(expected.clone()) }).await.unwrap();
        assert_eq!(scope, expected);
        assert!(!scope.role_clauses.iter().any(|clause| clause.company));
        let denied = resolve_scope(&roles, async { Ok(PersonDataScope::denied()) }).await.unwrap();
        assert_eq!(denied, PersonDataScope::denied());
        let failed = resolve_scope(&roles, async { Err(Error::Forbidden("范围读取失败".into())) }).await;
        assert!(matches!(failed, Err(Error::Forbidden(message)) if message == "范围读取失败"));
    }

    /// 内建角色失效后立即回到人员范围，不保留超管公司授权。
    #[tokio::test]
    async fn disabled_or_deleted_root_does_not_override_person_scope() {
        let mut root =
            Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超级管理员").with_system(true))
                .unwrap();
        root.disabled = true;
        let scope = resolve_scope(std::slice::from_ref(&root), async { Ok(PersonDataScope::denied()) })
            .await
            .unwrap();
        assert_eq!(scope, PersonDataScope::denied());
        root.disabled = false;
        root.base.deleted_at = 1;
        let scope = resolve_scope(&[root], async { Ok(PersonDataScope::denied()) }).await.unwrap();
        assert_eq!(scope, PersonDataScope::denied());
    }

    /// 共用公共事实仍须分别绑定每个来源和目标，所有授权版本变化均使游标失效。
    #[test]
    fn scope_versions_keep_each_source_and_all_authorization_versions() {
        let role = Role::new(RoleId::parse("role-finance").unwrap(), RoleData::new("财务")).unwrap();
        let mut roles = vec![role];
        roles[0].base.version = 0;
        let mut state = OrganizationState { version: 4, ..Default::default() };
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        };
        let target = ("invoice", "list");
        let source = ("sales_order", "list");
        let version = scope_version("finance", target, source, (2, 3), &roles, &state, &scope);
        assert_eq!(version, "fd4cfc9427c3662ca4f5922dbf4bf64c");
        assert_eq!(version, scope_version("finance", target, source, (2, 3), &roles, &state, &scope));
        for (user, changed_target, changed_source, versions) in [
            ("other", target, source, (2, 3)),
            ("finance", ("invoice", "detail"), source, (2, 3)),
            ("finance", target, ("purchase_order", "list"), (2, 3)),
            ("finance", target, source, (3, 3)),
            ("finance", target, source, (2, 4)),
        ] {
            assert_ne!(
                version,
                scope_version(user, changed_target, changed_source, versions, &roles, &state, &scope)
            );
        }
        roles[0].base.version += 1;
        assert_ne!(version, scope_version("finance", target, source, (2, 3), &roles, &state, &scope));
        roles[0].base.version -= 1;
        state.version += 1;
        assert_ne!(version, scope_version("finance", target, source, (2, 3), &roles, &state, &scope));
        state.version -= 1;
        let denied = ResolvedScope { role_clauses: vec![], user_limit: None };
        assert_ne!(version, scope_version("finance", target, source, (2, 3), &roles, &state, &denied));
    }

    /// 角色和条款顺序不改变范围版本；个人上限与条款字段变化仍使其失效。
    #[test]
    fn scope_versions_ignore_role_order_but_bind_limits_and_dimensions() {
        let roles = vec![
            Role::new(RoleId::parse("first").unwrap(), RoleData::new("甲")).unwrap(),
            Role::new(RoleId::parse("second").unwrap(), RoleData::new("乙")).unwrap(),
        ];
        let state = OrganizationState::default();
        let scope = ResolvedScope {
            role_clauses: vec![
                ScopeClause { self_owned: true, ..Default::default() },
                ScopeClause { collaborative: true, ..Default::default() },
            ],
            user_limit: None,
        };
        let hash = |roles: &[Role], scope: &ResolvedScope| {
            scope_version("u", ("invoice", "list"), ("sales_order", "list"), (1, 2), roles, &state, scope)
        };
        let original = hash(&roles, &scope);
        let mut reversed = scope.clone();
        reversed.role_clauses.reverse();
        assert_eq!(original, hash(&roles.into_iter().rev().collect::<Vec<_>>(), &reversed));
        let roles = Vec::new();
        let original = hash(&roles, &scope);
        let clauses = [
            ScopeClause::default(),
            ScopeClause { company: true, ..Default::default() },
            ScopeClause { self_owned: true, ..Default::default() },
            ScopeClause { collaborative: true, ..Default::default() },
            ScopeClause { org_unit_ids: ["org".into()].into(), ..Default::default() },
            ScopeClause { settlement_party_ids: ["party".into()].into(), ..Default::default() },
            ScopeClause { warehouse_ids: ["warehouse".into()].into(), ..Default::default() },
        ];
        let versions: BTreeSet<_> = clauses
            .into_iter()
            .map(|limit| hash(&roles, &ResolvedScope { user_limit: Some(limit), ..scope.clone() }))
            .collect();
        assert_eq!(versions.len(), 7);
        assert!(!versions.contains(&original));
    }
}
