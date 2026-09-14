//! DataScope v2 唯一应用解析入口，复用现有 RBAC 版本和同角色权限证明。

use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::access_control::{DataScopeSubjectType, ResolvedScope, ScopeDimension, ScopeResolution};
use crate::entity::organization::OrgTree;
use crate::entity::organization_change::OrganizationState;
use crate::repository::OrganizationRepository;
use crate::{AccessControlExt, Error, Permission, Result, SharedRbacService};
use application_core::AuditActor;

/// 服务端授权上下文；原始范围和组织事实不直接序列化给客户端。
pub struct AuthorizedDataScope {
    pub user_id: String,
    pub resource: String,
    pub action: String,
    pub scope: ResolvedScope,
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
        self.resolve_permissions(actor, resource, action, &[permission], executor)
            .await
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
        ensure_resource(resource, action)?;
        let mut required = permissions.to_vec();
        required.push(Permission::parse(format!("{resource}:{action}"))?);
        let account = self
            .db
            .accounts()
            .find_by_id(actor.id(), executor)
            .await?
            .filter(|a| a.is_active_backoffice())
            .ok_or_else(|| Error::Forbidden("账号已失效".into()))?;
        let snapshot = self
            .rbac
            .role_permission_snapshot(actor.kind(), actor.id(), &required)
            .await?;
        self.rbac
            .ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor)
            .await?;
        let role_ids = snapshot.granting_role_ids_for_all(&required);
        let roles = self.db.roles().enabled_roles(&role_ids, executor).await?;
        let eligible = roles.iter().map(|role| role.base.id.clone()).collect::<Vec<_>>();
        if eligible.is_empty() {
            return Err(Error::Forbidden("没有该资源动作权限".into()));
        }
        let state = OrganizationRepository::new(&self.db).state(executor).await?;
        let as_of = Instant::now();
        let scope = self
            .resolved(actor.id(), &eligible, (resource, action), &state, as_of, executor)
            .await?;
        let fingerprint = format!(
            "{}:{}:{}:{}:{}:{:?}:{:?}",
            account.base.version,
            snapshot.policy_revision(),
            state.version,
            resource,
            action,
            roles
                .iter()
                .map(|r| (&r.base.id, r.base.version))
                .collect::<Vec<_>>(),
            scope
        );
        Ok(AuthorizedDataScope {
            user_id: actor.id().into(),
            resource: resource.into(),
            action: action.into(),
            scope,
            organizations: state,
            policy_version: snapshot.policy_revision(),
            scope_version: format!(
                "{:x}",
                md5::compute(format!("{}:{fingerprint}", actor.id()).as_bytes())
            ),
            as_of,
        })
    }

    /// 批量取得主体规则，保留同角色证据；未来生效和到期关系按同一时点解释。
    async fn resolved(
        &self,
        user: &str,
        roles: &[String],
        resource_action: (&str, &str),
        state: &OrganizationState,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<ResolvedScope> {
        let (resource, action) = resource_action;
        let mut rules = self
            .db
            .data_scopes()
            .list_by_subjects(DataScopeSubjectType::Role, roles, executor)
            .await?;
        rules.extend(
            self.db
                .data_scopes()
                .list_by_subject(DataScopeSubjectType::User, user, executor)
                .await?,
        );
        for rule in rules.iter().filter(|rule| rule.binding.applies(resource, action)) {
            if rule.binding.target_dimension != ScopeDimension::InternalOrg {
                return Err(Error::ValidationError("该资源仅接受内部组织范围".into()));
            }
        }
        state.own_org(user, at)?;
        let tree = OrgTree::new(&state.units)?;
        let registration = super::consumers::registration(resource, action)?;
        ScopeResolution {
            user_id: user,
            eligible_role_ids: roles,
            resource,
            action,
            required_dimensions: registration.required_dimensions,
            rules: &rules,
            memberships: &state.memberships,
            management: &state.management,
            tree: &tree,
            as_of: at,
        }
        .resolve()
    }
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
