//! 审批管理入口及其全部功能资格，沿真实财务根和全部实际来源统一授权。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, FactKind, FactValue, ObjectFacts, ReadAccess, RelatedObjects, RelationKey, RelationMatch,
};
use erp_identity::entity::policy_permission::FINANCE_LEDGER_READ;
use erp_identity::{
    AccessFactsPort, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Error as IdentityError, Permission,
    ResourceTargets, Result as IdentityResult, SharedRbacService,
};
use erp_procurement::authorization::PurchaseFacts;
use erp_supply::repository::SupplierSettlementExt;
use erp_supply::repository::prelude::*;
use mongodb::Database;
use persistence_core::Executor;

use super::super::FundsAccess;
use super::management_facts::{self, ManagementFacts};
use crate::sales_center::access::approval_source_permissions;
use crate::sales_center::repository::authorization::SalesFacts;
use crate::{Error, Result};

impl FundsAccess {
    /// 用同次 identity 检查证明管理入口、全部附加权限及真实财务根的全部来源。
    /// # 参数
    /// actor 为真实账号，resource/id 为服务端关系解析得到的财务根；entry/required 为固定管理目录。
    /// executor 为原事务执行器。
    /// # 返回
    /// 完整根及全部实际来源获准时成功；真实零分配沿财务整账 Operation 检查。
    /// # 错误
    /// 未登记入口、来源缺损、功能资格失效或任一实际来源无权时整体拒绝。
    pub async fn require_source_operation(
        &self,
        actor: &AuditActor,
        resource: &str,
        id: &str,
        entry: &Permission,
        required: &[Permission],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        ensure_entry(entry, required)?;
        let mut required = required.to_vec();
        if matches!(resource, "customer_receipt" | "supplier_payment") {
            required.push(Permission::parse(FINANCE_LEDGER_READ)?);
        }
        let facts = management_facts::load(&self.db, resource, id, executor).await?;
        if partition(&facts) == 0 {
            return zero_source(&self.db, &self.rbac, actor, &facts, entry, required, executor).await;
        }
        let subject = AccessSubject::Source(RelationKey::parse("finance_root")?);
        let access = AccessMode::Object {
            data: DataAccess {
                subject: subject.clone(),
                required_facts: BTreeSet::new(),
                policy: root_policy(&facts)?,
            },
            facts: Arc::new(ManagementPort {
                db: self.db.clone(),
                resource: resource.into(),
                partition: partition(&facts),
            }),
        };
        let service = AuthorizationService::new(
            self.db.clone(),
            self.rbac.clone(),
            AuthorizationRegistry::new(vec![AccessRegistration {
                permission: entry.clone(),
                required_permissions: required,
                access,
            }])?,
        );
        service
            .require(
                actor,
                entry,
                AccessTarget::Records { subject, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?;
        Ok(())
    }
}

fn ensure_entry(entry: &Permission, required: &[Permission]) -> Result<()> {
    if matches!(
        entry.action(),
        "read" | "decide" | "resume" | "cancel" | "cancel_blocked" | "upgrade_binding"
    ) && !required.is_empty()
        && approval_source_permissions(entry.resource(), entry.action(), required)?
    {
        return Ok(());
    }
    Err(Error::ValidationError("未登记的财务审批管理完整权限组合".into()))
}

fn groups(facts: &ManagementFacts) -> [(&'static str, &'static str, &BTreeSet<String>); 3] {
    [
        ("sales_order", "sales", &facts.sales_ids),
        ("purchase_order", "purchase", &facts.purchase_ids),
        ("supplier_settlement_statement", "settlement", &facts.settlement_ids),
    ]
}

fn partition(facts: &ManagementFacts) -> u8 {
    groups(facts)
        .iter()
        .enumerate()
        .fold(0, |mask, (index, (_, _, ids))| mask | if ids.is_empty() { 0 } else { 1 << index })
}

fn root_policy(facts: &ManagementFacts) -> Result<AccessPolicy> {
    let policies = groups(facts)
        .into_iter()
        .filter(|(_, _, ids)| !ids.is_empty())
        .map(|(resource, relation, _)| {
            Ok(AccessPolicy::Related {
                relation: RelationKey::parse(relation)?,
                resource: resource.into(),
                mode: RelationMatch::AllNonEmpty,
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                policy: Box::new(AccessPolicy::Scope {
                    permission: Permission::parse(format!(
                        "{resource}:{}",
                        if resource == "supplier_settlement_statement" { "list" } else { "detail" }
                    ))?,
                    read: ReadAccess {
                        history: false,
                        records: matches!(resource, "sales_order" | "purchase_order"),
                    },
                }),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(AccessPolicy::Related {
        relation: RelationKey::parse("finance_root")?,
        resource: facts.root.resource.clone(),
        mode: RelationMatch::AllNonEmpty,
        required_facts: BTreeSet::new(),
        policy: Box::new(AccessPolicy::All(policies)),
    })
}

struct ManagementPort {
    db: Database,
    resource: String,
    partition: u8,
}

#[async_trait]
impl AccessFactsPort for ManagementPort {
    async fn load(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let mut result = Vec::new();
        for id in targets.ids() {
            let mut facts = management_facts::load(&self.db, &self.resource, id, executor)
                .await
                .map_err(identity_error)?;
            if partition(&facts) != self.partition {
                return Err(IdentityError::ConflictError("DATA_SCOPE_CHANGED：财务来源结构已变化".into()));
            }
            let groups = groups(&facts).map(|(resource, relation, ids)| (resource, relation, ids.clone()));
            for (resource, relation, ids) in groups {
                if ids.is_empty() {
                    continue;
                }
                let objects = source_objects(&self.db, context, resource, &ids, executor).await?;
                facts.root.relations.insert(
                    RelationKey::parse(relation)?,
                    RelatedObjects { expected_ids: ids.clone(), objects },
                );
            }
            result.push(ObjectFacts {
                resource: context.resource.clone(),
                id: id.clone(),
                version: facts.root.version.clone(),
                relations: [(
                    RelationKey::parse("finance_root")?,
                    RelatedObjects { expected_ids: [id.clone()].into(), objects: vec![facts.root] },
                )]
                .into(),
                ..Default::default()
            });
        }
        Ok(result)
    }
}

async fn source_objects(
    db: &Database,
    context: &AccessContext,
    resource: &str,
    ids: &BTreeSet<String>,
    executor: &mut dyn Executor,
) -> IdentityResult<Vec<ObjectFacts>> {
    let context = AccessContext { resource: resource.into(), model: None, ..context.clone() };
    let targets = ResourceTargets::new(&ids.iter().map(String::as_str).collect::<Vec<_>>())?;
    match resource {
        "sales_order" => SalesFacts(db.clone(), false).load(&context, &targets, executor).await,
        "purchase_order" => PurchaseFacts::new(db.clone()).load(&context, &targets, executor).await,
        "supplier_settlement_statement" => settlement_objects(db, &targets, executor).await,
        _ => Err(IdentityError::ValidationError("未知财务实际来源".into())),
    }
}

async fn settlement_objects(
    db: &Database,
    ids: &ResourceTargets,
    executor: &mut dyn Executor,
) -> IdentityResult<Vec<ObjectFacts>> {
    let rows = db
        .supplier_settlement_statements()
        .find_statements_by_ids(&ids.ids().iter().cloned().collect::<Vec<_>>(), executor)
        .await?;
    Ok(rows
        .into_iter()
        .map(|row| ObjectFacts {
            resource: "supplier_settlement_statement".into(),
            id: row.base.id,
            version: row.base.version.to_string(),
            owner_id: FactValue::Known(Some(row.prepared_by)),
            org_unit_id: FactValue::Known(Some(row.business_org_unit_id)),
            ..Default::default()
        })
        .collect())
}

async fn zero_source(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    facts: &ManagementFacts,
    entry: &Permission,
    mut required: Vec<Permission>,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !matches!(facts.root.resource.as_str(), "customer_receipt" | "supplier_payment") {
        return Err(Error::NotFound("财务根缺少必要实际来源".into()));
    }
    required.push(entry.clone());
    let ledger = Permission::parse(FINANCE_LEDGER_READ)?;
    required.retain(|permission| permission != &ledger);
    let service = AuthorizationService::new(
        db.clone(),
        rbac.clone(),
        AuthorizationRegistry::new(vec![AccessRegistration {
            permission: ledger.clone(),
            required_permissions: required,
            access: AccessMode::Operation,
        }])?,
    );
    service.require(actor, &ledger, AccessTarget::Operation, executor).await?;
    let current = management_facts::load(db, &facts.root.resource, &facts.root.id, executor).await?;
    if current.root != facts.root || partition(&current) != 0 {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：财务来源结构已变化".into()));
    }
    Ok(())
}

fn identity_error(error: Error) -> IdentityError {
    match error {
        Error::Internal(v) => IdentityError::Internal(v),
        Error::NotFound(v) => IdentityError::NotFound(v),
        Error::ValidationError(v) => IdentityError::ValidationError(v),
        Error::InvalidInput(v) => IdentityError::InvalidInput(v),
        Error::BusinessLogicError(v) => IdentityError::BusinessLogicError(v),
        Error::ConflictError(v) => IdentityError::ConflictError(v),
        Error::ReceiptDuplicate(v) => IdentityError::ReceiptDuplicate(v),
        Error::TransientTransaction(v) => IdentityError::TransientTransaction(v),
        Error::Forbidden(v) => IdentityError::Forbidden(v),
        Error::Unauthenticated(v) => IdentityError::Unauthenticated(v),
        Error::Logic(v) => IdentityError::Logic(v),
        Error::Rbac(v) => IdentityError::Rbac(v),
        Error::OutcomeUnknown(v) => IdentityError::OutcomeUnknown(v),
        Error::RepositoryError(v) => IdentityError::RepositoryError(v),
        Error::Coded(v) => IdentityError::Internal(v.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use erp_workflow::DocumentType;
    use erp_workflow::service::approval::policy::{DocumentApprovalPolicy, policy_of};

    use super::*;

    fn facts() -> ManagementFacts {
        ManagementFacts {
            root: ObjectFacts {
                resource: "supplier_payment".into(),
                id: "payment".into(),
                version: "1".into(),
                ..Default::default()
            },
            sales_ids: ["same".into()].into(),
            purchase_ids: ["same".into()].into(),
            settlement_ids: ["settlement".into()].into(),
        }
    }

    #[test]
    fn every_nonempty_actual_resource_is_required_without_any_source_fallback() {
        let data = facts();
        assert_eq!(partition(&data), 7);
        let AccessPolicy::Related { resource, mode, policy, .. } = root_policy(&data).unwrap() else {
            panic!("必须先验证真实财务根");
        };
        assert_eq!(resource, "supplier_payment");
        assert_eq!(mode, RelationMatch::AllNonEmpty);
        let AccessPolicy::All(policies) = *policy else {
            panic!("全部实际来源必须同时成立");
        };
        assert_eq!(policies.len(), 3);
        for (policy, (resource, _, _)) in policies.into_iter().zip(groups(&data)) {
            let AccessPolicy::Related { resource: actual, mode, required_facts, policy, .. } = policy else {
                panic!("来源必须通过关联政策");
            };
            assert_eq!(actual, resource);
            assert_eq!(mode, RelationMatch::AllNonEmpty);
            assert_eq!(required_facts, [FactKind::Owner, FactKind::Organization].into());
            let AccessPolicy::Scope { permission, read } = *policy else {
                panic!("来源范围必须由identity执行");
            };
            assert_eq!(permission.resource(), resource);
            assert!(!read.history);
            assert_eq!(read.records, resource != "supplier_settlement_statement");
        }
    }

    #[test]
    fn empty_and_mixed_source_structure_remains_distinguishable() {
        let mut data = facts();
        data.sales_ids.clear();
        assert_eq!(partition(&data), 6);
        data.purchase_ids.clear();
        assert_eq!(partition(&data), 4);
        data.settlement_ids.clear();
        assert_eq!(partition(&data), 0);
    }

    #[test]
    fn management_catalog_rejects_other_resources_and_preserves_failure_classes() {
        let DocumentApprovalPolicy::ProcessRequired(policy) =
            policy_of(DocumentType::CustomerReceipt).unwrap()
        else {
            panic!("回款使用正式审批政策");
        };
        for action in ["read", "decide", "resume", "cancel", "cancel_blocked", "upgrade_binding"] {
            let required = [
                Permission::parse("approval_instance:read").unwrap(),
                Permission::parse(policy.runtime_admin_permission.clone()).unwrap(),
            ];
            assert!(
                ensure_entry(&Permission::parse(format!("approval_instance:{action}")).unwrap(), &required)
                    .is_ok()
            );
        }
        assert!(ensure_entry(&Permission::parse("invoice:detail").unwrap(), &[]).is_err());
        assert!(ensure_entry(&Permission::parse("approval_instance:create").unwrap(), &[]).is_err());
        assert!(matches!(identity_error(Error::NotFound("missing".into())), IdentityError::NotFound(_)));
        assert!(matches!(
            identity_error(Error::ConflictError("DATA_SCOPE_CHANGED".into())),
            IdentityError::ConflictError(_)
        ));
    }
}
