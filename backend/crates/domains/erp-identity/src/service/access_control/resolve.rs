//! DataScope v2 唯一应用解析入口，复用现有 RBAC 版本和同角色权限证明。

mod batch;
use std::collections::BTreeSet;
use std::future::Future;
use std::ops::Deref;

use application_core::AuditActor;
pub use batch::DataScopeBatch;
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
pub struct AuthorizedDataScope {
    pub(crate) facts: DataScopeFacts,
}

/// 授权解析器产生的只读事实；构造事实本身不授予授权资格。
pub struct DataScopeFacts {
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub scope: ResolvedScope,
    /// 已通过同角色完整动作权限证明的角色集合。
    pub qualified_roles: BTreeSet<String>,
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

impl AuthorizedDataScope {
    /// 消费授权结果，将只读事实移交给业务 Port 的范围转换。
    /// # 参数
    /// self 为当前解析结果。
    /// # 返回
    /// 拥有的数据事实；不能反向构造授权结果。
    /// # 错误
    /// 不返回错误。
    pub fn into_facts(self) -> DataScopeFacts {
        self.facts
    }

    /// 组合业务事实与当前读取版本，沿用消费方已有的版本编码合同。
    /// # 参数
    /// derive 接收当前版本，必须将原版本与业务事实一并纳入结果。
    /// # 返回
    /// 更新组合读取版本；身份、范围、策略版本和角色资格保持不变。
    /// # 错误
    /// 不返回错误；该版本用于读取复验，不能代替当前授权重验。
    pub fn map_read_version(&mut self, derive: impl FnOnce(&str) -> String) {
        self.facts.scope_version = derive(&self.facts.scope_version);
    }
}

#[cfg(any(test, feature = "test-support"))]
impl AuthorizedDataScope {
    /// 为行为测试构造授权夹具，生产依赖不得启用 test-support。
    /// # 参数
    /// facts 为测试声明的身份与范围。
    /// # 返回
    /// 仅用于测试的授权上下文。
    /// # 错误
    /// 不执行 I/O。
    pub fn test_fixture(facts: DataScopeFacts) -> Self {
        Self { facts }
    }

    /// 调整行为测试夹具。
    /// # 参数
    /// 无。
    /// # 返回
    /// 夹具的可变事实。
    /// # 错误
    /// 不执行 I/O。
    pub fn test_facts_mut(&mut self) -> &mut DataScopeFacts {
        &mut self.facts
    }
}

/// 身份域应用解析器，不持有其他业务域实体或数据库集合。
#[derive(Clone)]
pub struct DataScopeService {
    db: Database,
    rbac: SharedRbacService,
}

impl DataScopeService {
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
    pub fn batch<'a>(
        &'a self,
        actor: &'a AuditActor,
        permissions: &[Permission],
        executor: &'a mut dyn Executor,
    ) -> DataScopeBatch<'a> {
        DataScopeBatch::new(self, actor, permissions, executor)
    }
    /// 读取混合任务队列的身份授权版本，不授予任何资源访问权。
    ///
    /// # 参数
    /// * `actor` - 已认证身份。
    /// * `executor` - 调用方执行器。
    /// # 返回
    /// 账号、启用角色、策略、组织及当前有效关系共同形成的版本。
    /// # 错误
    /// 账号失效、策略快照漂移或持久化失败时拒绝。
    pub async fn authorization_version(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<String> {
        self.batch(actor, &[], executor).authorization_version().await
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
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 在调用方事务内证明账号、角色、动作并解析范围。
    ///
    /// # 参数
    /// * `actor` - 当前账号。
    /// * `resource` - 目标资源。
    /// * `action` - 目标动作。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回该资源动作的授权范围。
    ///
    /// # 错误
    /// 无动作权限返回 Forbidden；版本变化拒绝；缺少范围正常返回空授权结果。
    pub async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        let source_permission = AuthorizationPolicy::scope_source(&permission)?;
        self.resolve_boundary(actor, &permission, &source_permission, &[], executor).await
    }

    /// 同一角色必须满足全部权限，范围仍只绑定目标资源动作。
    ///
    /// # 参数
    /// * `actor` - 当前账号。
    /// * `resource` - 目标资源。
    /// * `action` - 范围所绑定的目标动作。
    /// * `permissions` - 同一角色必须同时具备的权限。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 返回目标资源动作上的授权范围。
    ///
    /// # 错误
    /// 未注册资源、账号失效、完整权限缺失或事务版本不一致均拒绝。
    pub async fn resolve_permissions(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let entry_permission = Permission::parse(format!("{resource}:{action}"))?;
        let source_permission = AuthorizationPolicy::scope_source(&entry_permission)?;
        self.resolve_boundary(actor, &entry_permission, &source_permission, permissions, executor).await
    }

    /// 证明目标动作权限后读取真实来源范围，不要求来源动作权限。
    /// # 参数
    /// `actor` 为当前账号；`entry_permission` 为入口功能权限；`source_permission` 只选择来源范围；`executor` 为调用事务。
    /// # 返回
    /// 保留目标动作身份、采用来源人员范围的授权上下文。
    /// # 错误
    /// 任一资源未接线、账号或目标权限失效、版本变化时拒绝。
    pub async fn resolve_source_scope(
        &self,
        actor: &AuditActor,
        entry_permission: &Permission,
        source_permission: &Permission,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        self.resolve_boundary(actor, entry_permission, source_permission, &[], executor).await
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
    ) -> Result<(ResolvedScope, BTreeSet<String>)> {
        let (resource, action) = resource_action;
        let registration = super::consumers::registration(resource, action)?;
        let qualifications = roles.iter().map(|role| role.base.id.clone()).collect();
        let policy = AuthorizationPolicy::for_action(resource, action)?;
        let scope = resolve_policy_scope(policy, roles, async {
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
        .await?;
        // 此映射只证明同角色完整动作资格，不以全公司范围替代审批角色或节点责任。
        Ok((scope, qualifications))
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

/// 有效期边界没有写入也会改变授权，必须纳入跨页版本。
fn active_relations(state: &OrganizationState, at: Instant) -> Vec<&str> {
    let mut members = state
        .memberships
        .iter()
        .filter(|item| !item.base.is_deleted() && item.validity.contains(at))
        .map(|item| item.base.id.as_str())
        .collect::<Vec<_>>();
    members.sort();
    members
}

/// 只有完成接线的资源可配置 v2，禁止将新规则交给旧解释器。
///
/// # 参数
/// * `resource` - 业务资源
/// * `action` - 已注册动作
///
/// # 返回
/// 已接线时成功。
///
/// # 错误
/// 未接线资源或动作返回校验错误。
///
/// # 关键业务约束
/// 准入必须引用真实消费者登记，不得使用初始化清单。
pub(super) fn ensure_resource(resource: &str, action: &str) -> Result<()> {
    super::consumers::registration(resource, action)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;

    use super::*;
    use crate::RoleId;
    use crate::access_control::ScopedObject;
    use crate::entity::organization::{OrgManagementAssignment, OrgMembership, OrgValidity};
    use crate::entity::role::{ROOT_ROLE_ID, RoleData};
    use crate::service::access_control::consumers::registration;

    /// 绑定业务事实只改变读取版本，不扩大身份、角色资格或原始范围。
    #[test]
    fn business_versions_preserve_authorization_evidence() {
        let context = |version: &str| {
            AuthorizedDataScope::test_fixture(DataScopeFacts {
                user_id: "alice".into(),
                resource: "sales_order".into(),
                action: "list".into(),
                scope: PersonDataScope::denied(),
                qualified_roles: BTreeSet::from(["sales".into()]),
                organizations: OrganizationState::default(),
                policy_version: 3,
                scope_version: version.into(),
                as_of: Instant::from_unix_secs(10),
            })
        };
        let mut first = context("identity-v1");
        first.map_read_version(|base| format!("{base}:business-v1"));
        let mut replay = context("identity-v1");
        replay.map_read_version(|base| format!("{base}:business-v1"));
        assert_eq!(first.scope_version, replay.scope_version);
        let mut changed = context("identity-v2");
        changed.map_read_version(|base| format!("{base}:business-v1"));
        assert_ne!(first.scope_version, changed.scope_version);
        replay.map_read_version(|base| format!("{base}:business-v2"));
        assert_ne!(first.scope_version, replay.scope_version);
        assert_eq!(first.user_id, "alice");
        assert_eq!(first.resource, "sales_order");
        assert_eq!(first.action, "list");
        assert_eq!(first.policy_version, 3);
        assert_eq!(first.qualified_roles, BTreeSet::from(["sales".into()]));
        assert_eq!(first.scope, PersonDataScope::denied());
    }

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

    /// 只有主属关系的生效边界影响范围版本，旧管理关系不再激活授权。
    #[test]
    fn authorization_relations_ignore_historical_management() {
        let mut state = OrganizationState::default();
        state.memberships.push(OrgMembership {
            base: BaseModel::new("membership".into()),
            user_id: "alice".into(),
            org_unit_id: "one".into(),
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(1),
                valid_to: Some(Instant::from_unix_secs(30)),
            },
            changed_by: "admin".into(),
            reason: "调岗".into(),
        });
        state.management.push(OrgManagementAssignment {
            base: BaseModel::new("legacy".into()),
            user_id: "alice".into(),
            org_unit_id: "two".into(),
            role_id: "old".into(),
            include_descendants: true,
            validity: OrgValidity {
                valid_from: Instant::from_unix_secs(1),
                valid_to: Some(Instant::from_unix_secs(10)),
            },
            granted_by: "admin".into(),
            reason: "历史".into(),
        });
        assert_eq!(active_relations(&state, Instant::from_unix_secs(5)), vec!["membership"]);
        assert_eq!(active_relations(&state, Instant::from_unix_secs(15)), vec!["membership"]);
        assert!(active_relations(&state, Instant::from_unix_secs(30)).is_empty());
    }
}
