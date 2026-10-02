//! 只在原事务的一次读取阶段复用公共授权事实，范围和同角色证明仍逐请求执行。

use std::collections::BTreeSet;
use std::future::Future;
use std::pin::Pin;
use std::result::Result as CallbackResult;
use std::slice;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use persistence_core::Executor;

use super::{AuthorizedDataScope, DataScopeService, active_relations, ensure_resource};
use crate::access_control::ResolvedScope;
use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
use crate::entity::organization_change::OrganizationState;
use crate::entity::role::Role;
use crate::repository::prelude::*;
use crate::service::access_control::consumers::configurable_registration;
use crate::{AccessControlExt, Error, Permission, Result, RolePermissionSnapshot};

/// 一次事务读取阶段的范围解析批次；持有原执行器以阻止夹入业务写入。
pub struct DataScopeBatch<'a> {
    service: &'a DataScopeService,
    actor: &'a AuditActor,
    executor: &'a mut dyn Executor,
    permissions: Vec<Permission>,
    facts: Option<IdentityFacts>,
    organizations: Option<OrganizationState>,
    directory: Option<OrganizationState>,
    reuse: bool,
    revalidate: bool,
}

/// 账号、权限与启用角色在原事务中的共同事实，不携带任何业务范围。
struct IdentityFacts {
    account_version: u64,
    snapshot: RolePermissionSnapshot,
    roles: Vec<Role>,
    checked_roles: BTreeSet<String>,
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
        }
    }

    /// 证明同一角色的完整动作资格，再解释该业务操作的独立范围。
    /// # 参数
    /// 目标资源动作及额外必须由同一角色提供的权限。
    /// # 返回
    /// 当前目标动作的范围、版本及授权时点。
    /// # 错误
    /// 未声明权限、无完整角色资格、版本漂移或持久化失败时拒绝。
    pub async fn resolve_permissions(
        &mut self,
        resource: &str,
        action: &str,
        permissions: &[Permission],
    ) -> Result<AuthorizedDataScope> {
        let source = AuthorizationPolicy::scope_source(resource, action);
        self.resolve_boundary((resource, action), source, permissions).await
    }

    /// 目标资格独立证明，采用指定真实业务来源的人员范围。
    /// # 参数
    /// 目标及来源的资源动作；来源动作不要求调用人另有操作权限。
    /// # 返回
    /// 保留目标身份与来源范围的授权上下文。
    /// # 错误
    /// 任一登记无效、目标权限失效、版本漂移或持久化失败时拒绝。
    pub async fn resolve_source_scope(
        &mut self,
        resource: &str,
        action: &str,
        source_resource: &str,
        source_action: &str,
    ) -> Result<AuthorizedDataScope> {
        self.resolve_boundary((resource, action), (source_resource, source_action), &[]).await
    }

    /// 由本阶段当前启用角色独立证明静态动作，不产生任何业务数据范围。
    /// # 参数
    /// 已在批次中声明的操作权限。
    /// # 返回
    /// 存在实际授予该动作的启用角色时为 true。
    /// # 错误
    /// 未声明权限、账号失效、策略漂移或持久化失败时拒绝。
    pub async fn has_permission(&mut self, permission: &Permission) -> Result<bool> {
        self.ensure_declared(slice::from_ref(permission))?;
        if !self.reuse {
            let snapshot = self
                .service
                .rbac
                .role_permission_snapshot(self.actor.kind(), self.actor.id(), slice::from_ref(permission))
                .await?;
            self.service
                .rbac
                .ensure_policy_snapshot_with_executor(snapshot.policy_revision(), self.executor)
                .await?;
            let roles = self
                .service
                .db
                .roles()
                .enabled_roles(&snapshot.granting_role_ids(permission), self.executor)
                .await?;
            return Ok(!roles.is_empty());
        }
        self.load_identity().await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let ids = facts.snapshot.granting_role_ids(permission);
        self.load_roles(&ids).await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        Ok(facts.roles.iter().any(|role| ids.contains(&role.base.id)))
    }

    /// 读取本阶段的完整身份版本，不授予任何资源访问权。
    /// # 参数
    /// 使用构造时绑定的当前身份及原执行器。
    /// # 返回
    /// 账号、启用角色、策略及组织有效关系组成的版本。
    /// # 错误
    /// 账号失效、策略漂移或持久化失败时拒绝。
    pub async fn authorization_version(&mut self) -> Result<String> {
        if !self.reuse {
            return self.service.authorization_version(self.actor, self.executor).await;
        }
        self.load_identity().await?;
        let ids = self.facts.as_ref().expect("公共身份事实已成功加载").snapshot.role_ids().to_vec();
        self.load_roles(&ids).await?;
        let at = self.facts.as_ref().expect("公共身份事实已成功加载").as_of;
        let state = self.scope_organizations("work_item", at).await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let active = active_relations(&state, at);
        Ok(format!(
            "{:x}",
            md5::compute(format!(
                "{}:{}:{}:{}:{:?}:{active:?}",
                self.actor.id(),
                facts.account_version,
                facts.snapshot.policy_revision(),
                state.version,
                facts.roles.iter().map(|role| (&role.base.id, role.base.version)).collect::<Vec<_>>()
            ))
        ))
    }

    /// 在同一读取阶段借用原执行器完成其他只读事实，保持调用方已有错误顺序。
    /// # 参数
    /// `read` 只能查询事实，不得写入或开启事务。
    /// # 返回
    /// 回调的只读结果；回调结束后继续使用本批公共授权事实。
    /// # 错误
    /// 回调错误原样返回。
    ///
    /// 业务写入前必须释放本批；写入后的授权检查须创建新批次。
    pub async fn read<'b, T, E, F>(&'b mut self, read: F) -> CallbackResult<T, E>
    where
        T: Send + 'b,
        E: Send + 'b,
        F: FnOnce(&'b mut dyn Executor) -> Pin<Box<dyn Future<Output = CallbackResult<T, E>> + Send + 'b>>
            + Send,
    {
        self.revalidate = self.facts.is_some();
        read(self.executor).await
    }

    /// 保留调用顺序和各动作失败顺序，只去重同阶段公共事实。
    async fn resolve_boundary(
        &mut self,
        target: (&str, &str),
        source: (&str, &str),
        permissions: &[Permission],
    ) -> Result<AuthorizedDataScope> {
        ensure_resource(target.0, target.1)?;
        configurable_registration(source.0, source.1)?;
        let mut required = permissions.to_vec();
        required.push(Permission::parse(format!("{}:{}", target.0, target.1))?);
        self.ensure_declared(&required)?;
        if !self.reuse {
            return self
                .service
                .resolve_boundary(self.actor, target, source, permissions, self.executor)
                .await;
        }
        self.load_identity().await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let ids = facts.snapshot.granting_role_ids_for_all(&required);
        self.load_roles(&ids).await?;
        let facts = self.facts.as_ref().expect("公共身份事实已成功加载");
        let roles = qualified_roles(&facts.roles, &ids);
        if roles.is_empty() {
            return Err(Error::Forbidden("没有该资源动作权限".into()));
        }
        let at = facts.as_of;
        let state = self.scope_organizations(source.0, at).await?;
        let (scope, role_scopes) =
            self.service.resolved(self.actor.id(), &roles, source, &state, at, self.executor).await?;
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
            user_id: self.actor.id().into(),
            resource: target.0.into(),
            action: target.1.into(),
            scope,
            role_scopes,
            organizations: state,
            policy_version: facts.snapshot.policy_revision(),
            scope_version,
            as_of: at,
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
            .filter(|account| account.is_active_backoffice() && account.kind == self.actor.kind())
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
            as_of: Instant::now(),
        });
        if !self.reuse {
            self.organizations = None;
            self.directory = None;
        }
        Ok(())
    }

    /// 首次需要某角色时才查询；未启用及不存在的角色在原事务中也只检查一次。
    async fn load_roles(&mut self, ids: &[String]) -> Result<()> {
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
        let directory = matches!(
            resource,
            "sales_person"
                | "procurement_person"
                | "business_person"
                | "person_query_qualification"
                | "settlement_party"
                | "warehouse"
        );
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
fn qualified_roles(roles: &[Role], granting: &[String]) -> Vec<Role> {
    roles.iter().filter(|role| granting.contains(&role.base.id)).cloned().collect()
}

/// 每个来源仍单独生成指纹；不同目标、来源、账号、角色或组织版本不得合并。
/// # 参数
/// 身份、目标和来源、账号与策略版本，以及经本动作证明的角色、组织和范围。
/// # 返回
/// 保持既有跨页合同格式的摘要。
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
    let (account_version, policy_revision) = versions;
    let fingerprint = format!(
        "{account_version}:{policy_revision}:{}:{}:{}:{source:?}:{:?}:{scope:?}",
        state.version,
        target.0,
        target.1,
        roles.iter().map(|role| (&role.base.id, role.base.version)).collect::<Vec<_>>(),
    );
    format!("{:x}", md5::compute(format!("{user}:{fingerprint}").as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
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
        let root = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超级管理员").with_system(true)).unwrap();
        let ordinary = Role::new("role-sales".into(), RoleData::new("销售")).unwrap();
        let roles = vec![root, ordinary];
        let ordinary_scope = qualified_roles(&roles, &["role-sales".into()]);
        assert_eq!(ordinary_scope.len(), 1);
        assert_eq!(ordinary_scope[0].base.id, "role-sales");
        assert!(root_scope(&ordinary_scope).is_none());
        assert!(qualified_roles(&roles, &["disabled-or-missing".into()]).is_empty());
        assert!(root_scope(&qualified_roles(&roles, &[ROOT_ROLE_ID.into()])).is_some());
    }

    /// 共用公共事实仍须分别绑定每个来源和目标，所有授权版本变化均使游标失效。
    #[test]
    fn scope_versions_keep_each_source_and_all_authorization_versions() {
        let role = Role::new("role-finance".into(), RoleData::new("财务")).unwrap();
        let mut roles = vec![role];
        let mut state = OrganizationState { version: 4, ..Default::default() };
        let scope = ResolvedScope {
            role_clauses: vec![ScopeClause { self_owned: true, ..Default::default() }],
            user_limit: None,
        };
        let target = ("invoice", "list");
        let source = ("sales_order", "list");
        let version = scope_version("finance", target, source, (2, 3), &roles, &state, &scope);
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
}
