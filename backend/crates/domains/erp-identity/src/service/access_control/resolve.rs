//! DataScope v2 唯一应用解析入口，复用现有 RBAC 版本和同角色权限证明。

mod batch;
use std::future::Future;
use std::ops::Deref;

use application_core::AuditActor;
use batch::DataScopeBatch;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::access_control::{ResolvedScope, ScopeClause};
use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
use crate::entity::access_control::governance::root_scope;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::organization_change::OrganizationState;
use crate::entity::role::Role;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::{AccessControlExt, Error, Permission, Result, SharedRbacService};

/// 服务端授权上下文；原始范围和组织事实不直接序列化给客户端。
pub(crate) struct AuthorizedDataScope {
    pub(crate) facts: DataScopeFacts,
}

/// 授权解析器产生的只读事实；构造事实本身不授予授权资格。
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
    rbac: SharedRbacService,
}

impl DataScopeService {
    /// 统一目录已验证入口，按同一时点解释范围并证明同角色完整权限。
    pub(crate) async fn registered(
        &self,
        actor: &AuditActor,
        entry: &Permission,
        source: &Permission,
        required: &[Permission],
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let mut permissions = required.to_vec();
        permissions.push(entry.clone());
        self.batch(actor, &permissions, executor).at(at).resolve_boundary(entry, source, required).await
    }
    /// 在一次调用方事务中复用身份和组织事实，各动作分别解析真实范围。
    ///
    /// # 参数
    /// `actor` 为当前身份，`permissions` 声明本批所有目标及额外动作，`executor` 为原执行器。
    /// # 返回
    /// 借用原执行器的解析批次；非事务执行器保持逐次重新取证。
    /// # 错误
    /// 构造不执行 I/O；各解析方法分别报告资格、版本及持久化错误。
    ///
    /// 批次只能用于同一读取阶段；释放批次后才允许原执行器执行业务写入。
    /// 最终授权重验和下一读取阶段必须创建新的批次。
    pub(crate) fn batch<'a>(
        &'a self,
        actor: &'a AuditActor,
        permissions: &[Permission],
        executor: &'a mut dyn Executor,
    ) -> DataScopeBatch<'a> {
        DataScopeBatch::new(self, actor, permissions, executor)
    }
    /// 绑定身份数据库及现有 RBAC 实例。
    ///
    /// # 参数
    /// * `db` - 身份数据库。
    /// * `rbac` - 现有 RBAC 服务。
    ///
    /// # 返回
    /// 返回资源动作范围解析器。
    ///
    /// # 错误
    /// 不返回错误。
    pub(crate) fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 在调用方事务内证明账号、角色、动作并解析范围。
    ///
    /// # 参数
    /// * `actor` - 当前账号。
    /// * `entry_permission` - 已解析的目标资源动作。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回该资源动作的授权范围。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；版本变化拒绝；缺少范围正常返回空授权结果。
    pub(crate) async fn resolve(
        &self,
        actor: &AuditActor,
        entry_permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        super::consumers::registration(entry_permission.resource(), entry_permission.action())?;
        let source_permission = AuthorizationPolicy::scope_source(entry_permission)?;
        self.resolve_boundary(actor, entry_permission, &source_permission, &[], executor).await
    }

    /// 同一角色必须满足全部权限，范围仍只绑定目标资源动作。
    ///
    /// # 参数
    /// * `actor` - 当前账号。
    /// * `entry_permission` - 已解析的目标资源动作。
    /// * `permissions` - 同一角色必须同时具备的权限。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回目标资源动作上的授权范围。
    ///
    /// # 错误
    /// 未注册资源、账号失效、完整权限缺失或事务版本不一致均拒绝。
    pub(crate) async fn resolve_permissions(
        &self,
        actor: &AuditActor,
        entry_permission: &Permission,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        super::consumers::registration(entry_permission.resource(), entry_permission.action())?;
        let source_permission = AuthorizationPolicy::scope_source(entry_permission)?;
        self.resolve_boundary(actor, entry_permission, &source_permission, permissions, executor).await
    }

    /// 动作资格与数据边界在同一策略快照中独立取证。
    async fn resolve_boundary(
        &self,
        actor: &AuditActor,
        entry_permission: &Permission,
        source_permission: &Permission,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let mut required = permissions.to_vec();
        required.push(entry_permission.clone());
        self.batch(actor, &required, executor)
            .resolve_boundary(entry_permission, source_permission, permissions)
            .await
    }

    /// 管理及工作项需要全员事实，其他授权只装载组织树及操作人关系。
    async fn scope_organizations(
        &self,
        actor_id: &str,
        resource: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationState> {
        let repository = OrganizationRepository::new(&self.db);
        if AuthorizationPolicy::needs_all_members(resource) {
            Ok(repository.state(executor).await?)
        } else {
            repository.directory_state(actor_id, at, executor).await
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
        let registration = super::consumers::registration(resource, action)?;
        let policy = AuthorizationPolicy::for_action(resource, action)?;
        resolve_policy_scope(policy, roles, async {
            state.own_org(user, at)?;
            let configs =
                self.db.person_data_scopes().for_person(user, Some(resource), Some(action), executor).await?;
            match configs.first() {
                Some(config) => {
                    config.resolve(state, registration.required_dimensions, registration.allows_history, at)
                },
                None if registration.default_self => PersonDataScope::default_for(user, resource, action)
                    .resolve(state, registration.required_dimensions, registration.allows_history, at),
                None => Ok(PersonDataScope::denied()),
            }
        })
        .await
    }
}

/// 仅在账号与同角色完整动作已取证后应用共享政策；旧人员配置不再限制共享业务。
async fn resolve_policy_scope(
    policy: AuthorizationPolicy,
    qualified_roles: &[Role],
    person_scope: impl Future<Output = Result<ResolvedScope>>,
) -> Result<ResolvedScope> {
    if !qualified_roles.iter().any(|role| !role.disabled && !role.base.is_deleted()) {
        return Err(Error::Forbidden("没有启用角色提供本操作的完整权限".into()));
    }
    if policy == AuthorizationPolicy::Shared {
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

    use super::*;
    use crate::RoleId;
    use crate::access_control::ScopedObject;
    use crate::entity::role::{ROOT_ROLE_ID, RoleData};
    use crate::service::access_control::consumers::registration;

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
            let policy = AuthorizationPolicy::for_action(resource, "list").unwrap();
            let scope = resolve_policy_scope(policy, &roles, async {
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
        let sales = AuthorizationPolicy::for_action("sales_order", "list").unwrap();
        let denied =
            resolve_policy_scope(sales, &roles, async { Ok(PersonDataScope::denied()) }).await.unwrap();
        assert!(!denied.has_role_scope());
        let failed =
            resolve_policy_scope(sales, &roles, async { Err(Error::Forbidden("人员范围失效".into())) }).await;
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
            let result = resolve_policy_scope(AuthorizationPolicy::Shared, &roles, async {
                panic!("无动作资格不得读取人员范围");
            })
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
        let registration = registration("product", "create").unwrap();
        let config = PersonDataScope::default_for("admin", "product", "create");
        let expected = config
            .resolve(&state, registration.required_dimensions, registration.allows_history, at)
            .unwrap();
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
}
