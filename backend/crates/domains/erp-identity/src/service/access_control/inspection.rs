//! 只读身份检查；先证明检查人的公司配置读取边界，再解析目标账号。
use std::sync::Arc;

use application_core::AuditActor;
use erp_permissions::access_control;
use mongodb::Database;
use persistence_core::Executor;

use crate::dto::inspection::{AccessInspectionRequest, AccessInspectionView};
use crate::entity::access_control::scope_spec;
use crate::repository::access_control::person_scope::PersonDataScopeRepositoryExt;
use crate::service::access_control::resolve::AuthorizedDataScope;
use crate::{AccessControlExt, AccessTarget, AuthorizationService, Error, Permission, Result};

/// 只读身份检查的报告和后续单据检查所需的上下文；不能用作登录或操作凭证。
pub struct InspectedAccess {
    /// 已解析动作范围且请求指定单据时，提供目标账号身份，供调用方继续检查单据资格。
    pub actor: Option<AuditActor>,
    /// 已完成的检查结论及待核验的业务条件，供管理员查看。
    pub view: AccessInspectionView,
}

/// 管理员检查账号的操作资格和数据范围时，复用统一授权的范围解析。
#[derive(Clone)]
pub struct AccessInspectionService {
    // 检查账号状态和人员范围配置时使用的身份数据库。
    db: Database,
    // 应用注入的共享授权服务，用于核验检查人资格和解析目标账号权限。
    authorization: Arc<AuthorizationService>,
}

impl AccessInspectionService {
    /// 准备执行只读检查时，用身份数据库和应用的共享授权服务构造检查器；构造时不读取数据库。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 管理员检查指定账号和动作时，在调用方的读取事务内先核验检查资格，再检查目标账号及其操作范围。
    /// 目标账号无效或缺少动作资格时返回原因报告；检查人越权、参数错误、数据库或其他配置错误直接返回。
    pub async fn inspect(
        &self,
        operator: &AuditActor,
        request: &AccessInspectionRequest,
        executor: &mut dyn Executor,
    ) -> Result<InspectedAccess> {
        request.validate()?;
        self.authorization
            .require(operator, &access_control::INSPECTION_INSPECT, AccessTarget::Operation, executor)
            .await?;

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
        let permission = Permission::parse(format!("{}:{}", request.resource, request.action))?;
        match self.authorization.inspection_scope(&actor, &permission, executor).await {
            Ok(access) => {
                let mut result = inspected(actor.clone(), request, access.as_ref())?;
                if let Some(access) = access {
                    self.explain_relations(&actor, &permission, &access, &mut result.view, executor).await?;
                }
                Ok(result)
            },
            Err(Error::Forbidden(_)) => {
                Ok(denied("操作权限", "没有启用角色提供本操作的完整权限，请检查人员角色及操作权限。"))
            },
            Err(error) => Err(error),
        }
    }

    // 动作范围解析后，仅对可配置来源说明人员配置、基础范围或缺少范围的情况。
    // 部门关系本身不授予操作权限。
    async fn explain_relations(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        access: &AuthorizedDataScope,
        view: &mut AccessInspectionView,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let source = scope_spec::runtime_source(scope_spec::require_scope(permission)?)?;
        if !source.configurable() {
            return Ok(());
        }
        let source = source.permission();
        let configs = self
            .db
            .person_data_scopes()
            .for_person(actor.id(), Some(source.resource()), Some(source.action()), executor)
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
                "已按本操作的范围来源、基础政策及已保存范围检查；部门关系不授予操作权限。",
            );
        }
        Ok(())
    }
}

// 动作资格通过后组装已核对的配置和范围报告。缺少对象事实时将对象资格保留为待核验；已解析范围且指定单据时交给调用方继续检查。
fn inspected(
    actor: AuditActor,
    request: &AccessInspectionRequest,
    access: Option<&AuthorizedDataScope>,
) -> Result<InspectedAccess> {
    let mut view = match access {
        Some(access) => AccessInspectionView::from_access(access),
        None => AccessInspectionView::without_object(&request.resource, &request.action)?,
    };
    if access.is_some() && request.object_id.is_none() {
        view.push(
            "对象上下文",
            "review",
            "尚未检查具体对象；来源关系、单据责任、状态及任务资格仍须在实际业务中核验。",
        );
    }
    Ok(InspectedAccess { actor: access.and(request.object_id.as_ref()).map(|_| actor), view })
}

// 目标账号无效或动作资格不足时，生成该层的拒绝报告并留空目标身份。
fn denied(layer: &str, message: &str) -> InspectedAccess {
    let mut view = AccessInspectionView::default();
    view.push(layer, "blocked", message);
    InspectedAccess { actor: None, view }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::entity::access_control::{ResolvedScope, ScopeClause};
    use crate::entity::organization_change::OrganizationState;

    // 验证已解析范围时，无单据的对象检查仍待核验；指定单据后仅提供身份，单据范围由调用方检查。
    #[test]
    fn scoped_configuration_preserves_review_until_object_is_checked() {
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let mut request = AccessInspectionRequest {
            user_id: "user".into(),
            resource: "sales_order".into(),
            action: "detail".into(),
            object_id: None,
        };
        let access = AuthorizedDataScope {
            scope: ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            organizations: OrganizationState::default(),
            policy_version: 1,
            scope_version: "current".into(),
            as_of: Instant::from_unix_secs(100),
        };

        let result = inspected(actor.clone(), &request, Some(&access)).unwrap();
        assert!(result.actor.is_none());
        assert_eq!(result.view.scope_version.as_deref(), Some("current"));
        assert!(result.view.steps.iter().any(|step| step.layer == "对象上下文" && step.status == "review"));

        request.object_id = Some("order".into());
        let result = inspected(actor, &request, Some(&access)).unwrap();
        assert!(result.actor.is_some());
        assert!(result.view.steps.iter().all(|step| step.layer != "具体单据范围"));
    }

    // 验证依赖来源或任务事实的动作只报告已通过的操作资格，其余步骤待核验，目标身份留空。
    #[test]
    fn contextual_configuration_requires_object_and_task_facts() {
        let actor = AuditActor::new("user".into(), "login".into(), AccountKind::Admin);
        let request = AccessInspectionRequest {
            user_id: "user".into(),
            resource: "approval_instance".into(),
            action: "decide".into(),
            object_id: None,
        };
        let result = inspected(actor, &request, None).unwrap();
        assert!(result.actor.is_none());
        assert!(result.view.scope_version.is_none());
        assert_eq!(result.view.steps[0].status, "passed");
        assert!(result.view.steps[1..].iter().all(|step| step.status == "review"));
    }
}
