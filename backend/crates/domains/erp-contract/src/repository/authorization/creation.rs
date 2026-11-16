//! 只构造真实合同归属及精确来源引用；现存客户由 identity 在完整资格通过后读取。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{FactValue, ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{OrganizationDirectoryService, ResourceTargets};
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::ports::ContractCreationPort;

/// 连接拟写入负责销售的真实组织读取器，不预读客户。
/// # 参数
/// db 为组织目录数据库；客户 Facts 已绑定到共享 identity 服务。
/// # 返回
/// 在完整创建准备资格之后使用的候选端口。
/// # 错误
/// 构造不执行 I/O；读取时保留组织事实错误。
pub fn creation_facts(db: Database) -> Arc<dyn ContractCreationPort> {
    Arc::new(CreationFacts(OrganizationDirectoryService::new(db)))
}

struct CreationFacts(OrganizationDirectoryService);

#[async_trait]
impl ContractCreationPort for CreationFacts {
    async fn candidate(
        &self,
        actor: &AuditActor,
        customer_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ObjectFacts> {
        let customer = ResourceTargets::new(&[customer_id])?;
        let (organization, valid_until) =
            self.0.own_org_with_validity(actor.id(), Instant::now(), executor).await?;
        candidate(actor, &customer, organization, valid_until)
    }
}

fn candidate(
    actor: &AuditActor,
    customer: &ResourceTargets,
    organization: Option<String>,
    valid_until: Option<Instant>,
) -> Result<ObjectFacts> {
    Ok(ObjectFacts {
        resource: "contract".into(),
        owner_id: FactValue::Known(Some(actor.id().into())),
        org_unit_id: FactValue::Known(organization),
        collaborators: FactValue::Known(BTreeSet::new()),
        history_participants: FactValue::Known(BTreeSet::new()),
        valid_until,
        relations: [(
            RelationKey::parse("customer")?,
            RelatedObjects { expected_ids: customer.ids().clone(), objects: vec![] },
        )]
        .into(),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use persistence_core::NoTransaction;

    use super::*;
    use crate::Error;

    #[test]
    fn candidate_preserves_actual_owner_organization_boundary_and_only_the_customer_reference() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let customer = ResourceTargets::new(&["customer"]).unwrap();
        let until = Instant::from_unix_secs(100);
        let facts = candidate(&actor, &customer, Some("actor-org".into()), Some(until)).unwrap();
        assert_eq!(facts.owner_id, FactValue::Known(Some("actor".into())));
        assert_eq!(facts.org_unit_id, FactValue::Known(Some("actor-org".into())));
        assert_eq!(facts.owner_ids, FactValue::Missing);
        assert_eq!(facts.org_unit_ids, FactValue::Missing);
        assert_eq!(facts.valid_until, Some(until));
        assert!(facts.id.is_empty() && facts.version.is_empty());
        let source = &facts.relations[&RelationKey::parse("customer").unwrap()];
        assert_eq!(source.expected_ids, ["customer".into()].into());
        assert!(source.objects.is_empty());
        let unassigned = candidate(&actor, &customer, None, None).unwrap();
        assert_eq!(unassigned.org_unit_id, FactValue::Known(None));
        assert_eq!(unassigned.valid_until, None);
    }

    #[tokio::test]
    async fn malformed_customer_reference_is_rejected_before_organization_reads() {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("candidate_reference_unit");
        let port = creation_facts(db);
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        assert!(matches!(port.candidate(&actor, "", &mut NoTransaction).await, Err(Error::Logic(_))));
    }
}
