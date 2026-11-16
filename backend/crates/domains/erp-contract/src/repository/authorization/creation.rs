//! 从拥有领域提供的真实客户事实构造合同创建候选。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, FactValue, ObjectFacts, RelatedObjects, RelationKey,
};
use erp_identity::{AccessFactsPort, OrganizationDirectoryService, ResourceTargets};
use erp_permissions::contract;
use mongodb::Database;
use persistence_core::Executor;

use crate::ports::ContractCreationPort;
use crate::{Error, Result};

/// 为合同创建连接同一组合根提供的客户事实读取器。
/// # 参数
/// db 用于读取拟写入负责销售的主属组织，customers 为共享的真实客户事实实例。
/// # 返回
/// 在原事务中校验客户身份并转换为合同候选的窄端口。
/// # 错误
/// 构造不执行 I/O；读取时的缺失、错配和设施错误保持失败。
pub fn creation_facts(db: Database, customers: Arc<dyn AccessFactsPort>) -> Arc<dyn ContractCreationPort> {
    Arc::new(CreationFacts { customers, organizations: OrganizationDirectoryService::new(db) })
}

/// 合同归属与客户来源保持独立；客户来源的授权事实由 identity 在原事务重新取证。
struct CreationFacts {
    customers: Arc<dyn AccessFactsPort>,
    organizations: OrganizationDirectoryService,
}
#[async_trait]
impl ContractCreationPort for CreationFacts {
    async fn candidate(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFacts> {
        let at = Instant::now();
        let customer = load_customer(self.customers.as_ref(), actor, customer_id, at, executor).await?;
        let (organization, valid_until) =
            self.organizations.own_org_with_validity(actor.id(), at, executor).await?;
        candidate(actor, &customer, organization, valid_until)
    }
}

async fn load_customer(
    customers: &dyn AccessFactsPort,
    actor: &AuditActor,
    customer_id: &str,
    at: Instant,
    executor: &mut dyn Executor,
) -> Result<ObjectFacts> {
    let context = AccessContext {
        actor: actor.clone(),
        permission: contract::CREATE,
        resource: "customer".into(),
        model: None,
        as_of: at,
    };
    let ids = ResourceTargets::new(&[customer_id])?;
    let mut rows = customers.load(&context, &ids, executor).await?;
    if rows.len() != 1 || rows[0].resource != "customer" || rows[0].id != customer_id {
        return Err(Error::NotFound("客户不存在或无权访问".into()));
    }
    Ok(rows.remove(0))
}

/// 与 Contract::new 相同的独立负责销售；不从客户多负责人中选取合同归属。
fn candidate(
    actor: &AuditActor,
    customer: &ObjectFacts,
    organization: Option<String>,
    organization_until: Option<Instant>,
) -> Result<ObjectFacts> {
    Ok(ObjectFacts {
        resource: "contract".into(),
        owner_id: FactValue::Known(Some(actor.id().into())),
        org_unit_id: FactValue::Known(organization),
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(BTreeSet::new()),
        valid_until: customer.valid_until.into_iter().chain(organization_until).min(),
        relations: [(
            RelationKey::parse("customer")?,
            RelatedObjects { expected_ids: [customer.id.clone()].into(), objects: vec![] },
        )]
        .into(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::{AccountKind, Error as CoreError};
    use erp_identity::{Error as IdentityError, Result as IdentityResult};
    use persistence_core::NoTransaction;

    use super::*;

    struct Customers {
        rows: Vec<ObjectFacts>,
        calls: Mutex<Vec<Vec<String>>>,
        unavailable: bool,
    }

    #[async_trait]
    impl AccessFactsPort for Customers {
        async fn load(
            &self,
            context: &AccessContext,
            ids: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> IdentityResult<Vec<ObjectFacts>> {
            assert_eq!(context.permission, contract::CREATE);
            assert_eq!(context.resource, "customer");
            assert_eq!(context.actor.id(), "actor");
            self.calls.lock().unwrap().push(ids.ids().iter().cloned().collect());
            if self.unavailable {
                return Err(IdentityError::ConflictError("客户归属已变化".into()));
            }
            Ok(self.rows.clone())
        }
    }

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin)
    }

    fn customer() -> ObjectFacts {
        ObjectFacts {
            resource: "customer".into(),
            id: "customer".into(),
            version: "7".into(),
            owner_ids: FactValue::Known(["owner-a".into(), "owner-b".into()].into()),
            org_unit_ids: FactValue::Known(["org-a".into(), "org-b".into()].into()),
            collaborators: FactValue::Known(["collaborator".into()].into()),
            history_participants: FactValue::Known(["historical".into()].into()),
            valid_until: Some(Instant::from_unix_secs(100)),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn creation_uses_actor_ownership_and_preserves_exact_customer_reference_and_earliest_expiry() {
        let customers =
            Arc::new(Customers { rows: vec![customer()], calls: Mutex::new(Vec::new()), unavailable: false });
        assert!(customers.calls.lock().unwrap().is_empty());
        let source =
            load_customer(customers.as_ref(), &actor(), "customer", Instant::now(), &mut NoTransaction)
                .await
                .unwrap();
        let candidate = candidate(&actor(), &source, Some("actor-org".into()), None).unwrap();
        assert_eq!(*customers.calls.lock().unwrap(), vec![vec!["customer".to_owned()]]);
        assert_eq!(candidate.resource, "contract");
        assert!(candidate.id.is_empty());
        assert!(candidate.version.is_empty());
        assert_eq!(candidate.owner_id, FactValue::Known(Some("actor".into())));
        assert_eq!(candidate.org_unit_id, FactValue::Known(Some("actor-org".into())));
        assert_eq!(candidate.owner_ids, FactValue::Missing);
        assert_eq!(candidate.org_unit_ids, FactValue::Missing);
        let related = &candidate.relations[&RelationKey::parse("customer").unwrap()];
        assert_eq!(related.expected_ids, ["customer".into()].into());
        assert!(related.objects.is_empty());
        assert_eq!(candidate.valid_until, customer().valid_until);
        assert_eq!(candidate.collaborators, FactValue::Known(BTreeSet::new()));
        assert_eq!(candidate.history_participants, FactValue::Known(BTreeSet::new()));
        let earlier = Instant::from_unix_secs(50);
        let candidate = super::candidate(&actor(), &source, None, Some(earlier)).unwrap();
        assert_eq!(candidate.valid_until, Some(earlier));
        assert_eq!(candidate.org_unit_id, FactValue::Known(None));
    }

    #[tokio::test]
    async fn creation_rejects_missing_duplicate_foreign_or_wrong_customer_identity() {
        let mut foreign = customer();
        foreign.resource = "supplier".into();
        let mut wrong = customer();
        wrong.id = "other".into();
        for rows in [vec![], vec![customer(), customer()], vec![foreign], vec![wrong]] {
            let customers = Customers { rows, calls: Mutex::new(Vec::new()), unavailable: false };
            assert!(matches!(
                load_customer(&customers, &actor(), "customer", Instant::now(), &mut NoTransaction).await,
                Err(Error::NotFound(_))
            ));
        }
    }

    #[tokio::test]
    async fn creation_preserves_source_failure_instead_of_returning_an_empty_candidate() {
        let customers =
            Arc::new(Customers { rows: vec![], calls: Mutex::new(Vec::new()), unavailable: true });
        assert!(matches!(
            load_customer(customers.as_ref(), &actor(), "customer", Instant::now(), &mut NoTransaction).await,
            Err(Error::ConflictError(message)) if message == "客户归属已变化"
        ));
        assert!(matches!(
            load_customer(customers.as_ref(), &actor(), "", Instant::now(), &mut NoTransaction).await,
            Err(Error::Logic(CoreError::LogicError(message))) if message == "目标不能为空"
        ));
        assert_eq!(customers.calls.lock().unwrap().len(), 1);
    }
}
