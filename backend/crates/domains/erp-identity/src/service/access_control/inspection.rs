//! 只读身份检查；先证明检查人的公司配置读取边界，再解析目标账号。
use std::result::Result as StdResult;
use std::slice;

use application_core::AuditActor;
use mongodb::Database;
use persistence_core::Executor;

use crate::dto::inspection::{AccessInspectionRequest, AccessInspectionView};
use crate::entity::access_control::authorization_policy::AuthorizationPolicy;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::service::access_control::resolve::{AuthorizedDataScope, DataScopeService};
use crate::{AccessControlExt, Error, Permission, Result, RoleRepositoryExt, SharedRbacService};

/// 为跨域单据检查提供经授权的目标身份；不能作为登录或操作令牌。
pub struct InspectedAccess {
    pub actor: Option<AuditActor>,
    pub view: AccessInspectionView,
}

/// 复用真实 DataScope 解析，不维护另一套鉴权算法。
#[derive(Clone)]
pub struct AccessInspectionService {
    db: Database,
    rbac: SharedRbacService,
}

impl AccessInspectionService {
    /// 绑定身份数据与权限服务。
    /// # 参数
    /// * `db` - 身份数据库。
    /// * `rbac` - 现有 RBAC 服务。
    /// # 返回
    /// 无 I/O 的检查服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 在调用方读取事务内检查账号和操作范围。
    /// # 参数
    /// * `operator` - 发起检查的管理员。
    /// * `request` - 目标账号与业务动作。
    /// * `executor` - 当前读取事务。
    /// # 返回
    /// 服务端解析结果或明确的账号／操作拒绝原因。
    /// # 错误
    /// 检查人越权、参数错误、数据库或配置错误继续向上传递。
    pub async fn inspect(
        &self,
        operator: &AuditActor,
        request: &AccessInspectionRequest,
        executor: &mut dyn Executor,
    ) -> Result<InspectedAccess> {
        request.validate()?;
        self.authorize(operator, executor).await?;
        let account = self
            .db
            .accounts()
            .find_by_id(&request.user_id, executor)
            .await?
            .filter(|account| account.is_active_backoffice());
        let Some(account) = account else {
            return Ok(denied("账号", "账号不存在或未启用，请核对人员账号状态。"));
        };
        let actor = AuditActor::new(account.base.id.clone(), account.secret.account().into(), account.kind);
        let policy = AuthorizationPolicy::for_action(&request.resource, &request.action)?;
        if !policy.configurable() {
            return self.inspect_contextual(actor, request, policy, executor).await;
        }
        let result = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve(&actor, &request.resource, &request.action, executor)
            .await;
        match result {
            Ok(access) => {
                let mut view = AccessInspectionView::from_access(&access);
                self.explain_relations(&actor, &access, &mut view, executor).await?;
                Ok(InspectedAccess { view, actor: Some(actor) })
            },
            Err(Error::Forbidden(_)) => {
                Ok(denied("操作权限", "没有启用角色提供本操作的完整权限，请检查人员角色及操作权限。"))
            },
            Err(error) => Err(error),
        }
    }

    /// 来源与任务授权只验证动作资格；缺少对象上下文时不解析旧范围冒充结论。
    async fn inspect_contextual(
        &self,
        actor: AuditActor,
        request: &AccessInspectionRequest,
        policy: AuthorizationPolicy,
        executor: &mut dyn Executor,
    ) -> Result<InspectedAccess> {
        let permission = Permission::parse(format!("{}:{}", request.resource, request.action))?;
        let snapshot = self
            .rbac
            .role_permission_snapshot(actor.kind(), actor.id(), slice::from_ref(&permission))
            .await?;
        self.rbac.ensure_policy_snapshot_with_executor(snapshot.policy_revision(), executor).await?;
        let ids = snapshot.granting_role_ids(&permission);
        if self.db.roles().enabled_roles(&ids, executor).await?.is_empty() {
            return Ok(denied("操作权限", "没有启用角色提供本操作权限，请检查人员角色及操作权限。"));
        }
        Ok(InspectedAccess { actor: None, view: AccessInspectionView::contextual(policy) })
    }

    /// 读取本操作的合格角色规则，为缺失部门关系提供可操作原因。
    async fn explain_relations(
        &self,
        actor: &AuditActor,
        access: &AuthorizedDataScope,
        view: &mut AccessInspectionView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let configs = self
            .db
            .person_data_scopes()
            .for_person(actor.id(), Some(&access.resource), Some(&access.action), executor)
            .await?;
        if configs.is_empty() && access.scope.has_role_scope() {
            view.push(
                "人员数据范围",
                "passed",
                "本操作已按服务端基础范围生效；新增其他范围需在人员资料中添加授权。",
            );
        } else if configs.is_empty() {
            view.push(
                "人员数据范围",
                "blocked",
                "本操作没有本人基础范围，且尚未添加授权范围；请进入人员资料设置。",
            );
        } else {
            view.push(
                "人员数据范围",
                "passed",
                "已按此人、本业务、本操作的基础政策及已保存范围检查；部门关系不授予操作权限。",
            );
        }
        Ok(())
    }

    /// 检查能力要求同角色提供账号、角色、范围及公司组织读取边界。
    async fn authorize(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        let permissions = ["admin:list", "role:list", "data_scope:list"]
            .into_iter()
            .map(Permission::parse)
            .collect::<StdResult<Vec<_>, _>>()?;
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "org_unit", "list", &permissions, executor)
            .await?;
        if !access.scope.role_clauses.iter().any(|scope| scope.company)
            || access.scope.user_limit.as_ref().is_some_and(|limit| !limit.company)
        {
            return Err(Error::Forbidden(
                "检查他人权限需要公司范围的组织读取及账号、角色、数据范围读取权限".into(),
            ));
        }
        Ok(())
    }
}

/// 拒绝时不生成可供对象读取使用的目标身份。
fn denied(layer: &str, message: &str) -> InspectedAccess {
    let mut view = AccessInspectionView::default();
    view.push(layer, "blocked", message);
    InspectedAccess { actor: None, view }
}
