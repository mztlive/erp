//! W26 命令和原命令回放的当前账号、静态动作资格。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::Result as CoreResult;
use erp_identity::entity::authorization::{
    AccessContext, FactKind, FactValue, ObjectFacts, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use erp_workflow::entity::work_item::required_execution_permissions;
use erp_workflow::{WorkItem, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

use super::{SupplierFulfillmentProcess, W26_BUSINESS_OBJECT_TYPE};
use crate::{Error, Result};

impl SupplierFulfillmentProcess {
    /// 任务赋予指定主体的处理边界；任务写入仍在领域事务内重验责任及版本。
    ///
    /// # 参数
    /// * `actor` - 已认证的调用人
    /// * `executor` - 当前读取执行器
    /// # 返回
    /// 当前账号有效且具备 W26 完整动作权限时成功。
    /// # 错误
    /// 账号失效、角色撤权或政策快照变化时拒绝，包括原命令重放。
    pub(super) async fn require_task_permissions(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let codes = required_execution_permissions(WorkItemType::BusinessException, W26_BUSINESS_OBJECT_TYPE)
            .filter(|codes| !codes.is_empty())
            .ok_or_else(|| Error::Internal("供应商履约任务缺少完整执行权限合同".into()))?;
        let permissions = codes
            .into_iter()
            .map(|code| Permission::parse(code).map_err(Error::from))
            .collect::<Result<Vec<_>>>()?;
        let rbac = (self.identity)().rbac;
        let qualified = AuthorizationService::new(self.db.clone(), rbac, AuthorizationRegistry::default())
            .permissions(actor, &permissions, executor)
            .await?
            .ok_or_else(|| Error::Forbidden("供应商履约任务账号已失效".into()))?;
        if !qualified.same_role(&permissions) {
            return Err(Error::Forbidden("当前账号不具备供应商履约任务所需的完整执行权限".into()));
        }
        Ok(())
    }
}

struct TaskFacts(WorkItem);
#[async_trait]
impl AccessFactsPort for TaskFacts {
    async fn load(
        &self,
        context: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        Ok(vec![ObjectFacts {
            resource: context.resource.clone(),
            id: self.0.base.id.clone(),
            version: self.0.base.version.to_string(),
            relations: [(
                RelationKey::parse("work_item")?,
                RelatedObjects {
                    expected_ids: [self.0.base.id.clone()].into(),
                    objects: vec![ObjectFacts {
                        resource: "work_item".into(),
                        id: self.0.base.id.clone(),
                        version: self.0.base.version.to_string(),
                        assignee_id: FactValue::Known(self.0.owner_user_id.clone()),
                        ..Default::default()
                    }],
                },
            )]
            .into(),
            ..Default::default()
        }])
    }
}

/// W26 当前任务指定人与完整执行资格由同一次 identity 检查证明。
/// # 参数
/// db、identity 为原业务设施；actor、task、executor 为原办理上下文。
/// # 返回
/// 当前任务指定人与完整角色资格都成立时成功。
/// # 错误
/// 任一身份或任务资格失效时拒绝，设施错误透传。
pub(crate) async fn require_assignee(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    let codes = required_execution_permissions(WorkItemType::BusinessException, W26_BUSINESS_OBJECT_TYPE)
        .expect("固定W26执行权限合同存在");
    let required = codes.into_iter().map(Permission::parse).collect::<CoreResult<Vec<_>>>()?;
    let permission = Permission::parse("supplier_fulfillment_order:investigate")?;
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: required,
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Source(RelationKey::parse("work_item")?),
                required_facts: Default::default(),
                policy: AccessPolicy::Related {
                    relation: RelationKey::parse("work_item")?,
                    resource: "work_item".into(),
                    mode: RelationMatch::AllNonEmpty,
                    required_facts: [FactKind::Assignee].into(),
                    policy: Box::new(AccessPolicy::Assigned),
                },
            },
            facts: Arc::new(TaskFacts(task.clone())),
        },
    }])?;
    Ok(AuthorizationService::new(db.clone(), identity.clone(), registry)
        .require(
            actor,
            &permission,
            AccessTarget::Records {
                subject: AccessSubject::Source(RelationKey::parse("work_item")?),
                ids: ResourceTargets::new(&[task.base.id.as_str()])?,
            },
            executor,
        )
        .await?)
}
