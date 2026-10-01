//! DataScope v2 唯一应用解析入口，复用现有 RBAC 版本和同角色权限证明。

use std::collections::BTreeMap;
use std::future::Future;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use super::consumers::configurable_registration;
use crate::access_control::{ResolvedScope, ScopeClause};
use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
use crate::entity::access_control::governance::root_scope;
use crate::entity::access_control::person_scope::PersonDataScope;
use crate::entity::organization_change::OrganizationState;
use crate::entity::role::Role;
use crate::repository::OrganizationRepository;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::repository::prelude::*;
use crate::{AccessControlExt, Error, Permission, Result, SharedRbacService};

/// 服务端授权上下文；原始范围和组织事实不直接序列化给客户端。
pub struct AuthorizedDataScope {
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub scope: ResolvedScope,
    /// 已通过同角色完整动作权限证明的角色键；值不参与范围计算。
    pub role_scopes: BTreeMap<String, ScopeClause>,
    pub organizations: OrganizationState,
    pub policy_version: u64,
    pub scope_version: String,
    pub as_of: Instant,
}

/// 身份域应用解析器，不持有其他业务域实体或数据库集合。
#[derive(Clone)]
pub struct DataScopeService {
    db: Database,
    rbac: SharedRbacService,
}

impl DataScopeService {
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
        let account = self
            .db
            .accounts()
            .find_by_id(actor.id(), executor)
            .await?
            .filter(|account| account.is_active_backoffice() && account.kind == actor.kind())
            .ok_or_else(|| Error::Forbidden("账号已失效".into()))?;
        let snapshot = self.rbac.role_permission_snapshot(actor.kind(), actor.id(), &[]).await?;
        self.rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let mut roles = self.db.roles().enabled_roles(snapshot.role_ids(), executor).await?;
        roles.sort_by(|a, b| a.base.id.cmp(&b.base.id));
        let state = OrganizationRepository::new(&self.db).state(executor).await?;
        let active = active_relations(&state, Instant::now());
        Ok(format!(
            "{:x}",
            md5::compute(format!(
                "{}:{}:{}:{}:{:?}:{active:?}",
                actor.id(),
                account.base.version,
                snapshot.policy_revision(),
                state.version,
                roles.iter().map(|role| (&role.base.id, role.base.version)).collect::<Vec<_>>()
            ))
        ))
    }

    /// 绑定身份数据库及现有 RBAC 实例。
    ///
    /// # 返回
    /// 返回资源动作范围解析器。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 在调用方事务内证明账号、角色、动作并解析范围。
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
        self.resolve_permissions(actor, resource, action, &[permission], executor).await
    }

    /// 同一角色必须满足全部权限，范围仍只绑定目标资源动作。
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
        let source = AuthorizationPolicy::scope_source(resource, action);
        self.resolve_boundary(actor, (resource, action), source, permissions, executor).await
    }

    /// 证明目标动作权限后读取真实来源范围，不要求来源动作权限。
    /// # 参数
    /// `actor` 为当前账号；目标和来源分别明确资源、动作；`executor` 为调用事务。
    /// # 返回
    /// 保留目标动作身份、采用来源人员范围的授权上下文。
    /// # 错误
    /// 任一资源未接线、账号或目标权限失效、版本变化时拒绝。
    pub async fn resolve_source_scope(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        source_resource: &str,
        source_action: &str,
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        self.resolve_boundary(
            actor,
            (resource, action),
            (source_resource, source_action),
            &[permission],
            executor,
        )
        .await
    }

    /// 动作资格与数据边界在同一策略快照中独立取证。
    async fn resolve_boundary(
        &self,
        actor: &AuditActor,
        target: (&str, &str),
        source: (&str, &str),
        permissions: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<AuthorizedDataScope> {
        let (resource, action) = target;
        ensure_resource(resource, action)?;
        configurable_registration(source.0, source.1)?;
        let mut required = permissions.to_vec();
        required.push(Permission::parse(format!("{resource}:{action}"))?);
        let account = self
            .db
            .accounts()
            .find_by_id(actor.id(), executor)
            .await?
            .filter(|a| a.is_active_backoffice() && a.kind == actor.kind())
            .ok_or_else(|| Error::Forbidden("账号已失效".into()))?;
        let snapshot = self.rbac.role_permission_snapshot(actor.kind(), actor.id(), &required).await?;
        self.rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let role_ids = snapshot.granting_role_ids_for_all(&required);
        let roles = self.db.roles().enabled_roles(&role_ids, executor).await?;
        if roles.is_empty() {
            return Err(Error::Forbidden("没有该资源动作权限".into()));
        }
        let as_of = Instant::now();
        let state = self.scope_organizations(actor.id(), source.0, as_of, executor).await?;
        let (scope, role_scopes) = self.resolved(actor.id(), &roles, source, &state, as_of, executor).await?;
        let fingerprint = format!(
            "{}:{}:{}:{}:{}:{source:?}:{:?}:{:?}",
            account.base.version,
            snapshot.policy_revision(),
            state.version,
            resource,
            action,
            roles.iter().map(|r| (&r.base.id, r.base.version)).collect::<Vec<_>>(),
            scope
        );
        Ok(AuthorizedDataScope {
            user_id: actor.id().into(),
            resource: resource.into(),
            action: action.into(),
            scope,
            role_scopes,
            organizations: state,
            policy_version: snapshot.policy_revision(),
            scope_version: format!("{:x}", md5::compute(format!("{}:{fingerprint}", actor.id()).as_bytes())),
            as_of,
        })
    }

    /// 目录只装载有界组织树及操作人关系，其他资源保留既有组织事实合同。
    async fn scope_organizations(
        &self,
        actor_id: &str,
        resource: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<OrganizationState> {
        let repository = OrganizationRepository::new(&self.db);
        if matches!(
            resource,
            "sales_person"
                | "procurement_person"
                | "business_person"
                | "person_query_qualification"
                | "settlement_party"
                | "warehouse"
        ) {
            repository.directory_state(actor_id, at, executor).await
        } else {
            Ok(repository.state(executor).await?)
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
    ) -> Result<(ResolvedScope, BTreeMap<String, ScopeClause>)> {
        let (resource, action) = resource_action;
        let registration = super::consumers::registration(resource, action)?;
        let qualifications =
            roles.iter().map(|role| (role.base.id.clone(), ScopeClause::default())).collect();
        let scope = resolve_scope(roles, async {
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
    use crate::entity::organization::{OrgManagementAssignment, OrgMembership, OrgValidity};
    use crate::entity::role::{ROOT_ROLE_ID, RoleData};
    use crate::service::access_control::consumers::registration;

    /// 已有超管不读人员配置；缺配置、旧撤权或读取错误均不改变固定范围。
    #[tokio::test]
    async fn root_scope_does_not_poll_person_configuration() {
        let root = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超级管理员").with_system(true)).unwrap();
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
        let roles = [Role::new("role-custom".into(), RoleData::new("超级管理员")).unwrap()];
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
        let mut root = Role::new(ROOT_ROLE_ID.into(), RoleData::new("超级管理员").with_system(true)).unwrap();
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
