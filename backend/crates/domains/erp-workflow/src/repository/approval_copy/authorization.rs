//! 抄送独立资源使用账号资格及真实收件人事实，不要求角色功能权限。

use std::sync::Arc;

use async_trait::async_trait;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, ObjectFacts, QueryModel, QueryModelKey, QueryOutput,
    RelationKey,
};
use erp_identity::ports::{AccessFactsPort, AccessMapping, QueryFragment, RelationMapping};
use erp_identity::{Error, ResourceTargets, Result};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use crate::entity::approval_copy::ApprovalCopyReceipt;
use crate::repository::approval_copy::{ApprovalCopyExt, RECEIPTS};

pub(crate) const RESOURCE: &str = "approval_copy";
pub(crate) const MODEL: &str = "approval_copy_receipts";

pub(crate) struct CopyMapping(QueryModel);

impl CopyMapping {
    // 登记抄送读取的静态模型，只声明真实收件人及送达记录支持的事实，不参与其他审批范围解释。
    pub(crate) fn shared() -> Result<Arc<dyn AccessMapping>> {
        Ok(Arc::new(Self(QueryModel {
            key: QueryModelKey::parse(MODEL)?,
            resource: RESOURCE.into(),
            collection: RECEIPTS.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner].into(),
        })))
    }
}

#[async_trait]
impl AccessMapping for CopyMapping {
    fn model(&self) -> &QueryModel {
        &self.0
    }

    fn key(&self, _: &AccessContext) -> Result<Bson> {
        Ok("$id".into())
    }

    fn records(&self, _: &AccessContext) -> Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {
            "delivered": true,
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        }))
    }

    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> Result<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! { "recipient_user_id": context.actor.id() })),
            _ => Err(Error::ValidationError("抄送仅提供真实收件人事实".into())),
        }
    }

    fn relation(&self, _: &RelationKey, _: &AccessContext) -> Result<RelationMapping> {
        Err(Error::ValidationError("抄送不继承当前业务单据范围".into()))
    }
}

pub(crate) struct CopyFacts(pub(crate) Database);

#[async_trait]
impl AccessFactsPort for CopyFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        let mut facts = Vec::new();
        for id in ids.ids() {
            if let Some(receipt) = self.0.approval_copies().receipt(id, executor).await?
                && let Some(fact) = receipt_fact(receipt)
            {
                facts.push(fact);
            }
        }
        Ok(facts)
    }
}

// 送达的抄送记录才提供完整收件人事实；待送达或未送达记录不能作为可读根对象。
fn receipt_fact(receipt: ApprovalCopyReceipt) -> Option<ObjectFacts> {
    receipt.delivered.then(|| ObjectFacts {
        resource: RESOURCE.into(),
        id: receipt.base.id,
        version: receipt.base.version.to_string(),
        owner_id: FactValue::Known(Some(receipt.recipient_user_id)),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use entity_core::BaseModel;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_permissions::internal;

    use super::*;
    use crate::entity::approval_copy::CopyTrigger;
    use crate::entity::document_registry::DocumentType;

    fn receipt(delivered: bool) -> ApprovalCopyReceipt {
        let mut base = BaseModel::fake();
        base.id = "copy".into();
        ApprovalCopyReceipt {
            base,
            instance_id: "instance".into(),
            snapshot_id: "snapshot".into(),
            subject_version: 1,
            document_type: DocumentType::StockAdjustment,
            document_no: "ADJ-1".into(),
            submitter_name: None,
            event_key: "started".into(),
            trigger: CopyTrigger::Started,
            node_name: None,
            round_no: 1,
            recipient_user_id: "recipient".into(),
            recipient_name: "收件人".into(),
            delivered,
            undelivered_reason: None,
            copied_at: 1,
            read_at: None,
        }
    }

    #[test]
    fn only_delivered_receipts_supply_the_actual_recipient_as_owner() {
        let fact = receipt_fact(receipt(true)).unwrap();
        assert_eq!(fact.id, "copy");
        assert_eq!(fact.resource, RESOURCE);
        assert_eq!(fact.owner_id, FactValue::Known(Some("recipient".into())));
        assert!(receipt_fact(receipt(false)).is_none());
    }

    #[test]
    fn ownership_mapping_uses_the_frozen_account_and_rejects_other_dimensions() {
        let mapping = CopyMapping(QueryModel {
            key: QueryModelKey::parse(MODEL).unwrap(),
            resource: RESOURCE.into(),
            collection: RECEIPTS.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Owner].into(),
        });
        let context = AccessContext {
            actor: AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin),
            permission: internal::APPROVAL_COPY_LIST.clone(),
            resource: RESOURCE.into(),
            model: Some(QueryModelKey::parse(MODEL).unwrap()),
            as_of: Instant::now(),
        };

        let QueryFragment::Match(filter) = mapping.predicate(&AccessFact::Owned, &context).unwrap() else {
            panic!("收件人应映射为实际字段条件");
        };
        assert_eq!(filter, doc! { "recipient_user_id": "reader" });

        assert!(mapping.predicate(&AccessFact::AssignedTo, &context).is_err());
    }
}
