//! 付款跟进沿原提交资格和真实关联任务的分配事实授权。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactKind, FactValue, ObjectFacts, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets,
    Result as IdentityResult, SharedRbacService,
};
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

/// 以原付款提交入口核验关联工作项的真实分配，不增加工作项菜单权限。
/// # 参数
/// db、rbac 为身份设施；actor、task 为原事务中的真实账号和任务，executor 沿用原事务。
/// # 返回
/// 当前账号具备原入口资格且被分配该任务时成功。
/// # 错误
/// 账号、入口权限、关联任务或分配失效时返回 identity 原错误。
pub(super) async fn require_assigned(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = Permission::parse("purchase_payment_request:submit")?;
    let relation = RelationKey::parse("work_item")?;
    let subject = AccessSubject::Source(relation.clone());
    let registration = AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![],
        access: AccessMode::Object {
            data: DataAccess {
                subject: subject.clone(),
                required_facts: Default::default(),
                policy: AccessPolicy::Related {
                    relation,
                    resource: "work_item".into(),
                    mode: RelationMatch::AllNonEmpty,
                    required_facts: [FactKind::Assignee].into(),
                    policy: Box::new(AccessPolicy::Assigned),
                },
            },
            facts: Arc::new(FollowUpFacts { db: db.clone(), task_id: task.base.id.clone() }),
        },
    };
    AuthorizationService::new(db.clone(), rbac.clone(), AuthorizationRegistry::new(vec![registration])?)
        .require(
            actor,
            &permission,
            AccessTarget::Records { subject, ids: ResourceTargets::new(&[&task.business_object_id])? },
            executor,
        )
        .await?;
    Ok(())
}

struct FollowUpFacts {
    db: Database,
    task_id: String,
}

#[async_trait]
impl AccessFactsPort for FollowUpFacts {
    async fn load(
        &self,
        context: &AccessContext,
        _: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let Some(task) = self.db.work_items().find_by_id(&self.task_id, executor).await? else {
            return Ok(vec![]);
        };
        let relation = RelationKey::parse("work_item")?;
        let source = ObjectFacts {
            resource: "work_item".into(),
            id: task.base.id.clone(),
            version: task.base.version.to_string(),
            assignee_id: FactValue::Known(task.owner_user_id),
            ..Default::default()
        };
        Ok(vec![ObjectFacts {
            resource: context.resource.clone(),
            id: task.business_object_id,
            version: task.base.version.to_string(),
            relations: [(
                relation,
                RelatedObjects { expected_ids: [task.base.id].into(), objects: vec![source] },
            )]
            .into(),
            ..Default::default()
        }])
    }
}
