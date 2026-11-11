//! 供给分配沿采购创建入口和真实任务分配事实授权。

mod query;
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
use erp_workflow::entity::work_item::WorkItem;
use mongodb::Database;
use persistence_core::Executor;
pub(crate) use query::creation_tasks;

use crate::Result;

/// 在原业务执行器中检查采购创建权限与当前任务分配。
///
/// # 参数
/// `db`、`rbac` 为权威身份设施，`actor` 为真实身份；`task` 为服务端读取的任务，
/// `executor` 为读取任务和执行业务操作的原执行器。
///
/// # 返回
/// identity 确认入口权限与任务分配均有效时成功。
///
/// # 错误
/// 身份、功能权限、任务分配或身份设施失效时返回原错误。
/// 任务状态、来源版本和冻结责任范围由调用用例校验。
pub async fn require_creation_task(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    task: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = Permission::parse("purchase_order:create")?;
    let registry = AuthorizationRegistry::new(vec![registration(task, permission.clone())])?;
    AuthorizationService::new(db.clone(), rbac.clone(), registry)
        .require(
            actor,
            &permission,
            AccessTarget::Records { subject: data().subject, ids: ResourceTargets::new(&[&task.base.id])? },
            executor,
        )
        .await?;
    Ok(())
}

fn registration(task: &WorkItem, permission: Permission) -> AccessRegistration {
    let task_facts = ObjectFacts {
        resource: "work_item".into(),
        id: task.base.id.clone(),
        version: task.base.version.to_string(),
        assignee_id: FactValue::Known(task.owner_user_id.clone()),
        ..Default::default()
    };
    let facts = ObjectFacts {
        resource: permission.resource().into(),
        id: task.base.id.clone(),
        version: task.base.version.to_string(),
        relations: [(
            relation(),
            RelatedObjects { expected_ids: [task.base.id.clone()].into(), objects: vec![task_facts] },
        )]
        .into(),
        ..Default::default()
    };
    AccessRegistration {
        permission,
        required_permissions: vec![],
        access: AccessMode::Object { data: data(), facts: Arc::new(CreationTaskFacts(facts)) },
    }
}

/// 返回对象与查询共同使用的采购创建来源关系。
/// # 参数
/// 无。
/// # 返回
/// 服务端固定的来源关系身份。
/// # 错误
/// 固定关系码合法，不返回错误。
pub(in crate::purchase_center) fn relation() -> RelationKey {
    RelationKey::parse("creation_task").expect("固定采购创建来源关系合法")
}

/// 为对象检查与查询登记同一份真实任务分配政策。
/// # 参数
/// 无。
/// # 返回
/// 采购创建入口沿真实任务来源检查当前分配事实。
/// # 错误
/// 固定政策构造不执行 I/O。
pub(in crate::purchase_center) fn data() -> DataAccess {
    DataAccess {
        subject: AccessSubject::Source(relation()),
        required_facts: Default::default(),
        policy: AccessPolicy::Related {
            relation: relation(),
            resource: "work_item".into(),
            mode: RelationMatch::AllNonEmpty,
            required_facts: [FactKind::Assignee].into(),
            policy: Box::new(AccessPolicy::Assigned),
        },
    }
}

struct CreationTaskFacts(ObjectFacts);

#[async_trait]
impl AccessFactsPort for CreationTaskFacts {
    async fn load(
        &self,
        _: &AccessContext,
        _: &ResourceTargets,
        _: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        Ok(vec![self.0.clone()])
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_workflow::entity::work_item::{AssignmentSource, WorkItemData, WorkItemPriority, WorkItemType};
    use persistence_core::NoTransaction;

    use super::*;

    fn task() -> WorkItem {
        WorkItem::new_with_responsibility_scope(
            WorkItemId::new("task"),
            WorkItemData {
                work_item_type: WorkItemType::ProcurementOrderCreation,
                business_object_type: "sales_order".into(),
                business_object_id: "sales".into(),
                subject_version: "revision".into(),
                owner_role: "role-procurement".into(),
                owner_organization_id: "company".into(),
                owner_user_id: "buyer".into(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: None,
            },
            "scope",
            vec!["line".into()],
        )
        .unwrap()
    }

    #[tokio::test]
    async fn creation_policy_uses_current_assignee_without_historical_owner_fallback() {
        let actor = AuditActor::new("buyer".into(), "buyer".into(), AccountKind::Admin);
        let permission = Permission::parse("purchase_order:create").unwrap();
        let context = AccessContext {
            actor,
            permission: permission.clone(),
            resource: "purchase_order".into(),
            model: None,
            as_of: Instant::now(),
        };
        let mut task = task();
        for assignee in [Some("buyer"), Some("new-buyer"), None] {
            task.owner_user_id = assignee.map(str::to_owned);
            assert!(AuthorizationRegistry::new(vec![registration(&task, permission.clone())]).is_ok());
            let AccessMode::Object { data, facts } = registration(&task, permission.clone()).access else {
                panic!("需要对象授权")
            };
            assert!(matches!(data.subject, AccessSubject::Source(_)));
            let AccessPolicy::Related { resource, policy, .. } = data.policy else {
                panic!("必须沿真实来源任务授权")
            };
            assert_eq!(resource, "work_item");
            assert!(matches!(*policy, AccessPolicy::Assigned));
            let targets = ResourceTargets::new(&["task"]).unwrap();
            let objects = facts.load(&context, &targets, &mut NoTransaction).await.unwrap();
            assert_eq!(objects.len(), 1);
            assert_eq!(objects[0].id, task.base.id);
            assert_eq!(objects[0].version, task.base.version.to_string());
            let related = objects[0].relations.get(&relation()).unwrap();
            assert_eq!(related.expected_ids, [task.base.id.clone()].into());
            assert_eq!(related.objects[0].resource, "work_item");
            assert_eq!(related.objects[0].assignee_id, FactValue::Known(assignee.map(str::to_owned)));
        }
    }
}
