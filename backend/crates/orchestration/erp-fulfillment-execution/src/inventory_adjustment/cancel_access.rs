//! 普通撤回的固定身份政策与真实库存来源事实。

use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactKind, FactValue, ObjectFacts, ReadAccess, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use erp_inventory::InventoryExt;
use erp_warehouse::WarehouseExt;
use erp_workflow::entity::approval_integration::ApprovalSubjectSnapshot;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;

struct CancelFacts {
    db: Database,
    snapshot: ApprovalSubjectSnapshot,
}

#[async_trait]
impl AccessFactsPort for CancelFacts {
    async fn load(
        &self,
        context: &AccessContext,
        _: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let snapshot = &self.snapshot;
        let mut sources = Vec::new();
        if let Some(adjustment) =
            self.db.inventory().stock_adjustment(&snapshot.business_object_id, executor).await?
            && self.db.warehouse().warehouse(adjustment.warehouse_id.as_ref(), executor).await?.is_some()
        {
            sources.push(ObjectFacts {
                resource: "stock_adjustment".into(),
                id: adjustment.base.id,
                version: adjustment.base.version.to_string(),
                warehouse_id: FactValue::Known(Some(adjustment.warehouse_id.to_string())),
                ..Default::default()
            });
        }
        Ok(vec![ObjectFacts {
            resource: context.resource.clone(),
            id: snapshot.approval_process_instance_id.to_string(),
            version: snapshot.base.version.to_string(),
            owner_id: FactValue::Known(Some(snapshot.payload.submitted_by.clone())),
            relations: [(
                RelationKey::parse("stock_adjustment")?,
                RelatedObjects {
                    expected_ids: [snapshot.business_object_id.clone()].into(),
                    objects: sources,
                },
            )]
            .into(),
            ..Default::default()
        }])
    }
}

fn registration(facts: Arc<dyn AccessFactsPort>) -> erp_identity::Result<AccessRegistration> {
    let detail = Permission::parse("stock_adjustment:detail")?;
    Ok(AccessRegistration {
        permission: Permission::parse("approval_instance:cancel")?,
        required_permissions: vec![Permission::parse("approval_instance:read")?],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner].into(),
                policy: AccessPolicy::Any(vec![
                    AccessPolicy::Owned,
                    AccessPolicy::All(vec![
                        AccessPolicy::Permission(Permission::parse(
                            "stock_adjustment:approval_runtime_admin",
                        )?),
                        AccessPolicy::Related {
                            relation: RelationKey::parse("stock_adjustment")?,
                            resource: "stock_adjustment".into(),
                            mode: RelationMatch::AllNonEmpty,
                            required_facts: [FactKind::Warehouse].into(),
                            policy: Box::new(AccessPolicy::Independent {
                                permission: detail.clone(),
                                policy: Box::new(AccessPolicy::Scope {
                                    permission: detail,
                                    read: ReadAccess::default(),
                                }),
                            }),
                        },
                    ]),
                ]),
            },
            facts,
        },
    })
}

/// 根操作、冻结提交人或管理员及其真实来源读取资格统一交给 identity。
/// # 参数
/// db、identity 为原业务设施；actor、snapshot 和 executor 为原撤回事务上下文。
/// # 返回
/// 完整操作和对象资格成立时成功。
/// # 错误
/// 账号、权限、所有者或来源资格不满足时拒绝；事实读取错误透传。
pub(super) async fn require_cancel(
    db: &Database,
    identity: &SharedRbacService,
    actor: &AuditActor,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<()> {
    let entry = registration(Arc::new(CancelFacts { db: db.clone(), snapshot: snapshot.clone() }))?;
    let permission = entry.permission.clone();
    let registry = AuthorizationRegistry::new(vec![entry])?;
    let id: &str = snapshot.approval_process_instance_id.as_ref();
    Ok(AuthorizationService::new(db.clone(), identity.clone(), registry)
        .require(
            actor,
            &permission,
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
            executor,
        )
        .await?)
}
