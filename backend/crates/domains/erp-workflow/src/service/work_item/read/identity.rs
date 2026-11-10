//! 任务读取和对象检查统一调用 identity，登记不接受请求覆盖。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{AccessSubject, AccessTarget, FactKind, ReadAccess};
use erp_identity::ports::AccessMapping;
use erp_identity::service::authorization::{AccessPolicy, AccessRegistration, AuthorizationRegistry};
use erp_identity::{AuthorizationService, CompiledAccess, Permission, ResourceTargets, SharedRbacService};
use mongodb::Database;
use persistence_core::Executor;

use super::WorkItemReadService;
use super::facts::WorkItemFacts;
use crate::entity::work_item::WorkItemType;
use crate::repository::work_item::authorization::WorkItemAuthorizationExt;
use crate::service::work_item::access::required_execution_permissions;
use crate::{Error, Result, WorkItemExt};

/// 同一领域模型的读取、详情和管理分别证明各自动作范围。
pub(super) fn authorization(
    db: Database,
    rbac: SharedRbacService,
    mapping: Arc<dyn AccessMapping>,
) -> AuthorizationService {
    let facts = Arc::new(WorkItemFacts(db.clone()));
    let entries = ["list", "detail", "manage"]
        .into_iter()
        .map(|action| {
            let permission = permission(action).expect("固定任务动作合法");
            AccessRegistration {
                policy: AccessPolicy::Scope { permission: permission.clone(), read: ReadAccess::default() },
                permission,
                subject: AccessSubject::Direct,
                required_permissions: vec![],
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                allow_proposed: false,
                mapping: Some(mapping.clone()),
                facts: Some(facts.clone()),
            }
        })
        .collect();
    AuthorizationService::new(db, rbac, AuthorizationRegistry::new(entries).expect("固定任务登记完整"))
}

/// 仅服务端支持的任务动作可以解析为授权入口。
fn permission(action: &str) -> Result<Permission> {
    if !matches!(action, "list" | "detail" | "manage") {
        return Err(Error::ValidationError("未登记的任务权限动作".into()));
    }
    Ok(Permission::parse(format!("work_item:{action}"))?)
}

impl WorkItemReadService {
    /// 生成绑定当前事务的完整任务查询约束。
    pub(super) async fn task_query(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<Arc<CompiledAccess>> {
        let query =
            self.authorization.query(actor, &permission(action)?, AccessSubject::Direct, executor).await?;
        let compiled = query.compile(self.mapping.as_ref(), executor).await?;
        self.authorization.validate(&compiled, executor).await?;
        Ok(Arc::new(compiled))
    }

    /// 以完整对象合同检查一个任务，供详情和写命令复用。
    /// # 参数
    /// actor 为真实账号，action 为 detail/manage，id 为任务 ID，executor 沿用调用事务。
    /// # 返回
    /// 入口权限和任务责任范围均成立时成功。
    /// # 错误
    /// 身份失效、无权、任务缺失或事实读取失败时返回错误。
    pub async fn require_task(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization
            .require(
                actor,
                &permission(action)?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?;
        Ok(())
    }

    /// 复验本次任务读取及管理动作的完整依赖。
    /// # 参数
    /// access 为本服务生成的访问上下文；executor 为原执行器。
    /// # 返回
    /// 所有已使用的权限和映射仍有效时成功。
    /// # 错误
    /// 撤权或版本变化返回 DATA_SCOPE_CHANGED，其他错误保留原类别。
    pub async fn validate_access(
        &self,
        access: &super::access::ActorAccess,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.authorization.validate(access.read_access()?, executor).await?;
        if let Some(manage) = &access.manage {
            self.authorization.validate(manage, executor).await?;
        }
        Ok(())
    }
}

impl WorkItemReadService {
    /// 按实际任务注册表检查按钮权限，通配权限仍由 identity 解释。
    pub(super) async fn task_permissions(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<Vec<Permission>> {
        let mut codes = BTreeSet::from([
            "work_item:list",
            "work_item:detail",
            "work_item:manage",
            "approval_instance:decide",
        ]);
        for relation in WorkItemType::registered_brief_relations() {
            if let Some(required) =
                required_execution_permissions(relation.work_item_type, relation.business_object_type)
            {
                codes.extend(required);
            }
        }
        let mut permissions = Vec::new();
        for code in codes {
            let permission = Permission::parse(code)?;
            if self.authorization.has_permission(actor, &permission, executor).await? {
                permissions.push(permission);
            }
        }
        Ok(permissions)
    }

    /// 只查询当前展示批次的管理资格，不装载全部可管理任务。
    pub(super) async fn projected_access(
        &self,
        access: &super::access::ActorAccess,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<super::access::ActorAccess> {
        let mut projected = access.clone();
        if let Some(manage) = &access.manage {
            self.authorization.validate(manage, executor).await?;
            projected.managed_task_ids =
                self.db.work_items().authorized_ids(ids, manage, executor).await?.into_iter().collect();
            self.authorization.validate(manage, executor).await?;
        }
        Ok(projected)
    }
}
