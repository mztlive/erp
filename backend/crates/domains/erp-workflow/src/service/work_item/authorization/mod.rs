//! 工作项查询与命令共用的 identity 授权登记。
mod facts;

use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::entity::authorization::{AccessSubject, AccessTarget, FactKind, ReadAccess};
use erp_identity::ports::AccessMapping;
use erp_identity::{
    AccessPolicy, AccessRegistration, AuthorizationRegistry, AuthorizationService, CompiledAccess,
    Permission, ResourceTargets, SharedRbacService,
};
use facts::WorkItemFacts;
use mongodb::Database;
use persistence_core::Executor;

use crate::repository::work_item::authorization::WorkItemMapping;
use crate::{Error, Result};

/// 任务读取与管理共享的完整对象授权；不决定办理人业务资格。
#[derive(Clone)]
pub struct WorkItemAuthorization {
    pub(crate) service: AuthorizationService,
    mapping: Arc<dyn AccessMapping>,
}

impl WorkItemAuthorization {
    /// 绑定工作项模型、事实和 identity 服务。
    /// # 参数
    /// db 为本域数据库，rbac 为共享身份服务。
    /// # 返回
    /// 返回不执行查询的授权服务。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        let mapping = WorkItemMapping::shared();
        let service = authorization(db, rbac, mapping.clone());
        Self { service, mapping }
    }
}

/// 同一领域模型的读取、详情和管理分别证明各自动作范围。
fn authorization(
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

impl WorkItemAuthorization {
    /// 生成绑定当前事务的完整任务查询约束。
    /// # 参数
    /// actor 为真实账号，action 为登记动作，executor 沿用当前事务。
    /// # 返回
    /// 返回已经编译并验证的工作项查询凭证。
    /// # 错误
    /// 未登记动作、身份失效、范围或版本变化时拒绝。
    pub(crate) async fn query(
        &self,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<Arc<CompiledAccess>> {
        let query = self.service.query(actor, &permission(action)?, AccessSubject::Direct, executor).await?;
        let compiled = query.compile(self.mapping.as_ref(), executor).await?;
        self.service.validate(&compiled, executor).await?;
        Ok(Arc::new(compiled))
    }

    /// 以完整对象合同检查一个任务，供详情和写命令复用。
    /// # 参数
    /// actor 为真实账号，action 为 detail/manage，id 为任务 ID，executor 沿用调用事务。
    /// # 返回
    /// 入口权限和任务责任范围均成立时成功。
    /// # 错误
    /// 身份失效、无权、任务缺失或事实读取失败时返回错误。
    pub async fn require(
        &self,
        actor: &AuditActor,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.service
            .require(
                actor,
                &permission(action)?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?;
        Ok(())
    }
}
