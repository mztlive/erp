//! 只在原事务的一次读取阶段复用公共授权事实，范围和同角色证明仍逐请求执行。

use std::collections::BTreeSet;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use super::{AuthorizedDataScope, DataScopeFacts, DataScopeService};
use crate::access_control::ResolvedScope;
use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
use crate::entity::access_control::consumer_spec::{ConsumerSpec, OrganizationFacts};
use crate::entity::fingerprint::Fingerprint;
use crate::entity::organization_change::OrganizationState;
use crate::entity::role::Role;
use crate::repository::prelude::*;
use crate::service::access_control::consumers::scope_registration;
use crate::{AccessControlExt, Error, Permission, Result, RoleId, RolePermissionSnapshot};

/// 一次事务读取阶段的范围解析批次；持有原执行器以阻止夹入业务写入。
pub(crate) struct DataScopeBatch<'a> {
    service: &'a DataScopeService,
    actor: &'a AuditActor,
    executor: &'a mut dyn Executor,
    permissions: Vec<Permission>,
    facts: Option<IdentityFacts>,
    organizations: Option<OrganizationState>,
    directory: Option<OrganizationState>,
    reuse: bool,
    revalidate: bool,
    as_of: Instant,
}

/// 账号、权限与启用角色在原事务中的共同事实，不携带任何业务范围。
struct IdentityFacts {
    account_version: u64,
    snapshot: RolePermissionSnapshot,
    roles: Vec<Role>,
    checked_roles: BTreeSet<RoleId>,
    as_of: Instant,
}

impl<'a> DataScopeBatch<'a> {
    /// 绑定原执行器；只允许事务执行器复用公共读取。
    /// # 参数
    /// 当前解析器、身份、已声明权限与原执行器。
    /// # 返回
    /// 尚未执行 I/O 的读取批次。
    /// # 错误
    /// 无；持久化和资格错误由使用批次的方法返回。
    pub(super) fn new(
        service: &'a DataScopeService,
        actor: &'a AuditActor,
        permissions: &[Permission],
        executor: &'a mut dyn Executor,
    ) -> Self {
        let reuse = executor.session().is_some();
        Self {
            service,
            actor,
            executor,
            permissions: permissions.to_vec(),
            facts: None,
            organizations: None,
            directory: None,
            reuse,
            revalidate: false,
            as_of: Instant::now(),
        }
    }

    /// 统一授权阶段固定事实解释时点，必须在首次取证前调用。
    pub(super) fn at(mut self, at: Instant) -> Self {
        self.as_of = at;
        self
    }

    /// 保留调用顺序和各动作失败顺序，只去重同阶段公共事实。
    pub(super) async fn resolve_boundary(
        &mut self,
        entry_permission: &Permission,
        source_permission: &Permission,
        permissions: &[Permission],
    ) -> Result<AuthorizedDataScope> {
        let target = (entry_permission.resource(), entry_permission.action());
        let source = (source_permission.resource(), source_permission.action());
        scope_registration(source.0, source.1)?;
        let mut required = permissions.to_vec();
        required.push(entry_permission.clone());
        self.ensure_declared(&required)?;
        self.load_identity().await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let ids = facts.snapshot.granting_role_ids_for_all(&required);
        self.load_roles(&ids).await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let roles = qualified_roles(&facts.roles, &ids);
        if roles.is_empty() {
            return Err(Error::Forbidden(facts.snapshot.denial_message(&required)));
        }
        let at = facts.as_of;
        let state = if AuthorizationPolicy::for_action(source.0, source.1)? == AuthorizationPolicy::Shared {
            OrganizationState::default()
        } else {
            self.scope_organizations(source.0, at).await?
        };
        let scope = self.service.resolved(self.actor.id(), &roles, source, &state, at, self.executor).await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let scope_version = scope_version(
            self.actor.id(),
            target,
            source,
            (facts.account_version, facts.snapshot.policy_revision()),
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
                policy_version: facts.snapshot.policy_revision(),
                scope_version,
                as_of: at,
            },
        })
    }

    /// 不接受批次外动作，避免把未冻结的资格错误解释为撤权结果。
    fn ensure_declared(&self, permissions: &[Permission]) -> Result<()> {
        ensure_declared(&self.permissions, permissions)
    }

    /// 账号有效性、策略 revision 与启用角色均在调用方事务中读一次。
    async fn load_identity(&mut self) -> Result<()> {
        if self.reuse && self.facts.is_some() {
            if self.revalidate {
                let revision = self.service.rbac.current_policy_revision().await?;
                self.service.rbac.ensure_policy_snapshot_with_executor(revision, self.executor).await?;
                self.revalidate = false;
            }
            return Ok(());
        }
        let account = self
            .service
            .db
            .accounts()
            .find_by_id(self.actor.id(), self.executor)
            .await?
            .filter(|account| account.is_authorization_actor(self.actor.kind()))
            .ok_or_else(|| Error::Forbidden("账号已失效".into()))?;
        let snapshot = self
            .service
            .rbac
            .role_permission_snapshot(self.actor.kind(), self.actor.id(), &self.permissions)
            .await?;
        self.service
            .rbac
            .ensure_policy_snapshot_with_executor(snapshot.policy_revision(), self.executor)
            .await?;
        self.facts = Some(IdentityFacts {
            account_version: account.base.version,
            snapshot,
            roles: Vec::new(),
            checked_roles: BTreeSet::new(),
            as_of: self.as_of,
        });
        if !self.reuse {
            self.organizations = None;
            self.directory = None;
        }
        Ok(())
    }

    /// 首次需要某角色时才查询；未启用及不存在的角色在原事务中也只检查一次。
    async fn load_roles(&mut self, ids: &[RoleId]) -> Result<()> {
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let missing = ids.iter().filter(|id| !facts.checked_roles.contains(*id)).cloned().collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(());
        }
        let roles = self.service.db.roles().enabled_roles(&missing, self.executor).await?;
        let facts = self.facts.as_mut().expect("公共身份事实已成功加载");
        facts.checked_roles.extend(missing);
        facts.roles.extend(roles);
        facts.roles.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        Ok(())
    }

    /// 保留目录窄查询与普通业务完整组织事实的区别，不混用两种状态。
    async fn scope_organizations(&mut self, resource: &str, at: Instant) -> Result<OrganizationState> {
        let directory = ConsumerSpec::get(resource)?.organization_facts() == OrganizationFacts::Actor;
        let current = if directory { &self.directory } else { &self.organizations };
        if let Some(state) = current {
            return Ok(state.clone());
        }
        let state = self.service.scope_organizations(self.actor.id(), resource, at, self.executor).await?;
        let current = if directory { &mut self.directory } else { &mut self.organizations };
        *current = Some(state.clone());
        Ok(state)
    }
}

/// 声明必须包含实际请求，未冻结的动作不能被当作已检查后撤权。
fn ensure_declared(declared: &[Permission], required: &[Permission]) -> Result<()> {
    if required.iter().all(|permission| declared.contains(permission)) {
        Ok(())
    } else {
        Err(Error::ValidationError("批量授权未声明所需操作权限".into()))
    }
}

/// 只保留本次动作由同一角色完整证明的集合，保持仓储返回顺序和角色版本。
fn qualified_roles(roles: &[Role], granting: &[RoleId]) -> Vec<Role> {
    roles.iter().filter(|role| granting.iter().any(|id| id.as_str() == role.base.id)).cloned().collect()
}

/// 每个来源仍单独生成指纹；不同目标、来源、账号、角色或组织版本不得合并。
/// # 参数
/// 身份、目标和来源、账号与策略版本，以及经本动作证明的角色、组织和范围。
/// # 返回
/// v2 稳定编码摘要；保留跨页版本比较合同，旧摘要触发重新分页。
/// # 错误
/// 无。
pub(super) fn scope_version(
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RoleId;
    use crate::access_control::ScopeClause;
    use crate::entity::access_control::governance::root_scope;
    use crate::entity::role::{ROOT_ROLE_ID, RoleData};

    /// 本批每个目标和附加动作必须明确冻结；空要求、重复声明不扩大资格。
    #[test]
    fn batch_rejects_unfrozen_permission_before_resolving_scope() {
        let list = Permission::parse("stock_adjustment:list").unwrap();
        let detail = Permission::parse("stock_adjustment:detail").unwrap();
        let create = Permission::parse("stock_adjustment:create").unwrap();
        let declared = [list.clone(), detail.clone(), list.clone()];
        assert!(ensure_declared(&declared, &[]).is_ok());
        assert!(ensure_declared(&declared, &[list, detail]).is_ok());
        assert!(matches!(ensure_declared(&declared, &[create]), Err(Error::ValidationError(_))));
    }

    /// 不因本批其他动作有超管角色而把当前动作未通过的角色加入其范围。
    #[test]
    fn requested_roles_do_not_acquire_unrelated_root_scope() {
        let root =
            Role::new(RoleId::parse(ROOT_ROLE_ID).unwrap(), RoleData::new("超级管理员").with_system(true))
                .unwrap();
        let ordinary = Role::new(RoleId::parse("role-sales").unwrap(), RoleData::new("销售")).unwrap();
        let roles = vec![root, ordinary];
        let ordinary_scope = qualified_roles(&roles, &[RoleId::parse("role-sales").unwrap()]);
        assert_eq!(ordinary_scope.len(), 1);
        assert_eq!(ordinary_scope[0].base.id, "role-sales");
        assert!(root_scope(&ordinary_scope).is_none());
        assert!(qualified_roles(&roles, &[RoleId::parse("disabled-or-missing").unwrap()]).is_empty());
        assert!(root_scope(&qualified_roles(&roles, &[RoleId::parse(ROOT_ROLE_ID).unwrap()])).is_some());
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
