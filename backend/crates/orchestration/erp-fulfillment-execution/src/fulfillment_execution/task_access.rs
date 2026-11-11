//! 客户验收办理按完整执行权限及指定任务人统一授权。

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
use erp_workflow::{WorkItem, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

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

pub(super) enum TaskPurpose {
    CustomerAcceptance,
    Fulfillment,
}

/// 身份资格和当前任务分配共用调用方已经读取的可信事实。
/// # 参数
/// db、identity 为原业务设施；actor、task、executor 为当前办理上下文。
/// # 返回
/// 同角色完整执行资格与任务分配均成立时成功。
/// # 错误
/// 账号、角色或任务分配不足时拒绝；设施错误透传。
pub(super) async fn require_assignee(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    task: &WorkItem,
    purpose: TaskPurpose,
    executor: &mut dyn Executor,
) -> Result<()> {
    let codes = match purpose {
        TaskPurpose::CustomerAcceptance => WorkItemType::CustomerAcceptanceRegistration
            .customer_acceptance_execution_permissions("sales_order"),
        TaskPurpose::Fulfillment => {
            WorkItemType::FulfillmentOperation.fulfillment_execution_permissions(&task.business_object_type)
        },
    }
    .ok_or_else(|| crate::Error::Internal("履约任务缺少完整执行权限合同".into()))?;
    let required = codes.iter().map(Permission::parse).collect::<CoreResult<Vec<_>>>()?;
    let permission = required.first().expect("固定任务权限合同非空").clone();
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
