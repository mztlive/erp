//! 付款任务及其原命令结果的固定指定人政策。

use std::collections::BTreeMap;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactKind, FactValue, ObjectFacts, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets,
};
use erp_workflow::WorkItem;
use mongodb::Database;
use persistence_core::Executor;

use crate::{FinancePostingPorts, Result};

struct TaskFacts {
    item: WorkItem,
    result: Option<ObjectFacts>,
}

#[async_trait]
impl AccessFactsPort for TaskFacts {
    async fn load(
        &self,
        context: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let mut relations = [(
            RelationKey::parse("work_item")?,
            RelatedObjects {
                expected_ids: [self.item.base.id.clone()].into(),
                objects: vec![ObjectFacts {
                    resource: "work_item".into(),
                    id: self.item.base.id.clone(),
                    version: self.item.base.version.to_string(),
                    assignee_id: FactValue::Known(self.item.owner_user_id.clone()),
                    ..Default::default()
                }],
            },
        )]
        .into_iter()
        .collect::<BTreeMap<_, _>>();
        if let Some(result) = &self.result {
            relations.insert(
                RelationKey::parse("supplier_payment")?,
                RelatedObjects { expected_ids: [result.id.clone()].into(), objects: vec![result.clone()] },
            );
        }
        Ok(vec![ObjectFacts {
            resource: context.resource.clone(),
            id: self.item.base.id.clone(),
            version: self.item.base.version.to_string(),
            relations,
            ..Default::default()
        }])
    }
}

/// 固定动作不接受客户端提供的权限或政策。
pub(crate) enum TaskPurpose {
    Payment,
    Reveal,
    Invoice,
    PaymentResult { id: String, version: String, owner: String },
}

impl TaskPurpose {
    fn parts(self) -> (&'static str, Option<ObjectFacts>) {
        match self {
            TaskPurpose::Payment => ("purchase_payment_request:pay", None),
            TaskPurpose::PaymentResult { id, version, owner } => (
                "purchase_payment_request:pay",
                Some(ObjectFacts {
                    resource: "supplier_payment".into(),
                    id,
                    version,
                    owner_id: FactValue::Known(Some(owner)),
                    ..Default::default()
                }),
            ),
            TaskPurpose::Reveal => ("party_bank_account:reveal", None),
            TaskPurpose::Invoice => ("invoice:post", None),
        }
    }
}

/// 当前指定人和原结果创建人由 identity 判断；终态任务不丢失原负责人事实。
/// # 参数
/// db、ports 为原业务设施；task 为同执行器已读任务，purpose 为固定动作和原付款结果所有者。
/// # 返回
/// 当前功能、任务分配及可选原结果所有者均成立时成功。
/// # 错误
/// 任一身份或对象资格不足时拒绝；设施读取错误透传。
pub(crate) async fn require_task(
    db: &Database,
    ports: &FinancePostingPorts,
    actor: &AuditActor,
    task: &WorkItem,
    purpose: TaskPurpose,
    executor: &mut dyn Executor,
) -> Result<()> {
    let entry = registration(task, purpose)?;
    let permission = entry.permission.clone();
    let registry = AuthorizationRegistry::new(vec![entry])?;
    Ok(AuthorizationService::new(db.clone(), ports.identity.clone(), registry)
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

fn registration(task: &WorkItem, purpose: TaskPurpose) -> erp_identity::Result<AccessRegistration> {
    let (entry, result) = purpose.parts();
    let permission = Permission::parse(entry)?;
    let required_permissions = if entry == "purchase_payment_request:pay" {
        vec![Permission::parse("purchase_payment_request:detail")?]
    } else {
        vec![]
    };
    let assigned = AccessPolicy::Related {
        relation: RelationKey::parse("work_item")?,
        resource: "work_item".into(),
        mode: RelationMatch::AllNonEmpty,
        required_facts: [FactKind::Assignee].into(),
        policy: Box::new(AccessPolicy::Assigned),
    };
    let policy = if result.is_some() {
        AccessPolicy::All(vec![
            assigned,
            AccessPolicy::Related {
                relation: RelationKey::parse("supplier_payment")?,
                resource: "supplier_payment".into(),
                mode: RelationMatch::AllNonEmpty,
                required_facts: [FactKind::Owner].into(),
                policy: Box::new(AccessPolicy::Owned),
            },
        ])
    } else {
        assigned
    };
    Ok(AccessRegistration {
        permission,
        required_permissions,
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Source(RelationKey::parse("work_item")?),
                required_facts: Default::default(),
                policy,
            },
            facts: Arc::new(TaskFacts { item: task.clone(), result }),
        },
    })
}
