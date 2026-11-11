//! 库存审批管理以完整操作资格和真实调整单来源执行同一次 identity 检查。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactKind, FactValue, ObjectFacts, ReadAccess, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Error, Permission, ResourceTargets, Result,
    SharedRbacService,
};
use mongodb::Database;
use persistence_core::Executor;

use crate::StockAdjustment;
use crate::repository::InventoryExt;

struct StockSourceFacts(Database);
#[async_trait]
impl AccessFactsPort for StockSourceFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut facts = Vec::new();
        for id in ids.ids() {
            let Some(adjustment) = self.0.inventory().stock_adjustment(id, executor).await? else { continue };
            facts.push(stock_source(
                context,
                adjustment.base.id,
                adjustment.base.version.to_string(),
                adjustment.warehouse_id.to_string(),
            )?);
        }
        Ok(facts)
    }
}

struct DirectStockFacts(Database);
#[async_trait]
impl AccessFactsPort for DirectStockFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut facts = Vec::new();
        for id in ids.ids() {
            let Some(adjustment) = self.0.inventory().stock_adjustment(id, executor).await? else { continue };
            facts.push(submit_fact(adjustment));
        }
        Ok(facts)
    }
}

fn submit_fact(adjustment: StockAdjustment) -> ObjectFacts {
    ObjectFacts {
        resource: "stock_adjustment".into(),
        id: adjustment.base.id,
        version: adjustment.base.version.to_string(),
        warehouse_id: FactValue::Known(Some(adjustment.warehouse_id.to_string())),
        assignee_id: FactValue::Known(Some(adjustment.prepared_by)),
        ..Default::default()
    }
}

/// 提交按实际写动作的仓库范围检查，读取范围不能替代提交资格。
/// # 参数
/// db、rbac 为库存身份设施，actor 为实际提交人，id 为真实调整单，executor 为原写事务。
/// # 返回
/// 同角色提交与详情功能资格、submit 数据范围及实际仓储经办人资格同时成立时成功。
/// # 错误
/// 账号、完整功能、目标或写范围不足拒绝；事实与数据库错误透传。
pub async fn require_submit(
    db: Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let permission = Permission::parse("stock_adjustment:submit")?;
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: vec![Permission::parse("stock_adjustment:detail")?],
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Warehouse, FactKind::Assignee].into(),
                policy: AccessPolicy::All(vec![
                    AccessPolicy::Scope { permission: permission.clone(), read: ReadAccess::default() },
                    AccessPolicy::Assigned,
                ]),
            },
            facts: Arc::new(DirectStockFacts(db.clone())),
        },
    }])?;
    AuthorizationService::new(db, rbac, registry)
        .require(
            actor,
            &permission,
            AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
            executor,
        )
        .await
}

fn stock_source(
    context: &AccessContext,
    id: String,
    version: String,
    warehouse: String,
) -> Result<ObjectFacts> {
    let source = ObjectFacts {
        resource: "stock_adjustment".into(),
        id: id.clone(),
        version: version.clone(),
        warehouse_id: FactValue::Known(Some(warehouse)),
        ..Default::default()
    };
    Ok(ObjectFacts {
        resource: context.resource.clone(),
        id: id.clone(),
        version,
        relations: [(
            RelationKey::parse("stock_adjustment")?,
            RelatedObjects { expected_ids: [id].into(), objects: vec![source] },
        )]
        .into(),
        ..Default::default()
    })
}

/// 同一有效角色的完整管理资格与该角色可读的实际调整单来源同时成立。
/// # 参数
/// db、rbac 为原业务设施，actor 为真实管理账号，id 为实际调整单；entry、required 来自工作流固定政策，executor 为原事务。
/// # 返回
/// 全部必要资格及来源数据范围在同一次 identity require 中证明时成功。
/// # 错误
/// 非固定管理组合、账号、完整权限、来源缺失或越界均拒绝；基础设施错误透传。
pub async fn require_management_source(
    db: Database,
    rbac: SharedRbacService,
    actor: &AuditActor,
    id: &str,
    entry: &Permission,
    required: &[Permission],
    executor: &mut dyn Executor,
) -> Result<()> {
    let relation = RelationKey::parse("stock_adjustment")?;
    let registry = management_registry(db.clone(), entry, required, relation.clone())?;
    AuthorizationService::new(db, rbac, registry)
        .require(
            actor,
            entry,
            AccessTarget::Records {
                subject: AccessSubject::Source(relation),
                ids: ResourceTargets::new(&[id])?,
            },
            executor,
        )
        .await
}

fn management_registry(
    db: Database,
    entry: &Permission,
    required: &[Permission],
    relation: RelationKey,
) -> Result<AuthorizationRegistry> {
    let required_permissions = management_permissions(entry, required)?;
    AuthorizationRegistry::new(vec![AccessRegistration {
        permission: entry.clone(),
        required_permissions,
        access: AccessMode::Object {
            data: DataAccess {
                subject: AccessSubject::Source(relation.clone()),
                required_facts: BTreeSet::new(),
                policy: AccessPolicy::Related {
                    relation,
                    resource: "stock_adjustment".into(),
                    mode: RelationMatch::AllNonEmpty,
                    required_facts: [FactKind::Warehouse].into(),
                    policy: Box::new(AccessPolicy::Scope {
                        permission: Permission::parse("stock_adjustment:detail")?,
                        read: ReadAccess::default(),
                    }),
                },
            },
            facts: Arc::new(StockSourceFacts(db)),
        },
    }])
}

fn management_permissions(entry: &Permission, required: &[Permission]) -> Result<Vec<Permission>> {
    if entry.resource() != "approval_instance"
        || !matches!(
            entry.action(),
            "read" | "decide" | "resume" | "cancel" | "cancel_blocked" | "upgrade_binding"
        )
    {
        return Err(Error::ValidationError("未登记的库存审批管理动作".into()));
    }
    let administration = if entry.action() == "upgrade_binding" {
        "stock_adjustment:approval_definition_admin"
    } else {
        "stock_adjustment:approval_runtime_admin"
    };
    let expected =
        [entry.clone(), Permission::parse("approval_instance:read")?, Permission::parse(administration)?]
            .into_iter()
            .collect::<BTreeSet<_>>();
    if required.iter().cloned().collect::<BTreeSet<_>>() != expected {
        return Err(Error::ValidationError("库存审批管理必须使用固定完整权限组合".into()));
    }
    Ok(expected.into_iter().collect())
}

/// 机械保留统一身份授权错误分类。
pub(crate) fn map_identity_error(error: Error) -> crate::Error {
    match error {
        Error::Internal(value) | Error::Rbac(value) => crate::Error::Internal(value),
        Error::NotFound(value) => crate::Error::NotFound(value),
        Error::ValidationError(value) => crate::Error::ValidationError(value),
        Error::InvalidInput(value) => crate::Error::InvalidInput(value),
        Error::BusinessLogicError(value) => crate::Error::BusinessLogicError(value),
        Error::ConflictError(value) => crate::Error::ConflictError(value),
        Error::ReceiptDuplicate(value) => crate::Error::ReceiptDuplicate(value),
        Error::TransientTransaction(value) => crate::Error::TransientTransaction(value),
        Error::Forbidden(value) => crate::Error::Forbidden(value),
        Error::Unauthenticated(value) => crate::Error::Unauthenticated(value),
        Error::Logic(value) => crate::Error::Logic(value),
        Error::OutcomeUnknown(value) => crate::Error::OutcomeUnknown(value),
        Error::RepositoryError(value) => crate::Error::RepositoryError(value),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fixed_management_combinations_reject_partial_or_external_requirements() {
        let entry = Permission::parse("approval_instance:cancel").unwrap();
        let required = [
            entry.clone(),
            Permission::parse("approval_instance:read").unwrap(),
            Permission::parse("stock_adjustment:approval_runtime_admin").unwrap(),
        ];
        assert!(management_permissions(&entry, &required).is_ok());
        assert!(management_permissions(&entry, &required[..2]).is_err());
        assert!(
            management_permissions(&Permission::parse("stock_adjustment:update").unwrap(), &required)
                .is_err()
        );
    }
    #[test]
    fn submit_assignment_uses_actual_warehouse_handler_instead_of_creator() {
        use erp_core::ids::WarehouseId;

        use crate::{AdjustmentReasonType, StockAdjustmentData, StockAdjustmentId};
        let adjustment = StockAdjustment::new(
            StockAdjustmentId::new("adjustment"),
            StockAdjustmentData {
                adjustment_no: "ADJ-1".into(),
                warehouse_id: WarehouseId::new("warehouse"),
                reason_type: AdjustmentReasonType::StockGain,
                prepared_by: "handler".into(),
                note: None,
                occurred_at: None,
            },
            "different-creator",
        )
        .unwrap();
        let fact = submit_fact(adjustment);
        assert_eq!(fact.assignee_id, FactValue::Known(Some("handler".into())));
        assert_eq!(fact.warehouse_id, FactValue::Known(Some("warehouse".into())));
    }
}
