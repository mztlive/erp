//! 结算单来源字段事实，不解释授权条款。

use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, QueryModel, QueryModelKey, QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{AccessMapping, Error as IdentityError, Result as IdentityResult};
use erp_supply::repository::SupplierSettlementExt;
use mongodb::Database;
use mongodb::bson::{Bson, doc};

/// 结算单当前准备人与业务组织的固定字段映射。
pub(in crate::finance::funds_scope) struct SettlementMapping {
    model: QueryModel,
}

impl SettlementMapping {
    // 初始化结算单来源模型，供结算应付来源使用。
    //
    // 固定集合、对象粒度及已支持事实维度的映射。
    //
    // 固定模型标识无效时返回接入错误。
    pub(in crate::finance::funds_scope) fn new() -> IdentityResult<Self> {
        Ok(Self {
            model: QueryModel {
                key: QueryModelKey::parse("funds_settlement_source")?,
                resource: "supplier_settlement_statement".into(),
                collection: <Database as SupplierSettlementExt>::SUPPLIER_SETTLEMENT_STATEMENTS.into(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: [FactKind::Owner, FactKind::Organization].into(),
            },
        })
    }
}

#[async_trait]
impl AccessMapping for SettlementMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(doc! { "deleted_at": 0 }))
    }
    fn predicate(&self, fact: &AccessFact, context: &AccessContext) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(match fact {
            AccessFact::Owned => doc! { "prepared_by": context.actor.id() },
            AccessFact::OrganizationIn(ids) => {
                doc! { "business_org_unit_id": { "$in": ids.iter().collect::<Vec<_>>() } }
            },
            _ => return Err(IdentityError::ValidationError("结算单不支持该授权事实".into())),
        }))
    }
    fn relation(&self, _: &RelationKey, _: &AccessContext) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("结算来源不存在下级授权关系".into()))
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::Permission;

    use super::*;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("preparer".into(), "经办人".into(), AccountKind::Admin),
            permission: Permission::parse("supplier_settlement_statement:list").unwrap(),
            resource: "supplier_settlement_statement".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn owner_and_organization_translate_to_current_settlement_fields() {
        let mapping = SettlementMapping::new().unwrap();
        for (fact, expected) in [
            (AccessFact::Owned, doc! { "prepared_by": "preparer" }),
            (
                AccessFact::OrganizationIn(["org".into()].into()),
                doc! { "business_org_unit_id": { "$in": ["org"] } },
            ),
        ] {
            let QueryFragment::Match(actual) = mapping.predicate(&fact, &context()).unwrap() else {
                panic!("结算单责任事实应使用字段条件");
            };
            assert_eq!(actual, expected);
        }

        assert_eq!(mapping.key(&context()).unwrap(), Bson::String("$id".into()));
        assert!(
            mapping.predicate(&AccessFact::WarehouseIn(["warehouse".into()].into()), &context()).is_err()
        );
        assert!(mapping.relation(&RelationKey::parse("other").unwrap(), &context()).is_err());
    }
}
