//! DataScope v2 唯一应用解析入口，复用现有 RBAC 版本和同角色权限证明。

use std::future::Future;

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

/// 服务端判定数据访问或生成检查报告时使用的一次范围解析结果。
pub(crate) struct AuthorizedDataScope {
    /// 本动作的有效范围条款和个人上限，供对象判定或查询使用。
    pub scope: ResolvedScope,
    /// 按动作需求加载的组织事实，供同次授权判断组织关系。
    pub organizations: OrganizationState,
    /// 本次角色资格取证使用的策略修订号。
    pub policy_version: u64,
    /// 绑定本次授权依赖的稳定摘要，供复验和跨页比较使用。
    pub scope_version: String,
    /// 解析组织关系和范围时采用的授权时点。
    pub as_of: Instant,
}

/// 统一授权完成资格取证后使用的范围解析器，只访问身份域数据。
#[derive(Clone)]
pub(crate) struct DataScopeService {
    // 解析范围时读取组织关系和人员配置的身份数据库。
    db: Database,
}

impl DataScopeService {
    /// 统一授权取证后，按固定范围来源解析目标动作的范围和版本。
    /// 只使用各自具备全部必需权限的角色，沿用调用方的读取阶段和授权时点加载所需事实。
    /// 范围来源无效、没有合格角色或读取失败时返回错误。
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
            scope,
            organizations: state,
            policy_version: qualified.policy_revision(),
            scope_version,
            as_of: context.as_of,
        })
    }

    /// 统一授权需要解析数据范围时，用身份数据库构造解析器；完整资格在解析时传入。
    pub(crate) fn new(db: Database) -> Self {
        Self { db }
    }

    // 解析范围前按动作加载组织事实：管理或工作项可能需要全员关系，普通对象按需读取操作人的组织目录；无组织依赖时直接返回空状态。
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

    // 取得同角色完整资格后，按固定来源动作和原授权时点解析范围。
    // 需要人员范围时读取已保存的配置；未配置时按登记采用本人基础或返回拒绝范围。
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

// 范围解析完成后生成 v2 稳定摘要，绑定身份、目标与来源动作、账号和策略版本、合格角色、组织及范围。
// 跨页读取时比较摘要，版本不符时重新分页。
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

// 解析已取证账号的范围时，先确认至少有一个有效的合格角色。
// 岗位共享直接采用固定公司范围，其他动作再判断超管或人员范围。
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

// 解析非共享动作的范围时，有效内建超管直接取得公司范围；其他角色读取人员配置，读取失败时原样返回错误。
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

    // 验证岗位共享列表跳过旧人员限制，也无需主属部门；销售单列表仍保留人员范围的拒绝和读取错误。
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

    // 验证岗位共享仍要求有效的动作角色，资格为空或角色已停用、删除时均拒绝。
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

    // 验证有效内建超管跳过人员配置，缺省、旧撤权或读取故障均不影响其固定公司范围。
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

    // 验证普通角色即使与超管同名，也按人员配置处理 admin 的本人范围、撤权和读取错误。
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

    // 验证内建超管停用或删除后，解析采用人员范围的拒绝结果。
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

    // 固定范围摘要样例，并验证身份、目标与来源动作、授权版本或有效范围变化会让旧游标失效。
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

    // 验证角色和范围条款重排时摘要保持稳定，设置个人上限或改变上限维度时摘要随之变化。
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
